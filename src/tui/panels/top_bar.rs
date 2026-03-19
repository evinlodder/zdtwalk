use std::path::Path;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use super::super::app::SearchState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    search: &SearchState,
    zephyr_version: Option<&str>,
    zephyr_dir: Option<&Path>,
) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build the version+path badge text.
    let version_text = zephyr_version.unwrap_or("...");
    let path_text = zephyr_dir
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let full_badge = if path_text.is_empty() {
        format!("Zephyr {version_text}")
    } else {
        format!("Zephyr {version_text} | {path_text}")
    };

    // Size the right column to fit the badge, but cap it so search bar keeps at least 20 cols.
    let max_right = inner.width.saturating_sub(20);
    let badge_len = full_badge.len() as u16;
    let right_width = badge_len.min(max_right);

    // Truncate badge with "..." if it doesn't fit.
    let badge = if badge_len > right_width {
        let avail = right_width.saturating_sub(3) as usize;
        format!("{}...", &full_badge[..avail])
    } else {
        full_badge
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(right_width)])
        .split(inner);

    // Search bar.
    let search_text = if search.active {
        format!("/ {}_", search.query)
    } else if search.query.is_empty() {
        "Press / to search".to_string()
    } else {
        format!("/ {}", search.query)
    };

    let search_style = if search.active {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let search_widget = Paragraph::new(search_text).style(search_style);
    frame.render_widget(search_widget, chunks[0]);

    // Version and zephyr path badge.
    let version_widget = Paragraph::new(badge)
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Right);
    frame.render_widget(version_widget, chunks[1]);
}
