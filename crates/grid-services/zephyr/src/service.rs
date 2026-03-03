use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use grid_core::ProgramId;
use grid_programs_zephyr::{
    FinalityCertificate, Nullifier, ValidatorInfo, ZephyrGlobalDescriptor, ZephyrGlobalMessage,
    ZephyrSpendDescriptor, ZephyrValidatorDescriptor, ZephyrZoneDescriptor, ZephyrZoneMessage,
};
use grid_service::{
    ConfigField, ConfigFieldType, OwnedProgram, RouteInfo, Service, ServiceContext,
    ServiceDescriptor, ServiceError, ServiceGossipHandler,
};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::committee::{my_assigned_zones, sample_committee};
use crate::config::ZephyrConfig;
use crate::consensus::{ConsensusAction, ZoneConsensus};
use crate::epoch::EpochManager;
use crate::gossip::ZephyrGossipHandler;
use crate::shared_mempool::SharedMempool;
use crate::storage::zone_head::ZoneHead;

/// Summary of a finalized block for the metrics / dashboard feed.
struct BlockSummary {
    zone_id: u32,
    block_hash_hex: String,
    height: u64,
    tx_nullifiers: Vec<String>,
}

const MAX_RECENT_BLOCKS: usize = 100;
const MAX_BLOCK_TX_CACHE: usize = 200;

/// Live metrics snapshot shared between the consensus task and HTTP handlers.
pub(crate) struct ZephyrRuntime {
    pub zone_heads: HashMap<u32, [u8; 32]>,
    pub current_epoch: u64,
    pub epoch_progress_pct: f32,
    pub certificates_produced: u64,
    pub spends_processed: u64,
    pub mempool_sizes: HashMap<u32, usize>,
    pub assigned_zones: Vec<u32>,
    zone_heights: HashMap<u32, u64>,
    recent_blocks: VecDeque<BlockSummary>,
    blocks_produced: u64,
}

/// Shared state handed to HTTP route handlers.
pub(crate) struct ZephyrState {
    pub(crate) config: ZephyrConfig,
    pub(crate) global_program_id: ProgramId,
    pub(crate) zone_program_ids: Vec<ProgramId>,
    pub(crate) runtime: Arc<std::sync::RwLock<ZephyrRuntime>>,
}

/// The Zephyr currency service.
///
/// Implements zone-scoped BFT consensus for a note-based currency on GRID.
/// Lifecycle:
/// - `on_start`: subscribes to global + assigned zone topics, initializes
///   epoch manager, spawns consensus tasks
/// - `on_stop`: cancels all tasks via the shutdown token, unsubscribes topics
pub struct ZephyrService {
    descriptor: ServiceDescriptor,
    config: ZephyrConfig,
    global_program_id: ProgramId,
    zone_program_ids: Vec<ProgramId>,
    runtime: Arc<std::sync::RwLock<ZephyrRuntime>>,
    gossip_handler: Arc<ZephyrGossipHandler>,
    consensus_rx: std::sync::Mutex<Option<mpsc::Receiver<(String, ZephyrZoneMessage)>>>,
    zone_rx: std::sync::Mutex<Option<mpsc::Receiver<(String, ZephyrZoneMessage)>>>,
    global_rx: std::sync::Mutex<Option<mpsc::Receiver<ZephyrGlobalMessage>>>,
}

impl ZephyrService {
    pub fn new(config: ZephyrConfig) -> Result<Self, ServiceError> {
        let global_pid = ZephyrGlobalDescriptor::new()
            .program_id()
            .map_err(|e| ServiceError::Descriptor(e.to_string()))?;

        let mut zone_pids = Vec::with_capacity(config.total_zones as usize);
        for zone_id in 0..config.total_zones {
            let pid = ZephyrZoneDescriptor::new(zone_id)
                .program_id()
                .map_err(|e| ServiceError::Descriptor(e.to_string()))?;
            zone_pids.push(pid);
        }

        let spend_pid = ZephyrSpendDescriptor::new()
            .program_id()
            .map_err(|e| ServiceError::Descriptor(e.to_string()))?;
        let validator_pid = ZephyrValidatorDescriptor::new()
            .program_id()
            .map_err(|e| ServiceError::Descriptor(e.to_string()))?;

        let mut owned_programs = vec![
            OwnedProgram {
                name: "zephyr/global".into(),
                version: "1".into(),
                program_id: global_pid,
            },
            OwnedProgram {
                name: "zephyr/spend".into(),
                version: "1".into(),
                program_id: spend_pid,
            },
            OwnedProgram {
                name: "zephyr/validators".into(),
                version: "1".into(),
                program_id: validator_pid,
            },
        ];
        for (i, pid) in zone_pids.iter().enumerate() {
            owned_programs.push(OwnedProgram {
                name: format!("zephyr/zone-{i}"),
                version: "1".into(),
                program_id: *pid,
            });
        }

        let global_topic = grid_core::program_topic(&global_pid);
        let (consensus_tx, consensus_rx) = mpsc::channel(4096);
        let (zone_tx, zone_rx) = mpsc::channel(65_536);
        let (global_tx, global_rx) = mpsc::channel(1024);
        let gossip_handler = Arc::new(ZephyrGossipHandler::new(
            global_topic,
            consensus_tx,
            zone_tx,
            global_tx,
        ));

        Ok(Self {
            descriptor: ServiceDescriptor {
                name: "ZEPHYR".into(),
                version: "0.1.0".into(),
                required_programs: vec![],
                owned_programs,
                summary: "Note-based currency with zone-scoped consensus.".into(),
            },
            config,
            global_program_id: global_pid,
            zone_program_ids: zone_pids,
            runtime: Arc::new(std::sync::RwLock::new(ZephyrRuntime {
                zone_heads: HashMap::new(),
                current_epoch: 0,
                epoch_progress_pct: 0.0,
                certificates_produced: 0,
                spends_processed: 0,
                mempool_sizes: HashMap::new(),
                assigned_zones: Vec::new(),
                zone_heights: HashMap::new(),
                recent_blocks: VecDeque::new(),
                blocks_produced: 0,
            })),
            gossip_handler,
            consensus_rx: std::sync::Mutex::new(Some(consensus_rx)),
            zone_rx: std::sync::Mutex::new(Some(zone_rx)),
            global_rx: std::sync::Mutex::new(Some(global_rx)),
        })
    }

