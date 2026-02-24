#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use hkdf::Hkdf;
use sha2::Sha256;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use zfs_core::{Cid, Head, ProgramId, SectorId};
use zfs_crypto::{decrypt_sector, encrypt_sector, SectorKey};
use zfs_net::NetworkConfig;
use zfs_programs::zchat::{ChannelId, ZChatDescriptor, ZChatMessage, TEST_CHANNEL_ID};
use zfs_storage::{BlockStore, HeadStore, ProgramIndex, StorageBackend, StorageConfig};
use zfs_zode::{DefaultProgramsConfig, LogEvent, Zode, ZodeConfig, ZodeError, ZodeStatus};
use zero_neural::{
    derive_machine_keypair, ed25519_to_did_key, MachineKeyCapabilities, NeuralKey,
};

const MAX_LOG_ENTRIES: usize = 500;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let rt = Runtime::new()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Zode App")
            .with_inner_size([820.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Zode App",
        options,
        Box::new(move |_cc| Ok(Box::new(ZodeApp::new(rt)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Settings (editable while running)
// ---------------------------------------------------------------------------

struct Settings {
    data_dir: String,
    listen_addr: String,
    bootstrap_input: String,
    bootstrap_peers: Vec<String>,
    enable_zid: bool,
    enable_zchat: bool,
    topic_input: String,
    topics: Vec<String>,
    enable_kademlia: bool,
    kademlia_server_mode: bool,
    random_walk_interval_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            data_dir: "zfs-zode-data".into(),
            listen_addr: "/ip4/127.0.0.1/udp/0/quic-v1".into(),
            bootstrap_input: String::new(),
            bootstrap_peers: Vec::new(),
            enable_zid: true,
            enable_zchat: true,
            topic_input: String::new(),
            topics: Vec::new(),
            enable_kademlia: true,
            kademlia_server_mode: true,
            random_walk_interval_secs: 30,
        }
    }
}

impl Settings {
    fn build_config(&self) -> Result<ZodeConfig, String> {
        let listen_addr: zfs_net::Multiaddr = self
            .listen_addr
            .parse()
            .map_err(|e| format!("Bad listen address: {e}"))?;

        let mut bootstrap = Vec::new();
        for addr_str in &self.bootstrap_peers {
            let addr: zfs_net::Multiaddr = addr_str
                .parse()
                .map_err(|e| format!("Bad bootstrap addr '{addr_str}': {e}"))?;
            bootstrap.push(addr);
        }

        let mut topic_set = HashSet::new();
        for hex in &self.topics {
            let pid =
                ProgramId::from_hex(hex).map_err(|e| format!("Bad topic '{hex}': {e}"))?;
            topic_set.insert(pid);
        }

        let kad_mode = if self.kademlia_server_mode {
            zfs_net::KademliaMode::Server
        } else {
            zfs_net::KademliaMode::Client
        };

        let discovery = zfs_net::DiscoveryConfig {
            enable_kademlia: self.enable_kademlia,
            kademlia_mode: kad_mode,
            random_walk_interval: std::time::Duration::from_secs(self.random_walk_interval_secs),
            ..Default::default()
        };

        let network = NetworkConfig::new(listen_addr)
            .with_bootstrap_peers(bootstrap)
            .with_discovery(discovery);
        let storage = StorageConfig::new(PathBuf::from(&self.data_dir));

        Ok(ZodeConfig {
            storage,
            default_programs: DefaultProgramsConfig {
                zid: self.enable_zid,
                zchat: self.enable_zchat,
            },
            topics: topic_set,
            limits: Default::default(),
            proof_policy: Default::default(),
            network,
        })
    }
}

// ---------------------------------------------------------------------------
// Shared state (updated by background tasks)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct AppState {
    status: Option<ZodeStatus>,
    log_entries: VecDeque<String>,
    listen_addr: Option<String>,
}

impl AppState {
    fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            status: self.status.clone(),
            log_entries: self.log_entries.iter().cloned().collect(),
            listen_addr: self.listen_addr.clone(),
        }
    }
}

