#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod chat;
mod components;
mod helpers;
mod render;
mod settings;
mod state;
mod visualization;

use std::sync::Arc;

use eframe::egui;
use tokio::runtime::Runtime;

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
            .with_title("ZODE")
            .with_inner_size([820.0, 640.0])
            .with_icon(Arc::new(z_icon()))
            .with_decorations(false)
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "ZODE",
        options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            configure_theme(&cc.egui_ctx);
            Ok(Box::new(app::ZodeApp::new(rt)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;

    Ok(())
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/Inter-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

fn z_icon() -> egui::IconData {
    const S: u32 = 64;
    let mut rgba = vec![0u8; (S * S * 4) as usize];

    let set = |buf: &mut [u8], x: u32, y: u32, r: u8, g: u8, b: u8, a: u8| {
        let i = ((y * S + x) * 4) as usize;
        buf[i] = r;
        buf[i + 1] = g;
        buf[i + 2] = b;
        buf[i + 3] = a;
    };

    let radius = 12.0f32;
    for y in 0..S {
        for x in 0..S {
            let (fx, fy, s) = (x as f32, y as f32, S as f32);
            let inside = if fx < radius && fy < radius {
                (radius - fx).powi(2) + (radius - fy).powi(2) <= radius * radius
            } else if fx > s - radius && fy < radius {
                (fx - (s - radius)).powi(2) + (radius - fy).powi(2) <= radius * radius
            } else if fx < radius && fy > s - radius {
                (radius - fx).powi(2) + (fy - (s - radius)).powi(2) <= radius * radius
            } else if fx > s - radius && fy > s - radius {
                (fx - (s - radius)).powi(2) + (fy - (s - radius)).powi(2) <= radius * radius
            } else {
                true
            };
            if inside {
                set(&mut rgba, x, y, 38, 38, 42, 255);
            }
        }
    }

    let pad = 14u32;
    let thick = 7u32;
    let (r, g, b) = (255, 255, 255);

    for y in pad..pad + thick {
        for x in pad..S - pad {
            set(&mut rgba, x, y, r, g, b, 255);
        }
    }
    for y in S - pad - thick..S - pad {
        for x in pad..S - pad {
            set(&mut rgba, x, y, r, g, b, 255);
        }
    }

    let diag_top = pad + thick;
    let diag_bot = S - pad - thick;
    let diag_h = diag_bot - diag_top;
    let diag_w = (S - 2 * pad) as f32;
    for row in 0..diag_h {
        let y = diag_top + row;
        let progress = row as f32 / (diag_h - 1) as f32;
        let cx = (S - pad - 1) as f32 - progress * (diag_w - 1.0);
        let half = thick as f32 / 2.0;
        let x_start = (cx - half).max(pad as f32) as u32;
        let x_end = ((cx + half) as u32).min(S - pad);
        for x in x_start..x_end {
            set(&mut rgba, x, y, r, g, b, 255);
        }
    }

    egui::IconData {
        rgba,
        width: S,
        height: S,
    }
}

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::BLACK;
    visuals.window_fill = egui::Color32::from_rgb(28, 28, 30);
    visuals.extreme_bg_color = egui::Color32::from_rgb(20, 20, 22);
    visuals.faint_bg_color = egui::Color32::from_rgb(28, 28, 30);
    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 28, 30);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 38, 42);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(50, 50, 55);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.expansion = 0.0;
    visuals.widgets.active.bg_fill = egui::Color32::BLACK;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.expansion = 0.0;
    visuals.selection.bg_fill = egui::Color32::WHITE;
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::BLACK);
    ctx.set_visuals(visuals);

    ctx.style_mut(|s| {
        s.spacing.interact_size.y = 18.0;
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(11.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(11.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(9.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(10.0));
    });
}