    pub fn config(&self) -> &ZephyrConfig {
        &self.config
    }

    pub fn global_program_id(&self) -> &ProgramId {
        &self.global_program_id
    }

    pub fn zone_program_ids(&self) -> &[ProgramId] {
        &self.zone_program_ids
    }

    fn global_topic(&self) -> String {
        grid_core::program_topic(&self.global_program_id)
    }

    fn zone_topic(&self, zone_id: u32) -> String {
        grid_core::program_topic(&self.zone_program_ids[zone_id as usize])
    }
}

/// HMAC-SHA256 signing using the validator ID as key (sufficient for local testbed).
fn hmac_sign(validator_id: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(validator_id);
    hasher.update(data);
    hasher.finalize().to_vec()
}

#[async_trait]
impl Service for ZephyrService {
    fn descriptor(&self) -> &ServiceDescriptor {
        &self.descriptor
    }

    fn routes(&self, _ctx: &ServiceContext) -> Router {
        let state = Arc::new(ZephyrState {
            config: self.config.clone(),
            global_program_id: self.global_program_id,
            zone_program_ids: self.zone_program_ids.clone(),
            runtime: Arc::clone(&self.runtime),
        });

        Router::new()
            .route("/status", get(status_handler))
            .route("/zone/{id}/head", get(zone_head_handler))
            .route("/epoch/current", get(epoch_handler))
            .route("/health", get(health_handler))
            .with_state(state)
    }

    async fn on_start(&self, ctx: &ServiceContext) -> Result<(), ServiceError> {
        let global_topic = self.global_topic();
        ctx.subscribe_topic(&global_topic)?;
        info!(%global_topic, "subscribed to global topic");

        let mut validators = self.config.validators.clone();

        if self.config.self_validate && validators.is_empty() {
            if let Some(id) = ctx.identity() {
                let pk_bytes = id.public_key();
                let mut vid = [0u8; 32];
                let copy_len = pk_bytes.len().min(32);
                vid[..copy_len].copy_from_slice(&pk_bytes[..copy_len]);

                let mut pubkey = [0u8; 32];
                pubkey[..copy_len].copy_from_slice(&pk_bytes[..copy_len]);

                validators.push(ValidatorInfo {
                    validator_id: vid,
                    pubkey,
                    p2p_endpoint: id.zode_id().to_string(),
                });
                info!("self_validate enabled; running as solo validator");
            } else {
                warn!("self_validate enabled but no node identity available");
            }
        }

        if validators.is_empty() {
            warn!("no validators configured; Zephyr running in observer mode");
            return Ok(());
        }

        let my_validator_id = match ctx.identity() {
            Some(id) => {
                let mut vid = [0u8; 32];
                let pk_bytes = id.public_key();
                let copy_len = pk_bytes.len().min(32);
                vid[..copy_len].copy_from_slice(&pk_bytes[..copy_len]);
                vid
            }
            None => {
                warn!("no node identity; Zephyr running in observer mode");
                return Ok(());
            }
        };

        let epoch_mgr = EpochManager::new(
            0,
            self.config.epoch_duration_ms,
            self.config.initial_randomness,
            validators.clone(),
            self.config.total_zones,
            self.config.committee_size,
        );

        let assigned_zones = my_assigned_zones(
            &my_validator_id,
            epoch_mgr.randomness_seed(),
            &validators,
            self.config.total_zones,
            self.config.committee_size,
        );

        // Subscribe to ALL zone topics up-front so that epoch transitions
        // (which may reassign zones) don't require dynamic topic management.
        let mut topic_to_zone: HashMap<String, u32> = HashMap::new();
        for zone_id in 0..self.config.total_zones {
            let topic = self.zone_topic(zone_id);
            self.gossip_handler.add_zone_topic(topic.clone());
            ctx.subscribe_topic(&topic)?;
            topic_to_zone.insert(topic.clone(), zone_id);
            info!(zone_id, %topic, "subscribed to zone topic");
        }

        // Take channel receivers (one-time)
        let consensus_rx = self
            .consensus_rx
            .lock()
            .map_err(|e| ServiceError::Other(format!("lock poisoned: {e}")))?
            .take()
            .ok_or_else(|| ServiceError::Other("consensus_rx already taken".into()))?;

        let zone_rx = self
            .zone_rx
            .lock()
            .map_err(|e| ServiceError::Other(format!("lock poisoned: {e}")))?
            .take()
            .ok_or_else(|| ServiceError::Other("zone_rx already taken".into()))?;

        let global_rx = self
            .global_rx
            .lock()
            .map_err(|e| ServiceError::Other(format!("lock poisoned: {e}")))?
            .take()
            .ok_or_else(|| ServiceError::Other("global_rx already taken".into()))?;

        // Update runtime with initial state
        if let Ok(mut rt) = self.runtime.write() {
            rt.assigned_zones = assigned_zones.clone();
            rt.current_epoch = 0;
        }

        // Clone what we need for the spawned tasks
        let runtime = Arc::clone(&self.runtime);
        let config = self.config.clone();
        let shutdown = ctx.shutdown.clone();
        let publish_tx = ctx
            .publish_sender()
            .ok_or_else(|| ServiceError::NotInitialized("publish channel not set".into()))?;
        let global_topic_for_task = self.global_topic();

        // Shared mempool between ingest and consensus tasks
        let mempool = SharedMempool::new();
        for zone_id in 0..self.config.total_zones {
            mempool.add_zone(zone_id, 65_536).await;
        }

        // Spawn the ingest task (spend submissions only)
        tokio::spawn(ingest_loop(
            zone_rx,
            topic_to_zone.clone(),
            mempool.clone(),
            shutdown.clone(),
        ));

        // Spawn the consensus task
        tokio::spawn(consensus_loop(
            my_validator_id,
            validators,
            assigned_zones,
            topic_to_zone,
            config,
            runtime.clone(),
            consensus_rx,
            global_rx,
            publish_tx.clone(),
            global_topic_for_task.clone(),
            shutdown.clone(),
            epoch_mgr,
            mempool,
        ));

        info!(
            zones = self.config.total_zones,
            committee_size = self.config.committee_size,
            "Zephyr service started with consensus wiring"
        );

        Ok(())
    }

