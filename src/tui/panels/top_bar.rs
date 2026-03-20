use std::path::Path;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use crate::tui::theme;
use super::super::app::SearchState;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    search: &SearchState,
    zephyr_version: Option<&str>,
    zephyr_dir: Option<&Path>,
    selected_board: Option<&str>,
) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::BORDER_INACTIVE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build breadcrumb badge: Zephyr v3.x > workspace > board
    let sep_style = Style::default().fg(theme::COPPER);
    let sep = Span::styled(" > ", sep_style);

    let version_text = zephyr_version.unwrap_or("...");
    let mut badge_spans: Vec<Span> = vec![
        Span::styled(
            format!("Zephyr {version_text}"),
            Style::default().fg(theme::AMBER).add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(dir) = zephyr_dir {
        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir.display().to_string());
        badge_spans.push(sep.clone());
        badge_spans.push(Span::styled(dir_name, Style::default().fg(theme::TEXT_SECONDARY)));
    }

    if let Some(board) = selected_board {
        badge_spans.push(sep.clone());
        badge_spans.push(Span::styled(board.to_string(), Style::default().fg(theme::GOLD)));
    }

    let badge_line = Line::from(badge_spans);
    let badge_width: u16 = badge_line.width() as u16;

    // Size the right column to fit the badge, but cap it so search bar keeps at least 20 cols.
    let max_right = inner.width.saturating_sub(20);
    let right_width = badge_width.min(max_right);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(right_width)])
        .split(inner);

    // Search bar.
    let search_line = if search.active {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(theme::AMBER)),
            Span::styled(format!("{}_", search.query), Style::default().fg(theme::TEXT)),
        ])
    } else if search.query.is_empty() {
        Line::from(Span::styled("/ search...", theme::muted()))
    } else {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(theme::AMBER)),
            Span::styled(search.query.clone(), Style::default().fg(theme::TEXT)),
        ])
    };

    let search_widget = Paragraph::new(vec![search_line]);
    frame.render_widget(search_widget, chunks[0]);

    // Breadcrumb badge on the right.
    let version_widget = Paragraph::new(vec![badge_line]).alignment(Alignment::Right);
    frame.render_widget(version_widget, chunks[1]);
}
