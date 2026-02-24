use std::collections::VecDeque;

use zfs_core::{Cid, ProgramId, SectorId};
use zfs_crypto::SectorKey;
use zfs_programs::zchat::ChannelId;
use zfs_zode::ZodeStatus;

pub(crate) const MAX_LOG_ENTRIES: usize = 500;

pub(crate) enum PeerEvent {
    Connected(String),
    Disconnected(String),
    Discovered(String),
}

#[derive(Default)]
pub(crate) struct AppState {
    pub status: Option<ZodeStatus>,
    pub log_entries: VecDeque<String>,
    pub listen_addr: Option<String>,
    pub peer_events: VecDeque<PeerEvent>,
}

impl AppState {
    pub fn snapshot(&mut self) -> StateSnapshot {
        StateSnapshot {
            status: self.status.clone(),
            log_entries: self.log_entries.iter().cloned().collect(),
            listen_addr: self.listen_addr.clone(),
            peer_events: self.peer_events.drain(..).collect(),
        }
    }
}

pub(crate) struct StateSnapshot {
    pub status: Option<ZodeStatus>,
    pub log_entries: Vec<String>,
    pub listen_addr: Option<String>,
    pub peer_events: Vec<PeerEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Status,
    Storage,
    Peers,
    Log,
    Chat,
    Info,
    Settings,
}

pub(crate) struct DisplayMessage {
    pub sender: String,
    pub content: String,
    pub timestamp_ms: u64,
}

pub(crate) struct ChatUpdate {
    pub messages: Vec<DisplayMessage>,
    pub last_head_cid: Option<Cid>,
    pub version: u64,
    pub error: Option<String>,
}

pub(crate) struct ChatState {
    pub messages: Vec<DisplayMessage>,
    pub compose: String,
    pub sector_key: SectorKey,
    pub machine_did: String,
    pub channel_id: ChannelId,
    pub program_id: ProgramId,
    pub sector_id: SectorId,
    pub last_head_cid: Option<Cid>,
    pub version: u64,
    pub error: Option<String>,
    pub initialized: bool,
    pub scroll_to_bottom: bool,
    pub update_rx: tokio::sync::mpsc::Receiver<ChatUpdate>,
    pub refresh_tx: tokio::sync::mpsc::Sender<()>,
}