    async fn on_stop(&self) -> Result<(), ServiceError> {
        info!("Zephyr service stopped");
        Ok(())
    }

    fn route_info(&self) -> Vec<RouteInfo> {
        vec![
            RouteInfo {
                method: "GET",
                path: "/status",
                description: "Overall Zephyr status (epoch, zones, validator count)",
            },
            RouteInfo {
                method: "GET",
                path: "/zone/:id/head",
                description: "Current zone head hash",
            },
            RouteInfo {
                method: "GET",
                path: "/epoch/current",
                description: "Current epoch info",
            },
            RouteInfo {
                method: "GET",
                path: "/health",
                description: "Health check",
            },
        ]
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField {
            key: "self_validate",
            label: "Participate as validator",
            description: "Run this node as a solo validator using its own identity",
            field_type: ConfigFieldType::Bool { default: false },
        }]
    }

    fn current_config(&self) -> serde_json::Value {
        serde_json::json!({
            "self_validate": self.config.self_validate,
        })
    }

    fn gossip_handler(&self) -> Option<Arc<dyn ServiceGossipHandler>> {
        Some(Arc::clone(&self.gossip_handler) as _)
    }

    fn metrics(&self) -> serde_json::Value {
        let rt = self.runtime.read().unwrap_or_else(|e| e.into_inner());
        serde_json::json!({
            "zone_heads": rt.zone_heads.iter()
                .map(|(k, v)| (k.to_string(), hex::encode(&v[..8])))
                .collect::<HashMap<_, _>>(),
            "current_epoch": rt.current_epoch,
            "epoch_progress_pct": rt.epoch_progress_pct,
            "certificates_produced": rt.certificates_produced,
            "spends_processed": rt.spends_processed,
            "mempool_sizes": rt.mempool_sizes,
            "assigned_zones": rt.assigned_zones,
            "blocks_produced": rt.blocks_produced,
            "recent_blocks": rt.recent_blocks.iter().map(|b| {
                serde_json::json!({
                    "zone_id": b.zone_id,
                    "block_hash": &b.block_hash_hex,
                    "height": b.height,
                    "tx_nullifiers": &b.tx_nullifiers,
                })
            }).collect::<Vec<_>>(),
        })
    }
}

