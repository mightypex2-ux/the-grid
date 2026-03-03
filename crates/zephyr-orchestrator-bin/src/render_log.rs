use eframe::egui;

use crate::components::section;
use crate::components::tokens::{colors, font_size, spacing};
use crate::helpers::node_color;
use crate::state::{AppState, LogLevel};

pub(crate) fn render_log(ui: &mut egui::Ui, state: &AppState, level_filter: &mut Option<LogLevel>) {
    egui::TopBottomPanel::top("log_filter_bar")
        .frame(
            egui::Frame::default()
                .fill(colors::SURFACE_DARK)
                .inner_margin(egui::Margin::symmetric(12, 6)),
        )
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Filter:")
                        .size(font_size::ACTION)
                        .color(colors::TEXT_HEADING),
                );
                ui.add_space(spacing::SM);

                let all_active = level_filter.is_none();
                if ui.selectable_label(all_active, "ALL").clicked() {
                    *level_filter = None;
                }
                for lvl in &[
                    LogLevel::Info,
                    LogLevel::Warn,
                    LogLevel::Error,
                    LogLevel::Debug,
                ] {
                    let active = *level_filter == Some(*lvl);
                    let color = log_level_color(*lvl);
                    let resp =
                        ui.selectable_label(active, egui::RichText::new(lvl.label()).color(color));
                    if resp.clicked() {
                        *level_filter = Some(*lvl);
                    }
                }
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::default()
                .fill(colors::PANEL_BG)
                .inner_margin(0.0),
        )
        .show_inside(ui, |ui| {
            section(ui, "Aggregated Log", |ui| {
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

                let active_filter: Option<LogLevel> = *level_filter;
                let total_rows = match active_filter {
                    Some(f) => state.log_entries.iter().filter(|e| e.level == f).count(),
                    None => state.log_entries.len(),
                };

                egui::ScrollArea::vertical()
                    .id_salt("log_scroll")
                    .auto_shrink([false; 2])
                    .stick_to_bottom(true)
                    .show_rows(ui, row_height, total_rows, |ui, visible| {
                        if total_rows == 0 {
                            ui.label(
                                egui::RichText::new("Waiting for log events...")
                                    .color(colors::TEXT_MUTED),
                            );
                            return;
                        }

                        match active_filter {
                            None => {
                                for i in visible {
                                    if let Some(entry) = state.log_entries.get(i) {
                                        render_log_row(ui, entry);
                                    }
                                }
                            }
                            Some(f) => {
                                let mut logical = 0usize;
                                for entry in &state.log_entries {
                                    if entry.level != f {
                                        continue;
                                    }
                                    if logical >= visible.end {
                                        break;
                                    }
                                    if logical >= visible.start {
                                        render_log_row(ui, entry);
                                    }
                                    logical += 1;
                                }
                            }
                        }
                    });
            });
        });
}

fn render_log_row(ui: &mut egui::Ui, entry: &crate::state::AggregatedLogEntry) {
    let color = node_color(entry.node_id);
    let level_col = log_level_color(entry.level);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("[Node {}]", entry.node_id))
                .monospace()
                .size(font_size::BODY)
                .color(color),
        );
        ui.label(
            egui::RichText::new(entry.level.label())
                .monospace()
                .size(font_size::BODY)
                .color(level_col),
        );
        ui.label(
            egui::RichText::new(&entry.line)
                .monospace()
                .size(font_size::BODY)
                .color(colors::LOG_NORMAL),
        );
    });
}

fn log_level_color(level: LogLevel) -> egui::Color32 {
    match level {
        LogLevel::Info => colors::LOG_GOSSIP,
        LogLevel::Warn => colors::LOG_PEER_DISCONNECT,
        LogLevel::Error => colors::LOG_REJECT,
        LogLevel::Debug => colors::LOG_NORMAL,
    }
}
