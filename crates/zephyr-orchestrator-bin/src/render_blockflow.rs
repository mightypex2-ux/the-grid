use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::components::tokens::{colors, font_size};
use crate::components::{overlay_frame, section_heading};
use crate::state::{AppState, BlockStatus};

const BAR_HEIGHT: f32 = 10.0;
const BAR_MIN_WIDTH: f32 = 80.0;
const BAR_MAX_WIDTH: f32 = 500.0;
const BAR_WIDTH_PER_TX: f32 = 3.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_TOP_MARGIN: f32 = 36.0;
const GLOW_OUTER_EXPAND: f32 = 6.0;
const GLOW_INNER_EXPAND: f32 = 2.0;
const GLOW_OUTER_ALPHA: f32 = 0.08;
const GLOW_INNER_ALPHA: f32 = 0.20;
const CORE_ALPHA: f32 = 0.70;
const DOT_RADIUS: f32 = 1.5;
const DOT_SPACING: f32 = 4.0;
const LABEL_WIDTH: f32 = 72.0;
const SCROLL_SPEED: f32 = 60.0;
const MAX_BLOCKS_PER_ZONE: usize = 200;
const ENTRANCE_DURATION_SECS: f32 = 0.1;

const PROPOSED_THRESHOLD_MS: u128 = 300;
const VOTING_THRESHOLD_MS: u128 = 600;

pub(crate) struct BlockflowVisualization {
    blocks: Vec<FlowBlock>,
    seen: HashSet<(u32, u64)>,
    camera: Camera,
}

struct FlowBlock {
    zone_id: u32,
    height: u64,
    tx_count: usize,
    birth: Instant,
    #[allow(dead_code)]
    block_hash_hex: String,
}

struct Camera {
    offset: egui::Vec2,
    zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: egui::Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Default for BlockflowVisualization {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            seen: HashSet::new(),
            camera: Camera::default(),
        }
    }
}

impl BlockflowVisualization {
    pub fn ingest(&mut self, state: &AppState) {
        let now = Instant::now();
        let first_load = self.seen.is_empty() && !state.recent_blocks.is_empty();

        let mut new_blocks = Vec::new();
        for block in state.recent_blocks.iter() {
            let key = (block.zone_id, block.height);
            if self.seen.insert(key) {
                new_blocks.push(block);
            }
        }

        if new_blocks.is_empty() {
            return;
        }

        if first_load {
            let mut by_zone: HashMap<u32, Vec<_>> = HashMap::new();
            for b in &new_blocks {
                by_zone.entry(b.zone_id).or_default().push(*b);
            }
            for (_, zone_blocks) in &mut by_zone {
                zone_blocks.sort_by(|a, b| b.height.cmp(&a.height));
                for (i, block) in zone_blocks.iter().enumerate() {
                    let stagger =
                        Duration::from_secs_f32(i as f32 * (BAR_MIN_WIDTH + 14.0) / SCROLL_SPEED);
                    self.blocks.push(FlowBlock {
                        zone_id: block.zone_id,
                        height: block.height,
                        tx_count: block.tx_count,
                        birth: now.checked_sub(stagger).unwrap_or(now),
                        block_hash_hex: block.block_hash_hex.clone(),
                    });
                }
            }
        } else {
            for block in &new_blocks {
                self.blocks.push(FlowBlock {
                    zone_id: block.zone_id,
                    height: block.height,
                    tx_count: block.tx_count,
                    birth: now,
                    block_hash_hex: block.block_hash_hex.clone(),
                });
            }
        }

        self.enforce_limits();
    }

    fn enforce_limits(&mut self) {
        let mut keep_counts = HashMap::<u32, usize>::new();
        self.blocks.retain(|b| {
            let count = keep_counts.entry(b.zone_id).or_default();
            if *count < MAX_BLOCKS_PER_ZONE {
                *count += 1;
                true
            } else {
                false
            }
        });
    }

    fn cull(&mut self, max_x: f32) {
        let now = Instant::now();
        self.blocks.retain(|b| {
            let age = now.duration_since(b.birth).as_secs_f32();
            let x = age * SCROLL_SPEED;
            x < max_x + BAR_MAX_WIDTH * 2.0
        });
    }