/// Spend ingestion task -- runs concurrently with the consensus loop.
///
/// Receives spend submissions from gossip and inserts them into the shared
/// mempool. This decouples high-volume transaction ingestion from
/// latency-sensitive consensus round-trips.
async fn ingest_loop(
    mut zone_rx: mpsc::Receiver<(String, ZephyrZoneMessage)>,
    topic_to_zone: HashMap<String, u32>,
    mempool: SharedMempool,
    shutdown: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                debug!("ingest loop shutting down");
                break;
            }

            msg = zone_rx.recv() => {
                let Some(first) = msg else { break };

                let mut batch = Vec::with_capacity(1025);
                batch.push(first);
                while batch.len() < 1024 {
                    match zone_rx.try_recv() {
                        Ok(m) => batch.push(m),
                        Err(_) => break,
                    }
                }

                let mut zone_buckets: HashMap<u32, Vec<grid_programs_zephyr::SpendTransaction>> =
                    HashMap::new();
                for (topic, msg) in batch {
                    let Some(&zone_id) = topic_to_zone.get(&topic) else {
                        continue;
                    };
                    match msg {
                        ZephyrZoneMessage::SubmitSpend(tx) => {
                            zone_buckets.entry(zone_id).or_default().push(tx);
                        }
                        ZephyrZoneMessage::SubmitSpendBatch(txs) => {
                            zone_buckets.entry(zone_id).or_default().extend(txs);
                        }
                        _ => {}
                    }
                }
                for (zone_id, txs) in zone_buckets {
                    mempool.insert_batch(zone_id, txs).await;
                }
            }
        }
    }
}

