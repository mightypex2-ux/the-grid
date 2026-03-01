use eframe::egui;

use super::tokens::{colors, spacing};

/// ALL-CAPS bold field label (e.g. grid keys).
pub(crate) fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text.to_uppercase()).strong());
}

/// Subdued descriptive text shown beneath section headings.
pub(crate) fn hint_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).weak());
}

/// Placeholder / empty-state text (e.g. "No items.", "resolving...").
pub(crate) fn muted_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).weak());
}

/// Red error message with spacing below.
pub(crate) fn error_label(ui: &mut egui::Ui, text: &str) {
    ui.colored_label(colors::ERROR, text);
    ui.add_space(spacing::SM);
}