    pub fn render(&mut self, ui: &mut egui::Ui, state: &AppState) {
        self.ingest(state);

        let avail = ui.available_size();
        let (outer_rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());

        let mut child_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(outer_rect)
                .layout(egui::Layout::top_down(egui::Align::LEFT)),
        );
        let ui = &mut child_ui;

        let (resp, painter) = ui.allocate_painter(outer_rect.size(), egui::Sense::click_and_drag());
        let rect = resp.rect;

        self.handle_pan_zoom(&resp, ui);
        self.cull(rect.width() / self.camera.zoom + 200.0);

        painter.rect_filled(rect, 0.0, colors::PANEL_BG);

        let now = Instant::now();

        let mut zones: Vec<u32> = self.blocks.iter().map(|b| b.zone_id).collect();
        zones.sort_unstable();
        zones.dedup();

        let zone_count = zones.len();
        let total_blocks = self.blocks.len();
        let total_tps = state.network.actual_tps;

        for (row_idx, &zone_id) in zones.iter().enumerate() {
            let scaled_bar_h = BAR_HEIGHT * self.camera.zoom;
            let row_y = rect.top()
                + ROW_TOP_MARGIN
                + row_idx as f32 * ROW_HEIGHT * self.camera.zoom
                + self.camera.offset.y * self.camera.zoom;

            if row_y + scaled_bar_h < rect.top() || row_y > rect.bottom() {
                continue;
            }

            let label_x = rect.left() + 8.0;
            painter.text(
                egui::pos2(label_x, row_y + scaled_bar_h * 0.5),
                egui::Align2::LEFT_CENTER,
                format!("ZONE {zone_id}  \u{25B8}"),
                egui::FontId::proportional(11.0 * self.camera.zoom.sqrt()),
                egui::Color32::WHITE,
            );

            let zone_blocks: Vec<&FlowBlock> = self
                .blocks
                .iter()
                .filter(|b| b.zone_id == zone_id)
                .collect();

            let mut prev_screen_right: Option<f32> = None;

            for block in &zone_blocks {
                let age = now.duration_since(block.birth).as_secs_f32();
                let x_offset = age * SCROLL_SPEED;
                let bar_w = (BAR_MIN_WIDTH + block.tx_count as f32 * BAR_WIDTH_PER_TX)
                    .min(BAR_MAX_WIDTH);
                let scaled_w = bar_w * self.camera.zoom;

                let screen_x = rect.left()
                    + LABEL_WIDTH
                    + (x_offset + self.camera.offset.x) * self.camera.zoom;

                if screen_x + scaled_w < rect.left() || screen_x > rect.right() {
                    prev_screen_right = Some(screen_x + scaled_w);
                    continue;
                }

                let entrance_t = (age / ENTRANCE_DURATION_SECS).min(1.0);
                let alpha_mul = entrance_t;

                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(screen_x, row_y),
                    egui::vec2(scaled_w, scaled_bar_h),
                );

                if let Some(prev_right) = prev_screen_right {
                    let conn_y = row_y + scaled_bar_h * 0.5;
                    let connector_alpha = (25.0 * alpha_mul) as u8;
                    painter.line_segment(
                        [
                            egui::pos2(prev_right, conn_y),
                            egui::pos2(screen_x, conn_y),
                        ],
                        egui::Stroke::new(
                            0.75,
                            with_alpha(colors::NEON_CONNECTOR, connector_alpha),
                        ),
                    );
                }

                let status = block_status(block, now);
                let status_color = status_to_color(status);

                let outer_rect = bar_rect.expand2(egui::vec2(
                    4.0 * self.camera.zoom,
                    GLOW_OUTER_EXPAND * self.camera.zoom,
                ));
                painter.rect_filled(
                    outer_rect,
                    5.0,
                    with_alpha(
                        status_color,
                        (GLOW_OUTER_ALPHA * 255.0 * alpha_mul) as u8,
                    ),
                );

                let inner_rect = bar_rect.expand2(egui::vec2(
                    GLOW_INNER_EXPAND * self.camera.zoom,
                    GLOW_INNER_EXPAND * self.camera.zoom,
                ));
                painter.rect_filled(
                    inner_rect,
                    3.0,
                    with_alpha(
                        status_color,
                        (GLOW_INNER_ALPHA * 255.0 * alpha_mul) as u8,
                    ),
                );

                painter.rect_filled(
                    bar_rect,
                    2.0,
                    with_alpha(status_color, (CORE_ALPHA * 255.0 * alpha_mul) as u8),
                );

                if self.camera.zoom > 0.6 {
                    draw_tx_dots(&painter, bar_rect, block.tx_count, alpha_mul);
                }

                if scaled_w > 60.0 {
                    let label_pos = egui::pos2(
                        bar_rect.right() + 4.0,
                        bar_rect.center().y,
                    );
                    painter.text(
                        label_pos,
                        egui::Align2::LEFT_CENTER,
                        format!("#{}", block.height),
                        egui::FontId::proportional(
                            (font_size::TINY * self.camera.zoom.sqrt()).max(7.0),
                        ),
                        with_alpha(egui::Color32::WHITE, (180.0 * alpha_mul) as u8),
                    );
                }

                prev_screen_right = Some(screen_x + scaled_w);
            }
        }

        let overlay_pos = rect.left_top() + egui::vec2(12.0, 8.0);
        let overlay_w = rect.width() - 24.0;
        egui::Area::new(egui::Id::new("blockflow_overlay"))
            .fixed_pos(overlay_pos)
            .interactable(true)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                ui.set_width(overlay_w);
                ui.horizontal(|ui| {
                    overlay_frame().show(ui, |ui| {
                        section_heading(
                            ui,
                            &format!(
                                "BLOCKFLOW  \u{2022}  {zone_count} zones  \u{2022}  {total_blocks} blocks  \u{2022}  {total_tps:.1} tps"
                            ),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        overlay_frame()
                            .inner_margin(egui::Margin::symmetric(6, 4))
                            .show(ui, |ui| {
                                if crate::components::icon_button(
                                    ui,
                                    egui_phosphor::regular::ARROWS_IN,
                                )
                                .clicked()
                                {
                                    self.camera = Camera::default();
                                }
                            });
                    });
                });
            });

        ui.ctx().request_repaint();
    }

    fn handle_pan_zoom(&mut self, resp: &egui::Response, ui: &egui::Ui) {
        if resp.dragged() {
            self.camera.offset += resp.drag_delta() / self.camera.zoom;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let factor = 1.0 + scroll * 0.003;
                self.camera.zoom = (self.camera.zoom * factor).clamp(0.3, 4.0);
            }
        }
    }
}