/// The main consensus event loop, spawned as an async task.
#[allow(clippy::too_many_arguments)]
async fn consensus_loop(
    my_validator_id: [u8; 32],
    validators: Vec<ValidatorInfo>,
    mut assigned_zones: Vec<u32>,
    topic_to_zone: HashMap<String, u32>,
    config: ZephyrConfig,
    runtime: Arc<std::sync::RwLock<ZephyrRuntime>>,
    mut consensus_rx: mpsc::Receiver<(String, ZephyrZoneMessage)>,
    mut global_rx: mpsc::Receiver<ZephyrGlobalMessage>,
    publish_tx: mpsc::Sender<(String, Vec<u8>)>,
    global_topic: String,
    shutdown: tokio_util::sync::CancellationToken,
    mut epoch_mgr: EpochManager,
    mempool: SharedMempool,
) {
    let mut consensus_engines: HashMap<u32, ZoneConsensus> = HashMap::new();
    let mut zone_head_store = ZoneHead::new();
    // block_hash -> (zone_id, list of nullifier hex strings)
    let mut block_tx_cache: HashMap<[u8; 32], (u32, Vec<String>)> = HashMap::new();
    // block_hash -> full nullifiers (for mempool cleanup after finalization)
    let mut block_nullifiers: HashMap<[u8; 32], (u32, Vec<Nullifier>)> = HashMap::new();
    // Deferred cleanups: when a certificate arrives before its proposal,
    // we can't clean the mempool yet (block_nullifiers hasn't been populated).
    // Store the block_hash -> zone_id here and resolve once the proposal lands.
    let mut deferred_cleanups: HashMap<[u8; 32], u32> = HashMap::new();

    // Build zone topic lookup (zone_id -> topic string)
    let mut zone_to_topic: HashMap<u32, String> = HashMap::new();
    for (topic, &zone_id) in &topic_to_zone {
        zone_to_topic.insert(zone_id, topic.clone());
    }

    for &zone_id in &assigned_zones {
        let committee = sample_committee(
            epoch_mgr.randomness_seed(),
            zone_id,
            &validators,
            config.committee_size,
        );
        let prev_head = zone_head_store.get_or_genesis(zone_id);
        consensus_engines.insert(
            zone_id,
            ZoneConsensus::new(
                zone_id,
                0,
                committee,
                my_validator_id,
                prev_head,
                config.clone(),
            ),
        );
    }

    let round_interval = std::time::Duration::from_millis(config.round_interval_ms);
    let mut round_timer = tokio::time::interval(round_interval);
    round_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let epoch_start = tokio::time::Instant::now();

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                info!("consensus loop shutting down");
                break;
            }

            _ = round_timer.tick() => {
                // Update epoch progress
                let elapsed = epoch_start.elapsed();
                let epoch_elapsed = elapsed.as_millis() as u64 % config.epoch_duration_ms;
                let progress = epoch_elapsed as f32 / config.epoch_duration_ms as f32;

                // Check for epoch boundary
                let expected_epoch = elapsed.as_millis() as u64 / config.epoch_duration_ms;
                let current_epoch = epoch_mgr.current_epoch();

                if expected_epoch > current_epoch {
                    let transition = epoch_mgr.advance_epoch(&my_validator_id);
                    info!(
                        new_epoch = transition.new_epoch,
                        gained = transition.gained_zones.len(),
                        lost = transition.lost_zones.len(),
                        "epoch advanced"
                    );

                    let new_epoch = transition.new_epoch;

                    for &zone_id in &transition.retained_zones {
                        let committee = sample_committee(
                            epoch_mgr.randomness_seed(),
                            zone_id,
                            &validators,
                            config.committee_size,
                        );
                        if let Some(engine) = consensus_engines.get_mut(&zone_id) {
                            engine.advance_to_epoch(new_epoch, committee);
                        }
                    }

                    for &zone_id in &transition.gained_zones {
                        let committee = sample_committee(
                            epoch_mgr.randomness_seed(),
                            zone_id,
                            &validators,
                            config.committee_size,
                        );
                        let prev_head = zone_head_store.get_or_genesis(zone_id);
                        consensus_engines.insert(
                            zone_id,
                            ZoneConsensus::new(
                                zone_id,
                                new_epoch,
                                committee,
                                my_validator_id,
                                prev_head,
                                config.clone(),
                            ),
                        );
                        mempool.add_zone(zone_id, 65_536).await;
                    }

                    for &zone_id in &transition.lost_zones {
                        consensus_engines.remove(&zone_id);
                        mempool.remove_zone(zone_id).await;
                    }

                    assigned_zones = epoch_mgr.zones_for_validator(&my_validator_id);

                    if let Ok(mut rt) = runtime.write() {
                        rt.assigned_zones = assigned_zones.clone();
                    }
                }

                if let Ok(mut rt) = runtime.write() {
                    rt.epoch_progress_pct = progress;
                    rt.current_epoch = epoch_mgr.current_epoch();
                }

                for &zone_id in &assigned_zones {
                    if let Some(engine) = consensus_engines.get_mut(&zone_id) {
                        engine.tick();
                        if engine.is_round_timed_out(config.round_timeout_ticks) {
                            warn!(
                                zone_id,
                                round = engine.round(),
                                "round timed out without quorum, rotating leader"
                            );
                            engine.timeout_round();
                        }
                    }
                }

                try_propose_for_zones(
                    &assigned_zones,
                    &mut consensus_engines,
                    &mempool,
                    my_validator_id,
                    &config,
                    &mut block_tx_cache,
                    &mut block_nullifiers,
                    &zone_to_topic,
                    &global_topic,
                    &publish_tx,
                    &mut zone_head_store,
                    &runtime,
                    &mut deferred_cleanups,
                )
                .await;

                // Update mempool sizes in runtime
                let sizes = mempool.zone_sizes().await;
                if let Ok(mut rt) = runtime.write() {
                    for (zone_id, size) in sizes {
                        rt.mempool_sizes.insert(zone_id, size);
                    }
                }
            }

            // Proposals and votes are processed before certificates so
            // that `block_nullifiers` is populated (from the proposal)
            // before `cleanup_mempool_after_cert` runs (from the cert).
            // Spends are handled by the separate `ingest_loop`, so this
            // channel only carries low-volume consensus traffic.
            msg = consensus_rx.recv() => {
                let Some(first) = msg else { break };

                let mut batch = Vec::with_capacity(129);
                batch.push(first);
                while batch.len() < 128 {
                    match consensus_rx.try_recv() {
                        Ok(m) => batch.push(m),
                        Err(_) => break,
                    }
                }

                for (topic, msg) in batch {
                    let Some(&zone_id) = topic_to_zone.get(&topic) else {
                        warn!(%topic, "received consensus message for unknown zone topic");
                        continue;
                    };

                    match msg {
                        ZephyrZoneMessage::Proposal(proposal) => {
                            cache_block_txs(&mut block_tx_cache, &mut block_nullifiers, zone_id, &proposal);
                            let Some(engine) = consensus_engines.get_mut(&zone_id) else {
                                continue;
                            };
                            let vid = my_validator_id;
                            if let Some(action) = engine.vote_on_proposal(&proposal, |data| hmac_sign(&vid, data)) {
                                engine.reset_timeout();
                                publish_action(&action, &zone_to_topic, zone_id, &global_topic, &publish_tx, &block_tx_cache).await;
                            }
                        }
                        ZephyrZoneMessage::Vote(vote) => {
                            let mut cert_produced = false;
                            if let Some(engine) = consensus_engines.get_mut(&zone_id) {
                                if let Some(action) = engine.receive_vote(vote) {
                                    if let ConsensusAction::BroadcastCertificate(ref cert) = action {
                                        apply_certificate_locally(
                                            cert,
                                            &mut zone_head_store,
                                            &mut block_tx_cache,
                                            &runtime,
                                        );
                                        cleanup_mempool_after_cert(cert, &mempool, &mut block_nullifiers, &mut deferred_cleanups).await;
                                        cert_produced = true;
                                    }
                                    publish_action(&action, &zone_to_topic, zone_id, &global_topic, &publish_tx, &block_tx_cache).await;
                                }
                            }
                            if cert_produced {
                                try_propose_for_zones(
                                    &assigned_zones,
                                    &mut consensus_engines,
                                    &mempool,
                                    my_validator_id,
                                    &config,
                                    &mut block_tx_cache,
                                    &mut block_nullifiers,
                                    &zone_to_topic,
                                    &global_topic,
                                    &publish_tx,
                                    &mut zone_head_store,
                                    &runtime,
                                    &mut deferred_cleanups,
                                )
                                .await;
                            }
                        }
                        ZephyrZoneMessage::Reject(_) => {}
                        _ => {}
                    }
                }

                // After processing proposals (which populate block_nullifiers),
                // drain any deferred cleanups that were waiting for the proposal.
                let resolved: Vec<[u8; 32]> = deferred_cleanups
                    .keys()
                    .filter(|h| block_nullifiers.contains_key(*h))
                    .copied()
                    .collect();
                for hash in resolved {
                    if let Some(zone_id) = deferred_cleanups.remove(&hash) {
                        if let Some((_, nullifiers)) = block_nullifiers.remove(&hash) {
                            mempool.remove_nullifiers(zone_id, &nullifiers).await;
                        }
                    }
                }
            }

            msg = global_rx.recv() => {
                let Some(first) = msg else { break };

                let mut global_batch = Vec::with_capacity(33);
                global_batch.push(first);
                while global_batch.len() < 32 {
                    match global_rx.try_recv() {
                        Ok(m) => global_batch.push(m),
                        Err(_) => break,
                    }
                }

                for msg in global_batch {
                    match msg {
                        ZephyrGlobalMessage::Certificate { cert, tx_nullifiers } => {
                            if !tx_nullifiers.is_empty() {
                                block_tx_cache
                                    .entry(cert.block_hash)
                                    .or_insert_with(|| (cert.zone_id, tx_nullifiers));
                            }

                            let cz = cert.zone_id;
                            let mut round_advanced = false;
                            if let Some(engine) = consensus_engines.get_mut(&cz) {
                                if engine.apply_certificate(&cert) {
                                    apply_certificate_locally(
                                        &cert,
                                        &mut zone_head_store,
                                        &mut block_tx_cache,
                                        &runtime,
                                    );
                                    cleanup_mempool_after_cert(
                                        &cert,
                                        &mempool,
                                        &mut block_nullifiers,
                                        &mut deferred_cleanups,
                                    )
                                    .await;
                                    debug!(zone_id = cz, "applied certificate from global topic");
                                    round_advanced = true;
                                }
                            } else {
                                apply_certificate_locally(
                                    &cert,
                                    &mut zone_head_store,
                                    &mut block_tx_cache,
                                    &runtime,
                                );
                                cleanup_mempool_after_cert(
                                    &cert,
                                    &mempool,
                                    &mut block_nullifiers,
                                    &mut deferred_cleanups,
                                )
                                .await;
                            }
                            if round_advanced {
                                try_propose_for_zones(
                                    &assigned_zones,
                                    &mut consensus_engines,
                                    &mempool,
                                    my_validator_id,
                                    &config,
                                    &mut block_tx_cache,
                                    &mut block_nullifiers,
                                    &zone_to_topic,
                                    &global_topic,
                                    &publish_tx,
                                    &mut zone_head_store,
                                    &runtime,
                                    &mut deferred_cleanups,
                                )
                                .await;
                            }
                        }
                        ZephyrGlobalMessage::EpochAnnounce(ann) => {
                            debug!(epoch = ann.epoch, "received epoch announcement");
                        }
                    }
                }
            }
        }
    }
}