struct StateSnapshot {
    status: Option<ZodeStatus>,
    log_entries: Vec<String>,
    listen_addr: Option<String>,
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Status,
    Traverse,
    Peers,
    Log,
    TestChat,
    Info,
    Settings,
}

// ---------------------------------------------------------------------------
// Chat state
// ---------------------------------------------------------------------------

struct DisplayMessage {
    sender: String,
    content: String,
    timestamp_ms: u64,
}

struct ChatState {
    messages: Vec<DisplayMessage>,
    compose: String,
    sector_key: SectorKey,
    machine_did: String,
    channel_id: ChannelId,
    program_id: ProgramId,
    sector_id: SectorId,
    last_head_cid: Option<Cid>,
    version: u64,
    error: Option<String>,
    initialized: bool,
}

fn derive_test_sector_key() -> SectorKey {
    let hk = Hkdf::<Sha256>::new(None, b"zode-test-channel-v1");
    let mut key_bytes = [0u8; 32];
    hk.expand(b"zfs:test-channel-key:v1", &mut key_bytes)
        .expect("32-byte expand cannot fail");
    SectorKey::from_bytes(key_bytes)
}

fn derive_test_machine_did() -> String {
    let nk = NeuralKey::from_bytes([0x42; 32]);
    let identity_id = [0x01; 16];
    let machine_id = [0x02; 16];
    let caps = MachineKeyCapabilities::SIGN | MachineKeyCapabilities::ENCRYPT;
    let kp = derive_machine_keypair(&nk, &identity_id, &machine_id, 0, caps)
        .expect("deterministic derivation cannot fail");
    let pub_bytes = kp.public_key().ed25519_bytes();
    ed25519_to_did_key(&pub_bytes)
}

// ---------------------------------------------------------------------------
// Main app
// ---------------------------------------------------------------------------

struct ZodeApp {
    rt: Runtime,
    settings: Settings,
    zode: Option<Arc<Zode>>,
    shared: Arc<Mutex<AppState>>,
    tab: Tab,
    settings_error: Option<String>,
    shutdown_tx: Option<tokio::sync::mpsc::Sender<()>>,
    chat_state: Option<ChatState>,
}

impl ZodeApp {
    fn new(rt: Runtime) -> Self {
        let mut app = Self {
            rt,
            settings: Settings::default(),
            zode: None,
            shared: Arc::new(Mutex::new(AppState::default())),
            tab: Tab::Status,
            settings_error: None,
            shutdown_tx: None,
            chat_state: None,
        };
        app.boot_zode();
        app
    }

