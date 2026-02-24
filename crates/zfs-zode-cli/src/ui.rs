use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{App, Screen, TraverseView};

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    render_tabs(frame, app, chunks[0]);

    match app.screen {
        Screen::Status => render_status(frame, app, chunks[1]),
        Screen::Traverse => render_traverse(frame, app, chunks[1]),
        Screen::Peers => render_peers(frame, app, chunks[1]),
        Screen::Log => render_log(frame, app, chunks[1]),
        Screen::Info => render_info(frame, app, chunks[1]),
    }

    render_help(frame, app.screen, chunks[2]);
}

fn render_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Screen::ALL
        .iter()
        .enumerate()
        .map(|(i, s)| Line::from(format!(" {} {} ", i + 1, s.label())))
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Zode CLI "))
        .select(app.screen.index())
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn render_help(frame: &mut Frame, screen: Screen, area: Rect) {
    let traverse_hint = match screen {
        Screen::Traverse => " | Enter: drill in | Backspace: back",
        _ => "",
    };
    let text = format!(
        " q: quit | Tab/1-5: switch screen | ↑↓/jk: scroll{traverse_hint}"
    );
    let help = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, area);
}

fn render_status(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Status ");

    let Some(ref status) = app.status else {
        frame.render_widget(Paragraph::new("Loading...").block(block), area);
        return;
    };

    let mut lines = build_zode_lines(status);
    lines.extend(build_storage_lines(status));
    lines.extend(build_metrics_lines(&status.metrics));

    app.list_len = 0;
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn build_zode_lines(status: &zfs_zode::ZodeStatus) -> Vec<Line<'static>> {
    vec![
        kv_line_owned("Zode ID:  ", status.zode_id.clone()),
        kv_line_owned("Peers:    ", format!("{}", status.peer_count)),
        Line::from(""),
        kv_line_owned("Topics:   ", format!("{}", status.topics.len())),
        Line::from(
            status
                .topics
                .iter()
                .map(|t| format!("  {t}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    ]
}

fn build_storage_lines(status: &zfs_zode::ZodeStatus) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        section_header("── Storage ──"),
        kv_line_owned("DB size:  ", format_bytes(status.storage.db_size_bytes)),
        kv_line_owned("Blocks:   ", format!("{}", status.storage.block_count)),
        kv_line_owned("Heads:    ", format!("{}", status.storage.head_count)),
        kv_line_owned("Programs: ", format!("{}", status.storage.program_count)),
    ]
}

fn build_metrics_lines(m: &zfs_zode::MetricsSnapshot) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        section_header("── Metrics ──"),
        kv_line_owned("Stored:     ", format!("{}", m.blocks_stored_total)),
        kv_line_owned(
            "Rejections: ",
            format!(
                "{} (policy: {}, proof: {}, limit: {})",
                m.store_rejections_total, m.policy_rejections, m.proof_rejections, m.limit_rejections,
            ),
        ),
    ]
}

fn render_traverse(frame: &mut Frame, app: &mut App, area: Rect) {
    let view = app.traverse.clone();
    match view {
        TraverseView::ProgramList => render_program_list(frame, app, area),
        TraverseView::CidList { program_id } => render_cid_list(frame, app, &program_id, area),
        TraverseView::HeadDetail { head } => render_head_detail(frame, app, &head, area),
    }
}

fn render_program_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .programs
        .iter()
        .enumerate()
        .map(|(i, pid)| {
            let style = highlight_style(i == app.scroll_offset);
            ListItem::new(Line::from(Span::styled(pid.to_hex(), style)))
        })
        .collect();

    app.list_len = items.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Programs ({}) ", items.len()));
    frame.render_widget(List::new(items).block(block), area);
}