fn cache_block_txs(
    cache: &mut HashMap<[u8; 32], (u32, Vec<String>)>,
    nullifier_cache: &mut HashMap<[u8; 32], (u32, Vec<Nullifier>)>,
    zone_id: u32,
    block: &grid_programs_zephyr::Block,
) {
    if cache.len() >= MAX_BLOCK_TX_CACHE {
        let keys: Vec<[u8; 32]> = cache.keys().take(MAX_BLOCK_TX_CACHE / 4).copied().collect();
        for k in &keys {
            cache.remove(k);
            nullifier_cache.remove(k);
        }
    }
    let full_nullifiers: Vec<Nullifier> = block
        .transactions
        .iter()
        .map(|tx| tx.nullifier.clone())
        .collect();
    let hex_nullifiers: Vec<String> = full_nullifiers
        .iter()
        .map(|n| hex::encode(&n.0[..8]))
        .collect();
    cache.insert(block.block_hash, (zone_id, hex_nullifiers));
    nullifier_cache.insert(block.block_hash, (zone_id, full_nullifiers));
}

async fn cleanup_mempool_after_cert(
    cert: &FinalityCertificate,
    mempool: &SharedMempool,
    block_nullifiers: &mut HashMap<[u8; 32], (u32, Vec<Nullifier>)>,
    deferred_cleanups: &mut HashMap<[u8; 32], u32>,
) {
    if let Some((zone_id, nullifiers)) = block_nullifiers.remove(&cert.block_hash) {
        mempool.remove_nullifiers(zone_id, &nullifiers).await;
    } else {
        deferred_cleanups.insert(cert.block_hash, cert.zone_id);
    }
}

fn apply_certificate_locally(
    cert: &FinalityCertificate,
    zone_head_store: &mut ZoneHead,
    block_tx_cache: &mut HashMap<[u8; 32], (u32, Vec<String>)>,
    runtime: &Arc<std::sync::RwLock<ZephyrRuntime>>,
) {
    zone_head_store.set(cert.zone_id, cert.block_hash);
    let tx_nullifiers = block_tx_cache
        .get(&cert.block_hash)
        .map(|(_, n)| n.clone())
        .unwrap_or_default();
    let spend_count = tx_nullifiers.len() as u64;
    if let Ok(mut rt) = runtime.write() {
        rt.zone_heads.insert(cert.zone_id, cert.block_hash);
        rt.certificates_produced += 1;
        rt.spends_processed += spend_count;

        let height = rt.zone_heights.entry(cert.zone_id).or_insert(0);
        *height += 1;
        let block_height = *height;

        rt.recent_blocks.push_back(BlockSummary {
            zone_id: cert.zone_id,
            block_hash_hex: hex::encode(&cert.block_hash[..8]),
            height: block_height,
            tx_nullifiers,
        });
        rt.blocks_produced += 1;
        if rt.recent_blocks.len() > MAX_RECENT_BLOCKS {
            rt.recent_blocks.pop_front();
        }
    }
}

