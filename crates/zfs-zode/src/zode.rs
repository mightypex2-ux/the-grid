use std::sync::{Arc, RwLock};

use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, error, info, warn};
use zfs_core::GossipBlock;
use zfs_net::{format_zode_id, NetworkEvent, NetworkService, ZodeId};
use zfs_proof::{NoopVerifier, ProofVerifier};
use zfs_storage::{RocksStorage, StorageBackend, StorageStats};

use crate::config::ZodeConfig;
use crate::error::ZodeError;
use crate::handler::RequestHandler;
use crate::metrics::{MetricsSnapshot, ZodeMetrics};

/// Structured log events emitted by the Zode for UI consumption.
#[derive(Debug, Clone)]
pub enum LogEvent {
    /// Zode started and is serving.
    Started { listen_addr: String },
    /// A new peer connected.
    PeerConnected(String),
    /// A peer disconnected.
    PeerDisconnected(String),
    /// A new peer was discovered via DHT.
    PeerDiscovered(String),
    /// A store request was received and processed.
    StoreProcessed {
        program_id: String,
        cid: String,
        accepted: bool,
        reason: Option<String>,
    },
    /// A fetch request was received and processed.
    FetchProcessed { program_id: String, found: bool },
    /// A block was received and stored via GossipSub.
    GossipReceived {
        program_id: String,
        cid: String,
        accepted: bool,
    },
    /// The Zode is shutting down.
    ShuttingDown,
}

/// Status snapshot of the running Zode.
#[derive(Debug, Clone)]
pub struct ZodeStatus {
    /// The local Zode ID.
    pub zode_id: String,
    /// Number of connected Zodes.
    pub peer_count: u64,
    /// Connected Zode IDs.
    pub connected_peers: Vec<String>,
    /// Subscribed program topics.
    pub topics: Vec<String>,
    /// Storage usage.
    pub storage: StorageStats,
    /// Metrics snapshot.
    pub metrics: MetricsSnapshot,
}