fn block_status(block: &FlowBlock, now: Instant) -> BlockStatus {
    let age_ms = now.duration_since(block.birth).as_millis();
    if age_ms < PROPOSED_THRESHOLD_MS {
        BlockStatus::Proposed
    } else if age_ms < VOTING_THRESHOLD_MS {
        BlockStatus::Voting
    } else {
        BlockStatus::Certified
    }
}

fn status_to_color(status: BlockStatus) -> egui::Color32 {
    match status {
        BlockStatus::Proposed => colors::NEON_CYAN,
        BlockStatus::Voting => colors::NEON_AMBER,
        BlockStatus::Certified => colors::NEON_GREEN,
    }
}

fn with_alpha(c: egui::Color32, a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn draw_tx_dots(painter: &egui::Painter, bar_rect: egui::Rect, tx_count: usize, alpha: f32) {
    if tx_count == 0 {
        return;
    }

    let inner = bar_rect.shrink2(egui::vec2(DOT_SPACING, 0.0));
    let max_dots = ((inner.width() + DOT_SPACING) / (DOT_RADIUS * 2.0 + DOT_SPACING)).floor() as usize;
    let draw_count = tx_count.min(max_dots);
    let dot_color = with_alpha(egui::Color32::WHITE, (0.60 * 255.0 * alpha) as u8);
    let center_y = bar_rect.center().y;

    for i in 0..draw_count {
        let cx = inner.left() + DOT_RADIUS + i as f32 * (DOT_RADIUS * 2.0 + DOT_SPACING);
        if cx + DOT_RADIUS > inner.right() {
            break;
        }
        painter.circle_filled(egui::pos2(cx, center_y), DOT_RADIUS, dot_color);
    }
}