async fn publish_action(
    action: &ConsensusAction,
    zone_to_topic: &HashMap<u32, String>,
    zone_id: u32,
    global_topic: &str,
    publish_tx: &mpsc::Sender<(String, Vec<u8>)>,
    block_tx_cache: &HashMap<[u8; 32], (u32, Vec<String>)>,
) {
    let (topic, data) = match action {
        ConsensusAction::BroadcastProposal(p) => {
            let topic = zone_to_topic.get(&zone_id).cloned().unwrap_or_default();
            let msg = ZephyrZoneMessage::Proposal(p.clone());
            let data = match grid_core::encode_canonical(&msg) {
                Ok(d) => d,
                Err(e) => {
                    warn!(error = %e, "failed to encode proposal");
                    return;
                }
            };
            (topic, data)
        }
        ConsensusAction::BroadcastVote(v) => {
            let topic = zone_to_topic.get(&zone_id).cloned().unwrap_or_default();
            let msg = ZephyrZoneMessage::Vote(v.clone());
            let data = match grid_core::encode_canonical(&msg) {
                Ok(d) => d,
                Err(e) => {
                    warn!(error = %e, "failed to encode vote");
                    return;
                }
            };
            (topic, data)
        }
        ConsensusAction::BroadcastCertificate(c) => {
            let tx_nullifiers = block_tx_cache
                .get(&c.block_hash)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            let msg = ZephyrGlobalMessage::Certificate {
                cert: c.clone(),
                tx_nullifiers,
            };
            let data = match grid_core::encode_canonical(&msg) {
                Ok(d) => d,
                Err(e) => {
                    warn!(error = %e, "failed to encode certificate");
                    return;
                }
            };
            (global_topic.to_owned(), data)
        }
    };

    if publish_tx.send((topic, data)).await.is_err() {
        warn!("publish channel closed");
    }
}

/// Attempt to propose blocks for all assigned zones where this node is leader.
///
/// Called on every round timer tick **and** immediately after a round advances
/// (certificate produced or applied) to eliminate dead time between rounds.
///
/// Mempool peeks are issued concurrently across zones so that per-zone locks
/// are acquired in parallel rather than sequentially.
#[allow(clippy::too_many_arguments)]
async fn try_propose_for_zones(
    assigned_zones: &[u32],
    consensus_engines: &mut HashMap<u32, ZoneConsensus>,
    mempool: &SharedMempool,
    my_validator_id: [u8; 32],
    config: &ZephyrConfig,
    block_tx_cache: &mut HashMap<[u8; 32], (u32, Vec<String>)>,
    block_nullifiers: &mut HashMap<[u8; 32], (u32, Vec<Nullifier>)>,
    zone_to_topic: &HashMap<u32, String>,
    global_topic: &str,
    publish_tx: &mpsc::Sender<(String, Vec<u8>)>,
    zone_head_store: &mut ZoneHead,
    runtime: &Arc<std::sync::RwLock<ZephyrRuntime>>,
    deferred_cleanups: &mut HashMap<[u8; 32], u32>,
) {
    // Identify zones that need a fresh mempool peek (leader + no pending proposal).
    let zones_needing_peek: Vec<u32> = assigned_zones
        .iter()
        .filter(|&&zid| {
            consensus_engines
                .get(&zid)
                .map_or(false, |e| e.is_leader() && !e.has_pending_proposal())
        })
        .copied()
        .collect();

    // Fire all peeks concurrently -- with per-zone locks these run in parallel.
    let max_block_size = config.max_block_size;
    let mut peek_handles = Vec::with_capacity(zones_needing_peek.len());
    for &zid in &zones_needing_peek {
        let mp = mempool.clone();
        peek_handles.push(tokio::spawn(
            async move { (zid, mp.peek(zid, max_block_size).await) },
        ));
    }

    let mut prefetched: HashMap<u32, Vec<grid_programs_zephyr::SpendTransaction>> =
        HashMap::with_capacity(peek_handles.len());
    for handle in peek_handles {
        if let Ok((zid, spends)) = handle.await {
            prefetched.insert(zid, spends);
        }
    }

    // Process proposals using pre-fetched spends.
    for &zone_id in assigned_zones {
        let Some(engine) = consensus_engines.get_mut(&zone_id) else {
            continue;
        };
        if !engine.is_leader() {
            continue;
        }
        let spends = prefetched.remove(&zone_id).unwrap_or_default();
        let vid = my_validator_id;
        if let Some(action) = engine.propose(spends, |data| hmac_sign(&vid, data)) {
            if let ConsensusAction::BroadcastProposal(ref block) = action {
                cache_block_txs(block_tx_cache, block_nullifiers, zone_id, block);
                let vid2 = my_validator_id;
                if let Some(vote_action) =
                    engine.vote_on_proposal(block, |data| hmac_sign(&vid2, data))
                {
                    publish_action(
                        &vote_action,
                        zone_to_topic,
                        zone_id,
                        global_topic,
                        publish_tx,
                        block_tx_cache,
                    )
                    .await;
                    if let ConsensusAction::BroadcastVote(vote) = vote_action {
                        if let Some(cert_action) = engine.receive_vote(vote) {
                            if let ConsensusAction::BroadcastCertificate(ref cert) = cert_action {
                                apply_certificate_locally(
                                    cert,
                                    zone_head_store,
                                    block_tx_cache,
                                    runtime,
                                );
                                cleanup_mempool_after_cert(cert, mempool, block_nullifiers, deferred_cleanups).await;
                            }
                            publish_action(
                                &cert_action,
                                zone_to_topic,
                                zone_id,
                                global_topic,
                                publish_tx,
                                block_tx_cache,
                            )
                            .await;
                        }
                    }
                }
            }
            publish_action(&action, zone_to_topic, zone_id, global_topic, publish_tx, block_tx_cache).await;
        }
    }
}