    fn boot_zode(&mut self) {
        let config = match self.settings.build_config() {
            Ok(c) => c,
            Err(e) => {
                self.settings_error = Some(e);
                return;
            }
        };
        self.settings_error = None;

        // Shut down existing instance if any.
        self.stop_zode();

        let shared = Arc::new(Mutex::new(AppState::default()));
        self.shared = Arc::clone(&shared);

        // Start the Zode and immediately grab peer_id + topics via the
        // network lock before the event loop's spawned task can acquire it.
        let start_result = self.rt.block_on(async {
            let zode = Zode::start(config).await?;
            let peer_id = zode.network().lock().await.local_peer_id().to_string();
            Ok::<_, ZodeError>((zode, peer_id))
        });
        match start_result {
            Ok((zode, boot_peer_id)) => {
                let zode = Arc::new(zode);
                self.zode = Some(Arc::clone(&zode));

                let (stop_tx, stop_rx) = tokio::sync::mpsc::channel::<()>(1);
                self.shutdown_tx = Some(stop_tx);

                // Status poller — The Zode event loop holds the network
                // mutex while awaiting events, which can starve status()
                // indefinitely when no peers are connected. We build a
                // fallback status from lock-free accessors and the
                // peer_id/topics captured at boot.
                let bg_zode = Arc::clone(&zode);
                let bg_shared = Arc::clone(&shared);
                let mut stop_poll = stop_rx;
                self.rt.spawn(async move {
                    loop {
                        tokio::select! {
                            _ = stop_poll.recv() => return,
                            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
                        }
                        let timeout = std::time::Duration::from_millis(250);
                        let status = match tokio::time::timeout(
                            timeout,
                            bg_zode.status(),
                        )
                        .await
                        {
                            Ok(full) => full,
                            Err(_) => {
                                let storage_stats =
                                    bg_zode.storage().stats().unwrap_or_default();
                                let metrics = bg_zode.metrics().snapshot();
                                ZodeStatus {
                                    peer_id: boot_peer_id.clone(),
                                    peer_count: metrics.peer_count,
                                    connected_peers: Vec::new(),
                                    topics: Vec::new(),
                                    storage: storage_stats,
                                    metrics,
                                }
                            }
                        };
                        bg_shared.lock().await.status = Some(status);
                    }
                });

                // Log event listener (also captures listen address)
                let log_shared = Arc::clone(&shared);
                let mut event_rx = zode.subscribe_events();
                self.rt.spawn(async move {
                    loop {
                        match event_rx.recv().await {
                            Ok(event) => {
                                let line = format_log_event(&event);
                                let mut state = log_shared.lock().await;
                                if let LogEvent::Started { ref listen_addr } = event {
                                    state.listen_addr = Some(listen_addr.clone());
                                }
                                if state.log_entries.len() >= MAX_LOG_ENTRIES {
                                    state.log_entries.pop_front();
                                }
                                state.log_entries.push_back(line);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                let mut state = log_shared.lock().await;
                                state
                                    .log_entries
                                    .push_back(format!("[WARN] lagged {n} events"));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                });
            }
            Err(e) => {
                self.settings_error = Some(format!("Start failed: {e}"));
            }
        }
    }

    fn stop_zode(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
        if let Some(ref zode) = self.zode {
            self.rt.block_on(zode.shutdown());
        }
        self.zode = None;
    }
}

impl eframe::App for ZodeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = self
            .rt
            .block_on(async { self.shared.lock().await.snapshot() });

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Status, "Status");
                ui.selectable_value(&mut self.tab, Tab::Traverse, "Traverse");
                ui.selectable_value(&mut self.tab, Tab::Peers, "Peers");
                ui.selectable_value(&mut self.tab, Tab::Log, "Log");
                ui.selectable_value(&mut self.tab, Tab::TestChat, "Test Chat");
                ui.selectable_value(&mut self.tab, Tab::Info, "Info");
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Status => self.render_status(ui, &state),
            Tab::Traverse => self.render_traverse(ui, &state),
            Tab::Peers => self.render_peers(ui, &state),
            Tab::Log => self.render_log(ui, &state),
            Tab::TestChat => self.render_test_chat(ui),
            Tab::Info => self.render_info(ui, &state),
            Tab::Settings => self.render_settings(ui),
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }
}

// ---------------------------------------------------------------------------
// Render: Settings tab (editable, with restart button)
// ---------------------------------------------------------------------------

impl ZodeApp {
    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.separator();
        ui.add_space(4.0);

        if let Some(ref err) = self.settings_error {
            ui.colored_label(egui::Color32::from_rgb(255, 80, 80), err.as_str());
            ui.add_space(4.0);
        }

        let running = self.zode.is_some();
        if running {
            ui.label(
                egui::RichText::new(
                    "The Zode is running. Edit settings below and click Restart to apply.",
                )
                .weak(),
            );
        } else {
            ui.label(
                egui::RichText::new("The Zode is stopped. Edit settings and click Start.")
                    .weak(),
            );
        }
        ui.add_space(8.0);

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.strong("Data Directory:");
                ui.add(egui::TextEdit::singleline(&mut self.settings.data_dir).desired_width(400.0));
                ui.end_row();