fn render_cid_list(
    frame: &mut Frame,
    app: &mut App,
    program_id: &zfs_core::ProgramId,
    area: Rect,
) {
    let items: Vec<ListItem> = app
        .cids
        .iter()
        .enumerate()
        .map(|(i, cid)| {
            let style = highlight_style(i == app.scroll_offset);
            ListItem::new(Line::from(Span::styled(cid.to_hex(), style)))
        })
        .collect();

    app.list_len = items.len();
    let title = format!(
        " CIDs for {}.. ({}) ",
        &program_id.to_hex()[..8],
        items.len()
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    frame.render_widget(List::new(items).block(block), area);
}

fn render_head_detail(
    frame: &mut Frame,
    app: &mut App,
    head: &zfs_core::Head,
    area: Rect,
) {
    app.list_len = 0;
    let prev = head
        .prev_head_cid
        .as_ref()
        .map(|c| c.to_hex())
        .unwrap_or_else(|| "(none)".into());

    let lines = vec![
        kv_line_owned("Sector ID:    ", head.sector_id.to_hex()),
        kv_line_owned("CID:          ", head.cid.to_hex()),
        kv_line_owned("Version:      ", format!("{}", head.version)),
        kv_line_owned("Program ID:   ", head.program_id.to_hex()),
        kv_line_owned("Prev Head:    ", prev),
        kv_line_owned("Timestamp:    ", format!("{} ms", head.timestamp_ms)),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Head Detail ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_peers(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Connected Peers ");

    let Some(ref status) = app.status else {
        frame.render_widget(Paragraph::new("Loading...").block(block), area);
        return;
    };

    let mut lines = vec![
        kv_line_owned("Local Zode: ", status.zode_id.clone()),
        kv_line_owned("Connected:  ", format!("{}", status.peer_count)),
        Line::from(""),
    ];

    if status.connected_peers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No connected Zodes.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, peer) in status.connected_peers.iter().enumerate() {
            let style = highlight_style(i == app.scroll_offset);
            lines.push(Line::from(Span::styled(format!("  {peer}"), style)));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Peer discovery via GossipSub / bootstrap peers / Kademlia DHT.",
        Style::default().fg(Color::DarkGray),
    )));

    app.list_len = status.connected_peers.len();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_log(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Live Log ({}) ", app.log_entries.len()));

    let visible_height = area.height.saturating_sub(2) as usize;
    let total = app.log_entries.len();
    app.list_len = total;
    let start = total.saturating_sub(visible_height);

    let items: Vec<ListItem> = app
        .log_entries
        .iter()
        .skip(start)
        .map(|entry| {
            ListItem::new(Line::from(Span::styled(
                entry.as_str(),
                log_entry_style(entry),
            )))
        })
        .collect();

    frame.render_widget(List::new(items).block(block), area);
}

fn render_info(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Zode Info ");

    let Some(ref status) = app.status else {
        frame.render_widget(Paragraph::new("Loading...").block(block), area);
        return;
    };

    let mut lines = vec![
        kv_line_owned("Zode ID:        ", status.zode_id.clone()),
        kv_line_owned("DB Size:        ", format_bytes(status.storage.db_size_bytes)),
        kv_line_owned("Block Count:    ", format!("{}", status.storage.block_count)),
        kv_line_owned("Head Count:     ", format!("{}", status.storage.head_count)),
        kv_line_owned("Program Count:  ", format!("{}", status.storage.program_count)),
        Line::from(""),
        section_header("── Subscribed Topics ──"),
    ];
    for topic in &status.topics {
        lines.push(Line::from(format!("  {topic}")));
    }
    lines.push(Line::from(""));
    lines.push(section_header("── Version ──"));
    lines.push(Line::from(format!(
        "  zfs-zode-cli v{}",
        env!("CARGO_PKG_VERSION")
    )));

    app.list_len = 0;
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

// ---- shared helpers ----

fn kv_line_owned(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(label, Style::default().fg(Color::Yellow)),
        Span::raw(value),
    ])
}

fn section_header(title: &'static str) -> Line<'static> {
    Line::from(Span::styled(title, Style::default().fg(Color::Cyan)))
}

fn highlight_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

fn log_entry_style(entry: &str) -> Style {
    if entry.starts_with("[REJECT") || entry.starts_with("[STORE REJECT") {
        Style::default().fg(Color::Red)
    } else if entry.starts_with("[DHT") {
        Style::default().fg(Color::Blue)
    } else if entry.starts_with("[PEER+") {
        Style::default().fg(Color::Green)
    } else if entry.starts_with("[PEER-") {
        Style::default().fg(Color::Yellow)
    } else if entry.starts_with("[SHUTDOWN") {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default()
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
