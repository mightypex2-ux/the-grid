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
            Constraint::Length(3), // tab bar
            Constraint::Min(0),   // content
            Constraint::Length(1), // help bar
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
        .map(|(i, s)| {
            let label = format!(" {} {} ", i + 1, s.label());
            Line::from(label)
        })
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

    let metrics = &status.metrics;

    let lines = vec![
        Line::from(vec![
            Span::styled("Peer ID:  ", Style::default().fg(Color::Yellow)),
            Span::raw(&status.peer_id),
        ]),
        Line::from(vec![
            Span::styled("Peers:    ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.peer_count)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Topics:   ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.topics.len())),
        ]),
        Line::from(
            status
                .topics
                .iter()
                .map(|t| format!("  {t}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Line::from(""),
        Line::from(Span::styled(
            "── Storage ──",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(vec![
            Span::styled("DB size:  ", Style::default().fg(Color::Yellow)),
            Span::raw(format_bytes(status.storage.db_size_bytes)),
        ]),
        Line::from(vec![
            Span::styled("Blocks:   ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.storage.block_count)),
        ]),
        Line::from(vec![
            Span::styled("Heads:    ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.storage.head_count)),
        ]),
        Line::from(vec![
            Span::styled("Programs: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.storage.program_count)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "── Metrics ──",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(vec![
            Span::styled("Stored:     ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", metrics.blocks_stored_total)),
        ]),
        Line::from(vec![
            Span::styled("Rejections: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!(
                "{} (policy: {}, proof: {}, limit: {})",
                metrics.store_rejections_total,
                metrics.policy_rejections,
                metrics.proof_rejections,
                metrics.limit_rejections,
            )),
        ]),
    ];

    app.list_len = 0;
    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_traverse(frame: &mut Frame, app: &mut App, area: Rect) {
    match &app.traverse {
        TraverseView::ProgramList => {
            let items: Vec<ListItem> = app
                .programs
                .iter()
                .enumerate()
                .map(|(i, pid)| {
                    let style = if i == app.scroll_offset {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(Span::styled(pid.to_hex(), style)))
                })
                .collect();

            app.list_len = items.len();

            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" Programs ({}) ", items.len()));

            let list = List::new(items).block(block);
            frame.render_widget(list, area);
        }
        TraverseView::CidList { program_id } => {
            let items: Vec<ListItem> = app
                .cids
                .iter()
                .enumerate()
                .map(|(i, cid)| {
                    let style = if i == app.scroll_offset {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
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
            let list = List::new(items).block(block);
            frame.render_widget(list, area);
        }
        TraverseView::HeadDetail { head } => {
            app.list_len = 0;

            let lines = vec![
                Line::from(vec![
                    Span::styled("Sector ID:    ", Style::default().fg(Color::Yellow)),
                    Span::raw(head.sector_id.to_hex()),
                ]),
                Line::from(vec![
                    Span::styled("CID:          ", Style::default().fg(Color::Yellow)),
                    Span::raw(head.cid.to_hex()),
                ]),
                Line::from(vec![
                    Span::styled("Version:      ", Style::default().fg(Color::Yellow)),
                    Span::raw(format!("{}", head.version)),
                ]),
                Line::from(vec![
                    Span::styled("Program ID:   ", Style::default().fg(Color::Yellow)),
                    Span::raw(head.program_id.to_hex()),
                ]),
                Line::from(vec![
                    Span::styled("Prev Head:    ", Style::default().fg(Color::Yellow)),
                    Span::raw(
                        head.prev_head_cid
                            .as_ref()
                            .map(|c| c.to_hex())
                            .unwrap_or_else(|| "(none)".into()),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Timestamp:    ", Style::default().fg(Color::Yellow)),
                    Span::raw(format!("{} ms", head.timestamp_ms)),
                ]),
            ];

            let block = Block::default()
                .borders(Borders::ALL)
                .title(" Head Detail ");
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
    }
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
        Line::from(vec![
            Span::styled("Local Peer: ", Style::default().fg(Color::Yellow)),
            Span::raw(&status.peer_id),
        ]),
        Line::from(vec![
            Span::styled("Connected:  ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.peer_count)),
        ]),
        Line::from(""),
    ];

    if status.connected_peers.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No connected peers.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, peer) in status.connected_peers.iter().enumerate() {
            let style = if i == app.scroll_offset {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(format!("  {peer}"), style)));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Peer discovery via GossipSub / bootstrap peers / Kademlia DHT.",
        Style::default().fg(Color::DarkGray),
    )));

    app.list_len = status.connected_peers.len();
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
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
            let style = if entry.starts_with("[REJECT") || entry.starts_with("[STORE REJECT") {
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
            };
            ListItem::new(Line::from(Span::styled(entry.as_str(), style)))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_info(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Zode Info ");

    let Some(ref status) = app.status else {
        frame.render_widget(Paragraph::new("Loading...").block(block), area);
        return;
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Peer ID:        ", Style::default().fg(Color::Yellow)),
            Span::raw(&status.peer_id),
        ]),
        Line::from(vec![
            Span::styled("DB Size:        ", Style::default().fg(Color::Yellow)),
            Span::raw(format_bytes(status.storage.db_size_bytes)),
        ]),
        Line::from(vec![
            Span::styled("Block Count:    ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.storage.block_count)),
        ]),
        Line::from(vec![
            Span::styled("Head Count:     ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.storage.head_count)),
        ]),
        Line::from(vec![
            Span::styled("Program Count:  ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", status.storage.program_count)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "── Subscribed Topics ──",
            Style::default().fg(Color::Cyan),
        )),
    ];

    let mut all_lines = lines;
    for topic in &status.topics {
        all_lines.push(Line::from(format!("  {topic}")));
    }
    all_lines.push(Line::from(""));
    all_lines.push(Line::from(Span::styled(
        "── Version ──",
        Style::default().fg(Color::Cyan),
    )));
    all_lines.push(Line::from(format!(
        "  zfs-zode-cli v{}",
        env!("CARGO_PKG_VERSION")
    )));

    app.list_len = 0;
    let paragraph = Paragraph::new(all_lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
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
