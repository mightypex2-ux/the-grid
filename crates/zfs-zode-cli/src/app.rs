use std::collections::VecDeque;

use zfs_core::{Cid, Head, ProgramId};
use zfs_storage::{HeadStore, ProgramIndex, StorageStats};
use zfs_zode::{LogEvent, MetricsSnapshot, Zode, ZodeStatus};

const MAX_LOG_ENTRIES: usize = 500;

/// Active TUI screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Status,
    Traverse,
    Peers,
    Log,
    Info,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Status,
        Screen::Traverse,
        Screen::Peers,
        Screen::Log,
        Screen::Info,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Status => "Status",
            Screen::Traverse => "Traverse",
            Screen::Peers => "Peers",
            Screen::Log => "Log",
            Screen::Info => "Info",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }
}

/// Sub-view within the Traverse screen.
#[derive(Debug, Clone)]
pub enum TraverseView {
    ProgramList,
    CidList { program_id: ProgramId },
    HeadDetail { head: Head },
}

/// TUI application state — reads all data from the Zode in-process API.
pub struct App<'z> {
    pub zode: &'z Zode,
    pub screen: Screen,

    // Cached data (refreshed each tick)
    pub status: Option<ZodeStatus>,
    pub storage_stats: Option<StorageStats>,
    pub metrics: Option<MetricsSnapshot>,

    // Traverse state
    pub traverse: TraverseView,
    pub programs: Vec<ProgramId>,
    pub cids: Vec<Cid>,

    // Log buffer
    pub log_entries: VecDeque<String>,
    event_rx: Option<tokio::sync::broadcast::Receiver<LogEvent>>,

    // Scroll position per screen
    pub scroll_offset: usize,
    pub list_len: usize,
}

impl<'z> App<'z> {
    pub fn new(zode: &'z Zode) -> Self {
        let event_rx = Some(zode.subscribe_events());
        Self {
            zode,
            screen: Screen::Status,
            status: None,
            storage_stats: None,
            metrics: None,
            traverse: TraverseView::ProgramList,
            programs: Vec::new(),
            cids: Vec::new(),
            log_entries: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            event_rx,
            scroll_offset: 0,
            list_len: 0,
        }
    }

    /// Refresh cached data from the Zode.
    pub async fn refresh(&mut self) {
        let status = self.zode.status();
        self.storage_stats = Some(status.storage.clone());
        self.metrics = Some(status.metrics.clone());

        self.programs = status
            .topics
            .iter()
            .filter_map(|t: &String| {
                let hex = t.strip_prefix("prog/")?;
                ProgramId::from_hex(hex).ok()
            })
            .collect();

        self.status = Some(status);

        if let TraverseView::CidList { ref program_id } = self.traverse {
            let pid = *program_id;
            self.cids = self
                .zode
                .storage()
                .list_cids(&pid)
                .unwrap_or_default();
        }

        self.drain_events();
    }

    fn drain_events(&mut self) {
        if let Some(ref mut rx) = self.event_rx {
            while let Ok(event) = rx.try_recv() {
                let line = format_log_event(&event);
                if self.log_entries.len() >= MAX_LOG_ENTRIES {
                    self.log_entries.pop_front();
                }
                self.log_entries.push_back(line);
            }
        }
    }

    pub fn next_screen(&mut self) {
        let idx = (self.screen.index() + 1) % Screen::ALL.len();
        self.screen = Screen::ALL[idx];
        self.scroll_offset = 0;
    }

    pub fn prev_screen(&mut self) {
        let idx = (self.screen.index() + Screen::ALL.len() - 1) % Screen::ALL.len();
        self.screen = Screen::ALL[idx];
        self.scroll_offset = 0;
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.list_len > 0 && self.scroll_offset < self.list_len.saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    /// Handle Enter key — drill into traverse views or no-op on other screens.
    pub fn select(&mut self) {
        if self.screen == Screen::Traverse {
            match &self.traverse {
                TraverseView::ProgramList => {
                    if let Some(pid) = self.programs.get(self.scroll_offset) {
                        self.traverse = TraverseView::CidList { program_id: *pid };
                        self.scroll_offset = 0;
                    }
                }
                TraverseView::CidList { program_id: _ } => {
                    if let Some(cid) = self.cids.get(self.scroll_offset) {
                        let sector_id =
                            zfs_core::SectorId::from_bytes(cid.as_bytes().to_vec());
                        if let Ok(Some(head)) =
                            self.zode.storage().get_head(&sector_id)
                        {
                            self.traverse = TraverseView::HeadDetail { head };
                            self.scroll_offset = 0;
                        }
                    }
                }
                TraverseView::HeadDetail { .. } => {}
            }
        }
    }

    /// Handle Backspace — go back in traverse drill-down.
    pub fn back(&mut self) {
        if self.screen == Screen::Traverse {
            match &self.traverse {
                TraverseView::CidList { .. } | TraverseView::HeadDetail { .. } => {
                    self.traverse = TraverseView::ProgramList;
                    self.scroll_offset = 0;
                }
                TraverseView::ProgramList => {}
            }
        }
    }
}

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
            format!("[STORE {status}] prog={} cid={} {detail}", &program_id[..8.min(program_id.len())], &cid[..8.min(cid.len())])
        }
        LogEvent::FetchProcessed { program_id, found } => {
            let status = if *found { "FOUND" } else { "MISS" };
            format!("[FETCH {status}] prog={}", &program_id[..8.min(program_id.len())])
        }
        LogEvent::PeerDiscovered(peer) => format!("[DHT] discovered {peer}"),
        LogEvent::GossipReceived {
            program_id,
            cid,
            accepted,
        } => {
            let status = if *accepted { "OK" } else { "DROP" };
            format!(
                "[GOSSIP {status}] prog={} cid={}",
                &program_id[..8.min(program_id.len())],
                &cid[..8.min(cid.len())]
            )
        }
        LogEvent::ShuttingDown => "[SHUTDOWN] Zode shutting down".to_string(),
    }
}