// --- HTTP Handlers ---

async fn status_handler(State(state): State<Arc<ZephyrState>>) -> impl IntoResponse {
    let rt = state.runtime.read().unwrap_or_else(|e| e.into_inner());
    Json(serde_json::json!({
        "service": "ZEPHYR",
        "total_zones": state.config.total_zones,
        "committee_size": state.config.committee_size,
        "validator_count": state.config.validators.len(),
        "global_program_id": state.global_program_id.to_hex(),
        "current_epoch": rt.current_epoch,
        "certificates_produced": rt.certificates_produced,
        "spends_processed": rt.spends_processed,
    }))
}

async fn zone_head_handler(
    State(state): State<Arc<ZephyrState>>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    if id >= state.config.total_zones {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "zone not found" })),
        )
            .into_response();
    }
    let pid = &state.zone_program_ids[id as usize];
    let rt = state.runtime.read().unwrap_or_else(|e| e.into_inner());
    let head = rt.zone_heads.get(&id).map(hex::encode);
    Json(serde_json::json!({
        "zone_id": id,
        "program_id": pid.to_hex(),
        "head": head,
    }))
    .into_response()
}

async fn epoch_handler(State(state): State<Arc<ZephyrState>>) -> impl IntoResponse {
    let rt = state.runtime.read().unwrap_or_else(|e| e.into_inner());
    Json(serde_json::json!({
        "epoch": rt.current_epoch,
        "epoch_duration_ms": state.config.epoch_duration_ms,
        "epoch_progress_pct": rt.epoch_progress_pct,
        "total_zones": state.config.total_zones,
        "committee_size": state.config.committee_size,
    }))
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_succeeds_with_default_config() {
        let svc = ZephyrService::new(ZephyrConfig::default()).unwrap();
        assert_eq!(svc.descriptor().name, "ZEPHYR");
        assert_eq!(svc.descriptor().version, "0.1.0");
    }

    #[test]
    fn zone_program_ids_match_zone_count() {
        let config = ZephyrConfig {
            total_zones: 4,
            ..ZephyrConfig::default()
        };
        let svc = ZephyrService::new(config).unwrap();
        assert_eq!(svc.zone_program_ids().len(), 4);
    }

    #[test]
    fn route_info_contains_expected_paths() {
        let svc = ZephyrService::new(ZephyrConfig::default()).unwrap();
        let routes = svc.route_info();
        assert_eq!(routes.len(), 4);
        assert!(routes.iter().any(|r| r.path == "/health"));
        assert!(routes.iter().any(|r| r.path == "/status"));
    }

    #[test]
    fn global_program_id_is_deterministic() {
        let svc1 = ZephyrService::new(ZephyrConfig::default()).unwrap();
        let svc2 = ZephyrService::new(ZephyrConfig::default()).unwrap();
        assert_eq!(svc1.global_program_id(), svc2.global_program_id());
    }

    #[test]
    fn global_topic_format() {
        let svc = ZephyrService::new(ZephyrConfig::default()).unwrap();
        let topic = svc.global_topic();
        assert!(topic.starts_with("prog/"));
        assert_eq!(topic.len(), 5 + 64);
    }

    #[test]
    fn zone_topics_are_distinct() {
        let config = ZephyrConfig {
            total_zones: 4,
            ..ZephyrConfig::default()
        };
        let svc = ZephyrService::new(config).unwrap();
        let topics: Vec<String> = (0..4).map(|z| svc.zone_topic(z)).collect();
        for (i, a) in topics.iter().enumerate() {
            for (j, b) in topics.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "zone {i} and {j} should have distinct topics");
                }
            }
        }
    }

    #[test]
    fn gossip_handler_is_some() {
        let svc = ZephyrService::new(ZephyrConfig::default()).unwrap();
        assert!(svc.gossip_handler().is_some());
    }

    #[test]
    fn metrics_returns_valid_json() {
        let svc = ZephyrService::new(ZephyrConfig::default()).unwrap();
        let m = svc.metrics();
        assert!(m.is_object());
        assert_eq!(m["current_epoch"], 0);
    }

    #[test]
    fn hmac_sign_is_deterministic() {
        let vid = [0xAB; 32];
        let data = b"test-data";
        let s1 = hmac_sign(&vid, data);
        let s2 = hmac_sign(&vid, data);
        assert_eq!(s1, s2);
    }
}
