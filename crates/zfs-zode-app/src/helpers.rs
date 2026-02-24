use zfs_zode::LogEvent;

pub(crate) fn format_log_event(event: &LogEvent) -> String {
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

pub(crate) fn format_bytes(bytes: u64) -> String {
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

pub(crate) fn format_timestamp_ms(ms: u64) -> String {
    if ms == 0 {
        return "--:--:--".to_string();
    }
    let secs = ms / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