                ui.strong("Listen Address:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.listen_addr).desired_width(400.0),
                );
                ui.end_row();
            });

        ui.add_space(12.0);
        ui.strong("Bootstrap Peers");
        ui.label(
            egui::RichText::new("Multiaddrs of other Zode nodes to connect to on startup.")
                .weak()
                .small(),
        );
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.settings.bootstrap_input).desired_width(360.0),
            );
            if (ui.button("Add").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                && !self.settings.bootstrap_input.trim().is_empty()
            {
                let addr = self.settings.bootstrap_input.trim().to_string();
                self.settings.bootstrap_peers.push(addr);
                self.settings.bootstrap_input.clear();
            }
        });

        let mut remove_idx = None;
        for (i, peer) in self.settings.bootstrap_peers.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.monospace(peer);
                if ui.small_button("x").clicked() {
                    remove_idx = Some(i);
                }
            });
        }
        if let Some(idx) = remove_idx {
            self.settings.bootstrap_peers.remove(idx);
        }

        ui.add_space(12.0);
        ui.strong("Default Programs");
        ui.label(
            egui::RichText::new(
                "Standard programs the Zode subscribes to. Toggle off to skip.",
            )
            .weak()
            .small(),
        );
        ui.add_space(4.0);
        ui.checkbox(&mut self.settings.enable_zid, "ZID (Zero Identity)");
        ui.checkbox(&mut self.settings.enable_zchat, "Z Chat");

        ui.add_space(12.0);
        ui.strong("Additional Topics (Program IDs)");
        ui.label(
            egui::RichText::new("64-character hex program IDs for non-default programs.")
                .weak()
                .small(),
        );
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.settings.topic_input).desired_width(360.0),
            );
            if (ui.button("Add").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                && !self.settings.topic_input.trim().is_empty()
            {
                let topic = self.settings.topic_input.trim().to_string();
                self.settings.topics.push(topic);
                self.settings.topic_input.clear();
            }
        });

        let mut remove_topic = None;
        for (i, topic) in self.settings.topics.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.monospace(topic);
                if ui.small_button("x").clicked() {
                    remove_topic = Some(i);
                }
            });
        }
        if let Some(idx) = remove_topic {
            self.settings.topics.remove(idx);
        }

        ui.add_space(12.0);
        ui.strong("Discovery (Kademlia DHT)");
        ui.label(
            egui::RichText::new(
                "Automatic peer discovery via Kademlia DHT. Nodes find each other transitively.",
            )
            .weak()
            .small(),
        );
        ui.add_space(4.0);
        ui.checkbox(&mut self.settings.enable_kademlia, "Enable Kademlia DHT");

        if self.settings.enable_kademlia {
            ui.indent("kad_settings", |ui| {
                ui.checkbox(
                    &mut self.settings.kademlia_server_mode,
                    "Server mode (respond to DHT queries from other peers)",
                );
                ui.horizontal(|ui| {
                    ui.label("Random walk interval (seconds):");
                    ui.add(
                        egui::DragValue::new(&mut self.settings.random_walk_interval_secs)
                            .range(5..=300)
                            .speed(1),
                    );
                });
            });
        }

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let label = if running {
                "Restart Zode"
            } else {
                "Start Zode"
            };
            let btn = egui::Button::new(egui::RichText::new(label).strong().size(15.0));
            if ui.add(btn).clicked() {
                self.boot_zode();
            }

            if running {
                ui.add_space(12.0);
                let stop_btn =
                    egui::Button::new(egui::RichText::new("Stop Zode").size(15.0));
                if ui.add(stop_btn).clicked() {
                    self.stop_zode();
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Render: running tabs
// ---------------------------------------------------------------------------

impl ZodeApp {
    fn render_status(&self, ui: &mut egui::Ui, state: &StateSnapshot) {
        let Some(ref status) = state.status else {
            ui.spinner();
            ui.label(if self.zode.is_some() {
                "Starting Zode..."
            } else {
                "Zode is stopped. Go to Settings to start."
            });
            return;
        };

        ui.heading("Node Status");
        ui.separator();

        // Full connectable address: listen_addr/p2p/peer_id
        let full_addr = state.listen_addr.as_ref().map(|listen| {
            format!("{listen}/p2p/{}", status.peer_id)
        });

        egui::Grid::new("status_grid")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Peer ID:");
                ui.horizontal(|ui| {
                    ui.label(&status.peer_id);
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(status.peer_id.clone());
                    }
                });
                ui.end_row();

                ui.strong("Address:");
                ui.horizontal(|ui| {
                    if let Some(ref addr) = full_addr {
                        ui.monospace(addr);
                        if ui.small_button("Copy").clicked() {
                            ui.ctx().copy_text(addr.clone());
                        }
                    } else {
                        ui.label(egui::RichText::new("resolving...").weak());
                    }
                });
                ui.end_row();

                ui.strong("Peers:");
                ui.label(format!("{}", status.peer_count));
                ui.end_row();

                ui.strong("Topics:");
                ui.label(format!("{}", status.topics.len()));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.heading("Storage");
        ui.separator();

        egui::Grid::new("storage_grid")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                ui.strong("DB Size:");
                ui.label(format_bytes(status.storage.db_size_bytes));
                ui.end_row();

                ui.strong("Blocks:");
                ui.label(format!("{}", status.storage.block_count));
                ui.end_row();

                ui.strong("Heads:");
                ui.label(format!("{}", status.storage.head_count));
                ui.end_row();

                ui.strong("Programs:");
                ui.label(format!("{}", status.storage.program_count));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.heading("Metrics");
        ui.separator();

        let m = &status.metrics;
        egui::Grid::new("metrics_grid")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Blocks stored:");
                ui.label(format!("{}", m.blocks_stored_total));
                ui.end_row();

                ui.strong("Rejections:");
                ui.label(format!(
                    "{} (policy: {}, proof: {}, limit: {})",
                    m.store_rejections_total,
                    m.policy_rejections,
                    m.proof_rejections,
                    m.limit_rejections,
                ));
                ui.end_row();
            });
    }

    fn render_traverse(&self, ui: &mut egui::Ui, state: &StateSnapshot) {
        let Some(ref status) = state.status else {
            ui.spinner();
            return;
        };

        ui.heading("Program Traverse");
        ui.separator();

        if status.topics.is_empty() {
            ui.label("No subscribed programs.");
            return;
        }

        if let Some(ref zode) = self.zode {
            for topic in &status.topics {
                let Some(hex) = topic.strip_prefix("prog/") else {
                    continue;
                };
                let Ok(pid) = zfs_core::ProgramId::from_hex(hex) else {
                    continue;
                };

                ui.collapsing(format!("Program: {}", &hex[..16.min(hex.len())]), |ui| {
                    let cids = zode.storage().list_cids(&pid).unwrap_or_default();
                    if cids.is_empty() {
                        ui.label("  No CIDs stored.");
                    } else {
                        for cid in &cids {
                            ui.monospace(format!("  {}", cid.to_hex()));
                        }
                    }
                });
            }
        }
    }

    fn render_peers(&self, ui: &mut egui::Ui, state: &StateSnapshot) {
        let Some(ref status) = state.status else {
            ui.spinner();
            return;
        };

        ui.heading("Connected Peers");
        ui.separator();

        egui::Grid::new("peer_info")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Local Peer:");
                ui.label(&status.peer_id);
                ui.end_row();

                ui.strong("Connected:");
                ui.label(format!("{}", status.peer_count));
                ui.end_row();
            });

        ui.add_space(8.0);

        if status.connected_peers.is_empty() {
            ui.label(egui::RichText::new("No connected peers.").weak());
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for peer in &status.connected_peers {
                        ui.horizontal(|ui| {
                            ui.monospace(peer);
                            if ui.small_button("Copy").clicked() {
                                ui.ctx().copy_text(peer.clone());
                            }
                        });
                    }
                });
        }

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Peer discovery via GossipSub / bootstrap peers / Kademlia DHT.").weak(),
        );
    }

    fn render_log(&self, ui: &mut egui::Ui, state: &StateSnapshot) {
        ui.heading(format!("Live Log ({})", state.log_entries.len()));
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in &state.log_entries {
                    let color = if entry.starts_with("[STORE REJECT") {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else if entry.starts_with("[DHT") {
                        egui::Color32::from_rgb(100, 150, 255)
                    } else if entry.starts_with("[PEER+") {
                        egui::Color32::from_rgb(100, 255, 100)
                    } else if entry.starts_with("[PEER-") {
                        egui::Color32::from_rgb(255, 255, 100)
                    } else if entry.starts_with("[SHUTDOWN") {
                        egui::Color32::from_rgb(200, 100, 255)
                    } else {
                        egui::Color32::from_rgb(200, 200, 200)
                    };
                    ui.label(egui::RichText::new(entry).monospace().color(color));
                }
            });
    }

    fn render_info(&self, ui: &mut egui::Ui, state: &StateSnapshot) {
        let Some(ref status) = state.status else {
            ui.spinner();
            return;
        };

        ui.heading("Zode Info");
        ui.separator();

        egui::Grid::new("info_grid")
            .num_columns(2)
            .spacing([20.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Peer ID:");
                ui.label(&status.peer_id);
                ui.end_row();

                ui.strong("DB Size:");
                ui.label(format_bytes(status.storage.db_size_bytes));
                ui.end_row();

                ui.strong("Block Count:");
                ui.label(format!("{}", status.storage.block_count));
                ui.end_row();

                ui.strong("Head Count:");
                ui.label(format!("{}", status.storage.head_count));
                ui.end_row();

                ui.strong("Program Count:");
                ui.label(format!("{}", status.storage.program_count));
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.heading("Subscribed Topics");
        ui.separator();

        for topic in &status.topics {
            ui.monospace(format!("  {topic}"));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(format!("zfs-zode-app v{}", env!("CARGO_PKG_VERSION")));
    }
}

// ---------------------------------------------------------------------------
// Test Chat tab
// ---------------------------------------------------------------------------

impl ZodeApp {
    fn init_chat(&mut self) {
        let sector_key = derive_test_sector_key();
        // PQ key derivation (ML-DSA-65 / ML-KEM-768) needs more stack than
        // the main thread provides on Windows debug builds, and we can't use
        // rt.block_on here because update() already holds a block_on context.
        let machine_did = std::thread::spawn(derive_test_machine_did)
            .join()
            .expect("key derivation thread panicked");
        let channel_id = ChannelId::from_str_id(TEST_CHANNEL_ID);
        let sector_id = channel_id.sector_id();
        let program_id = ZChatDescriptor::v1()
            .program_id()
            .expect("ZChat descriptor is valid");

        let mut chat = ChatState {
            messages: Vec::new(),
            compose: String::new(),
            sector_key,
            machine_did,
            channel_id,
            program_id,
            sector_id,
            last_head_cid: None,
            version: 0,
            error: None,
            initialized: true,
        };

        if let Some(ref zode) = self.zode {
            Self::load_messages(zode.storage(), &mut chat);
        }

        self.chat_state = Some(chat);
    }

    fn load_messages(
        storage: &Arc<zfs_storage::RocksStorage>,
        chat: &mut ChatState,
    ) {
        let aad = build_aad(&chat.program_id, &chat.sector_id);

        // Restore latest head metadata (version, last_head_cid)
        match storage.get_head(&chat.sector_id) {
            Ok(Some(h)) => {
                chat.last_head_cid = Some(h.cid);
                chat.version = h.version;
            }
            Ok(None) => {
                chat.messages = Vec::new();
                chat.last_head_cid = None;
                chat.version = 0;
                chat.error = None;
                return;
            }
            Err(e) => {
                chat.error = Some(format!("Failed to read head: {e}"));
                return;
            }
        }

        // Load all blocks via the program index
        let cids = match storage.list_cids(&chat.program_id) {
            Ok(c) => c,
            Err(e) => {
                chat.error = Some(format!("Failed to list CIDs: {e}"));
                return;
            }
        };

        let mut msgs = Vec::new();
        for cid in &cids {
            match storage.get(cid) {
                Ok(Some(ciphertext)) => {
                    match decrypt_sector(&ciphertext, &chat.sector_key, &aad) {
                        Ok(plaintext) => match ZChatMessage::decode_canonical(&plaintext) {
                            Ok(msg) => {
                                msgs.push(DisplayMessage {
                                    sender: msg.sender_did,
                                    content: msg.content,
                                    timestamp_ms: msg.timestamp_ms,
                                });
                            }
                            Err(e) => {
                                msgs.push(DisplayMessage {
                                    sender: "system".into(),
                                    content: format!("[decode error: {e}]"),
                                    timestamp_ms: 0,
                                });
                            }
                        },
                        Err(e) => {
                            msgs.push(DisplayMessage {
                                sender: "system".into(),
                                content: format!("[decrypt error: {e}]"),
                                timestamp_ms: 0,
                            });
                        }
                    }
                }
                Ok(None) => {
                    msgs.push(DisplayMessage {
                        sender: "system".into(),
                        content: format!("[block not found: {}]", cid.to_hex()),
                        timestamp_ms: 0,
                    });
                }
                Err(e) => {
                    chat.error = Some(format!("Storage read error: {e}"));
                    break;
                }
            }
        }

        msgs.sort_by_key(|m| m.timestamp_ms);
        chat.messages = msgs;
        chat.error = None;
    }

    fn send_message(&mut self) {
        let Some(ref zode) = self.zode else {
            if let Some(ref mut chat) = self.chat_state {
                chat.error = Some("Zode is not running".into());
            }
            return;
        };
        let storage = Arc::clone(zode.storage());

        let chat = self.chat_state.as_mut().unwrap();
        let text = chat.compose.trim().to_string();
        if text.is_empty() {
            return;
        }
        chat.compose.clear();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let msg = ZChatMessage {
            sender_did: chat.machine_did.clone(),
            channel_id: chat.channel_id.clone(),
            content: text.clone(),
            timestamp_ms: now_ms,
        };

        let cbor = match msg.encode_canonical() {
            Ok(b) => b,
            Err(e) => {
                chat.error = Some(format!("Encode failed: {e}"));
                return;
            }
        };

        let aad = build_aad(&chat.program_id, &chat.sector_id);
        let ciphertext = match encrypt_sector(&cbor, &chat.sector_key, &aad) {
            Ok(ct) => ct,
            Err(e) => {
                chat.error = Some(format!("Encrypt failed: {e}"));
                return;
            }
        };

        let cid = Cid::from_ciphertext(&ciphertext);
        chat.version += 1;

        let head = Head {
            sector_id: chat.sector_id.clone(),
            cid,
            version: chat.version,
            program_id: chat.program_id,
            prev_head_cid: chat.last_head_cid,
            timestamp_ms: now_ms,
            signature: None,
        };

        if let Err(e) = storage.put(&cid, &ciphertext) {
            chat.error = Some(format!("Block write failed: {e}"));
            return;
        }
        if let Err(e) = storage.put_head(&chat.sector_id, &head) {
            chat.error = Some(format!("Head write failed: {e}"));
            return;
        }
        if let Err(e) = storage.add_cid(&chat.program_id, &cid) {
            chat.error = Some(format!("Index write failed: {e}"));
            return;
        }

        chat.last_head_cid = Some(cid);
        chat.messages.push(DisplayMessage {
            sender: chat.machine_did.clone(),
            content: text,
            timestamp_ms: now_ms,
        });
        chat.error = None;
    }

    fn render_test_chat(&mut self, ui: &mut egui::Ui) {
        if self.zode.is_none() {
            ui.spinner();
            ui.label("Zode is stopped. Go to Settings to start.");
            return;
        }

        if self.chat_state.is_none() || !self.chat_state.as_ref().unwrap().initialized {
            self.init_chat();
        }

        // Channel info header
        let (sector_key_preview, channel_id_display, msg_count) = {
            let chat = self.chat_state.as_ref().unwrap();
            let key_bytes = chat.sector_key.as_bytes();
            let preview: String = key_bytes[..8]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let ch_display = String::from_utf8_lossy(chat.channel_id.as_bytes()).to_string();
            (preview, ch_display, chat.messages.len())
        };

        ui.heading("Test Chat");
        ui.separator();

        egui::Grid::new("chat_info_grid")
            .num_columns(2)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.strong("Channel:");
                ui.label(&channel_id_display);
                ui.end_row();

                ui.strong("Sector Key:");
                ui.label(
                    egui::RichText::new(format!("{sector_key_preview}..."))
                        .monospace()
                        .weak(),
                );
                ui.end_row();

                ui.strong("Messages:");
                ui.label(format!("{msg_count}"));
                ui.end_row();
            });

        ui.add_space(4.0);

        // Error display
        if let Some(ref err) = self.chat_state.as_ref().unwrap().error {
            ui.colored_label(egui::Color32::from_rgb(255, 80, 80), err.as_str());
            ui.add_space(4.0);
        }

        // Refresh button
        if ui.button("Refresh").clicked() {
            if let Some(ref zode) = self.zode {
                let storage = Arc::clone(zode.storage());
                let chat = self.chat_state.as_mut().unwrap();
                Self::load_messages(&storage, chat);
            }
        }
        ui.add_space(4.0);
        ui.separator();

        // Message list
        let available = ui.available_height() - 40.0;
        egui::ScrollArea::vertical()
            .max_height(available.max(100.0))
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let chat = self.chat_state.as_ref().unwrap();
                if chat.messages.is_empty() {
                    ui.label(
                        egui::RichText::new("No messages yet. Type something below!")
                            .weak()
                            .italics(),
                    );
                } else {
                    for msg in &chat.messages {
                        let time = format_timestamp_ms(msg.timestamp_ms);
                        let sender_short = if msg.sender.len() > 16 {
                            &msg.sender[..16]
                        } else {
                            &msg.sender
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("[{time}]"))
                                    .monospace()
                                    .weak(),
                            );
                            ui.label(
                                egui::RichText::new(format!("{sender_short}:"))
                                    .monospace()
                                    .strong(),
                            );
                            ui.label(&msg.content);
                        });
                    }
                }
            });

        ui.separator();

        // Compose area
        let mut do_send = false;
        ui.horizontal(|ui| {
            let chat = self.chat_state.as_mut().unwrap();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut chat.compose)
                    .desired_width(ui.available_width() - 70.0)
                    .hint_text("Type a message..."),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                do_send = true;
                resp.request_focus();
            }
            if ui.button("Send").clicked() {
                do_send = true;
            }
        });

        if do_send {
            self.send_message();
        }
    }
}