/// The Zode — ties together storage, network, proof, and programs.
///
/// Created via [`Zode::start`]. The event loop runs in a background tokio
/// task; the caller interacts via [`status`](Zode::status),
/// [`subscribe_events`](Zode::subscribe_events), and
/// [`shutdown`](Zode::shutdown).
pub struct Zode {
    metrics: Arc<ZodeMetrics>,
    storage: Arc<RocksStorage>,
    network: Arc<Mutex<NetworkService>>,
    zode_id: ZodeId,
    topics: Vec<String>,
    connected_peers: Arc<RwLock<Vec<String>>>,
    event_tx: broadcast::Sender<LogEvent>,
    shutdown_tx: mpsc::Sender<()>,
    publish_tx: mpsc::Sender<(String, Vec<u8>)>,
    event_loop_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Zode {
    /// Start the Zode with the given configuration.
    ///
    /// Opens storage, starts the network, subscribes to topics, and begins
    /// the event loop in a background task.
    pub async fn start(config: ZodeConfig) -> Result<Self, ZodeError> {
        Self::start_with_verifier(config, Arc::new(NoopVerifier)).await
    }

    /// Start the Zode with a custom proof verifier (for testing or future use).
    pub async fn start_with_verifier(
        config: ZodeConfig,
        verifier: Arc<dyn ProofVerifier>,
    ) -> Result<Self, ZodeError> {
        let storage =
            Arc::new(RocksStorage::open(config.storage.clone()).map_err(ZodeError::Storage)?);
        info!(path = ?config.storage.path, "storage opened");

        let (network, zode_id, topic_strings, effective) =
            Self::start_network(&config).await?;

        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (publish_tx, publish_rx) = mpsc::channel(64);
        let metrics = Arc::new(ZodeMetrics::default());
        let connected_peers: Arc<RwLock<Vec<String>>> = Arc::default();

        if let Ok(stats) = storage.stats() {
            metrics.set_db_size(stats.db_size_bytes);
        }

        let handler = RequestHandler::new(
            Arc::clone(&storage),
            effective,
            config.limits.clone(),
            config.proof_policy.clone(),
            verifier,
            Arc::clone(&metrics),
        );

        let network = Arc::new(Mutex::new(network));
        let event_loop_handle = Self::spawn_event_loop(
            handler,
            Arc::clone(&network),
            event_tx.clone(),
            Arc::clone(&metrics),
            Arc::clone(&connected_peers),
            shutdown_rx,
            publish_rx,
        );

        Ok(Self {
            metrics,
            storage,
            network,
            zode_id,
            topics: topic_strings,
            connected_peers,
            event_tx,
            shutdown_tx,
            publish_tx,
            event_loop_handle: Mutex::new(Some(event_loop_handle)),
        })
    }

    async fn start_network(
        config: &ZodeConfig,
    ) -> Result<
        (
            NetworkService,
            ZodeId,
            Vec<String>,
            std::collections::HashSet<zfs_core::ProgramId>,
        ),
        ZodeError,
    > {
        let mut network = NetworkService::new(config.network.clone())
            .await
            .map_err(ZodeError::Network)?;
        let zode_id = *network.local_zode_id();
        info!(%zode_id, "network started");

        let effective = config.effective_topics();
        let mut topic_strings = Vec::new();
        for pid in &effective {
            let topic = zfs_programs::program_topic(pid);
            network.subscribe(&topic).map_err(ZodeError::Network)?;
            topic_strings.push(topic);
            debug!(program_id = %pid.to_hex(), "subscribed to topic");
        }
        Ok((network, zode_id, topic_strings, effective))
    }

    fn spawn_event_loop<S: StorageBackend + Send + Sync + 'static>(
        handler: RequestHandler<S>,
        network: Arc<Mutex<NetworkService>>,
        event_tx: broadcast::Sender<LogEvent>,
        metrics: Arc<ZodeMetrics>,
        connected_peers: Arc<RwLock<Vec<String>>>,
        shutdown_rx: mpsc::Receiver<()>,
        publish_rx: mpsc::Receiver<(String, Vec<u8>)>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            Self::event_loop(
                handler,
                network,
                event_tx,
                metrics,
                connected_peers,
                shutdown_rx,
                publish_rx,
            )
            .await;
        })
    }

    /// Get a status snapshot of the running Zode (lock-free, never blocks).
    pub fn status(&self) -> ZodeStatus {
        let storage_stats = self.storage.stats().unwrap_or_default();
        let metrics = self.metrics.snapshot();
        let connected_peers = self
            .connected_peers
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let peer_count = connected_peers.len() as u64;
        ZodeStatus {
            zode_id: format_zode_id(&self.zode_id),
            peer_count,
            connected_peers,
            topics: self.topics.clone(),
            storage: storage_stats,
            metrics,
        }
    }

    /// Subscribe to structured log events from the Zode.
    pub fn subscribe_events(&self) -> broadcast::Receiver<LogEvent> {
        self.event_tx.subscribe()
    }

    /// Access the metrics (for direct reads from atomic counters).
    pub fn metrics(&self) -> &Arc<ZodeMetrics> {
        &self.metrics
    }

    /// Access the underlying storage (for advanced queries).
    pub fn storage(&self) -> &Arc<RocksStorage> {
        &self.storage
    }

    /// Lock-free access to subscribed topic strings (e.g. `"prog/{hex}"`).
    pub fn topics(&self) -> &[String] {
        &self.topics
    }

    /// Queue a GossipSub publish.  The event loop will send it on the
    /// next iteration (non-blocking from the caller's perspective).
    pub fn publish(&self, topic: String, data: Vec<u8>) {
        if let Err(e) = self.publish_tx.try_send((topic, data)) {
            warn!(error = %e, "publish channel full or closed");
        }
    }

    /// Access the network service (for advanced operations).
    pub fn network(&self) -> &Arc<Mutex<NetworkService>> {
        &self.network
    }

    /// Gracefully shut down the Zode and wait for the event loop to exit.
    pub async fn shutdown(&self) {
        let _ = self.event_tx.send(LogEvent::ShuttingDown);
        let _ = self.shutdown_tx.send(()).await;
        if let Some(handle) = self.event_loop_handle.lock().await.take() {
            let _ = handle.await;
        }
        info!("zode shutdown complete");
    }

    async fn event_loop<S: StorageBackend>(
        handler: RequestHandler<S>,
        network: Arc<Mutex<NetworkService>>,
        event_tx: broadcast::Sender<LogEvent>,
        metrics: Arc<ZodeMetrics>,
        connected_peers: Arc<RwLock<Vec<String>>>,
        mut shutdown_rx: mpsc::Receiver<()>,
        mut publish_rx: mpsc::Receiver<(String, Vec<u8>)>,
    ) {
        loop {
            let event = {
                let mut net = network.lock().await;
                tokio::select! {
                    event = net.next_event() => event,
                    _ = shutdown_rx.recv() => {
                        info!("event loop shutting down");
                        return;
                    }
                    Some((topic, data)) = publish_rx.recv() => {
                        if let Err(e) = net.publish(&topic, data) {
                            warn!(error = %e, "gossip publish failed");
                        }
                        continue;
                    }
                }
            };

            let Some(event) = event else {
                warn!("network event stream ended");
                return;
            };

            Self::dispatch_event(
                event,
                &handler,
                &network,
                &event_tx,
                &metrics,
                &connected_peers,
            )
            .await;
        }
    }

    async fn dispatch_event<S: StorageBackend>(
        event: NetworkEvent,
        handler: &RequestHandler<S>,
        network: &Arc<Mutex<NetworkService>>,
        event_tx: &broadcast::Sender<LogEvent>,
        metrics: &Arc<ZodeMetrics>,
        connected_peers: &Arc<RwLock<Vec<String>>>,
    ) {
        match event {
            NetworkEvent::PeerConnected(peer) => {
                Self::handle_peer_connected(peer, metrics, connected_peers, event_tx);
            }
            NetworkEvent::PeerDisconnected(peer) => {
                Self::handle_peer_disconnected(peer, metrics, connected_peers, event_tx);
            }
            NetworkEvent::IncomingStore { peer, request, channel } => {
                Self::handle_incoming_store(handler, network, event_tx, peer, request, channel)
                    .await;
            }
            NetworkEvent::IncomingFetch { peer, request, channel } => {
                Self::handle_incoming_fetch(handler, network, event_tx, peer, request, channel)
                    .await;
            }
            NetworkEvent::ListenAddress(addr) => {
                info!(%addr, "listening");
                let _ = event_tx.send(LogEvent::Started { listen_addr: addr.to_string() });
            }
            NetworkEvent::GossipMessage { topic, data, .. } => {
                Self::handle_gossip_message(handler, event_tx, &topic, &data);
            }
            NetworkEvent::PeerDiscovered { zode_id, addresses, .. } => {
                debug!(%zode_id, addr_count = addresses.len(), "zode discovered via DHT");
                let _ = event_tx.send(LogEvent::PeerDiscovered(format_zode_id(&zode_id)));
            }
            NetworkEvent::StoreResult { .. }
            | NetworkEvent::FetchResult { .. }
            | NetworkEvent::OutboundFailure { .. } => {}
        }
    }

    fn handle_peer_connected(
        peer: ZodeId,
        metrics: &Arc<ZodeMetrics>,
        connected_peers: &Arc<RwLock<Vec<String>>>,
        event_tx: &broadcast::Sender<LogEvent>,
    ) {
        metrics.inc_peer_count();
        let peer_str = format_zode_id(&peer);
        if let Ok(mut peers) = connected_peers.write() {
            if !peers.contains(&peer_str) {
                peers.push(peer_str.clone());
            }
        }
        debug!(%peer, "peer connected");
        let _ = event_tx.send(LogEvent::PeerConnected(peer_str));
    }

    fn handle_peer_disconnected(
        peer: ZodeId,
        metrics: &Arc<ZodeMetrics>,
        connected_peers: &Arc<RwLock<Vec<String>>>,
        event_tx: &broadcast::Sender<LogEvent>,
    ) {
        metrics.dec_peer_count();
        let peer_str = format_zode_id(&peer);
        if let Ok(mut peers) = connected_peers.write() {
            peers.retain(|p| p != &peer_str);
        }
        debug!(%peer, "peer disconnected");
        let _ = event_tx.send(LogEvent::PeerDisconnected(peer_str));
    }

    async fn handle_incoming_store<S: StorageBackend>(
        handler: &RequestHandler<S>,
        network: &Arc<Mutex<NetworkService>>,
        event_tx: &broadcast::Sender<LogEvent>,
        peer: ZodeId,
        request: Box<zfs_core::StoreRequest>,
        channel: zfs_net::ResponseChannel<zfs_net::ZfsResponse>,
    ) {
        let program_hex = request.program_id.to_hex();
        let cid_hex = request.cid.to_hex();
        debug!(%peer, program = %program_hex, cid = %cid_hex, "incoming store");

        let response = handler.handle_store(&request);
        let accepted = response.ok;
        let reason = response.error_code.map(|c| c.to_string());

        let _ = event_tx.send(LogEvent::StoreProcessed {
            program_id: program_hex,
            cid: cid_hex,
            accepted,
            reason,
        });

        let mut net = network.lock().await;
        if let Err(e) = net.send_store_response(channel, response) {
            error!(error = %e, "failed to send store response");
        }
    }

    async fn handle_incoming_fetch<S: StorageBackend>(
        handler: &RequestHandler<S>,
        network: &Arc<Mutex<NetworkService>>,
        event_tx: &broadcast::Sender<LogEvent>,
        peer: ZodeId,
        request: zfs_core::FetchRequest,
        channel: zfs_net::ResponseChannel<zfs_net::ZfsResponse>,
    ) {
        let program_hex = request.program_id.to_hex();
        debug!(%peer, program = %program_hex, "incoming fetch");

        let response = handler.handle_fetch(&request);
        let found = response.error_code.is_none();

        let _ = event_tx.send(LogEvent::FetchProcessed {
            program_id: program_hex,
            found,
        });

        let mut net = network.lock().await;
        if let Err(e) = net.send_fetch_response(channel, response) {
            error!(error = %e, "failed to send fetch response");
        }
    }

    fn handle_gossip_message<S: StorageBackend>(
        handler: &RequestHandler<S>,
        event_tx: &broadcast::Sender<LogEvent>,
        topic: &str,
        data: &[u8],
    ) {
        debug!(%topic, bytes = data.len(), "gossip message received");
        match zfs_core::decode_canonical::<GossipBlock>(data) {
            Ok(block) => {
                let program_id = block.program_id.to_hex();
                let cid = block.cid.to_hex();
                let accepted = handler.handle_gossip(&block);
                let _ = event_tx.send(LogEvent::GossipReceived {
                    program_id,
                    cid,
                    accepted,
                });
            }
            Err(e) => {
                debug!(error = %e, "failed to decode gossip block");
            }
        }
    }
}