fn build_aad(program_id: &ProgramId, sector_id: &SectorId) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32 + sector_id.as_bytes().len());
    aad.extend_from_slice(program_id.as_bytes());
    aad.extend_from_slice(sector_id.as_bytes());
    aad
}

fn format_timestamp_ms(ms: u64) -> String {
    if ms == 0 {
        return "--:--:--".to_string();
    }
    let secs = ms / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_log_event(event: &LogEvent) -> String {
    match event {
        LogEvent::Started { listen_addr } => format!("[STARTED] listening on {listen_addr}"),
        LogEvent::PeerConnected(peer) => format!("[PEER+] {peer}"),
        LogEvent::PeerDisconnected(peer) => format!("[PEER-] {peer}"),
        LogEvent::StoreProcessed {
            program_id,
            cid,
            accepted,
            reason,
        } => {
            let status = if *accepted { "OK" } else { "REJECT" };
            let detail = reason.as_deref().unwrap_or("");
            format!(
                "[STORE {status}] prog={} cid={} {detail}",
                &program_id[..8.min(program_id.len())],
                &cid[..8.min(cid.len())]
            )
        }
        LogEvent::FetchProcessed { program_id, found } => {
            let status = if *found { "FOUND" } else { "MISS" };
            format!(
                "[FETCH {status}] prog={}",
                &program_id[..8.min(program_id.len())]
            )
        }
        LogEvent::PeerDiscovered(peer) => format!("[DHT] discovered {peer}"),
        LogEvent::ShuttingDown => "[SHUTDOWN] Zode shutting down".to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
