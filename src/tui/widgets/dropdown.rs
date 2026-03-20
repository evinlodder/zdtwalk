use ratatui::prelude::*;

use crate::tui::theme;
use super::status_dot::StatusColor;

/// Render a collapsible dropdown row.
///
/// Returns a `Line` with: indent + arrow + optional status dot + label.
#[allow(dead_code)]
pub fn dropdown_line<'a>(
    label: &str,
    expanded: bool,
    depth: usize,
    status: Option<StatusColor>,
) -> Line<'a> {
    let indent = "  ".repeat(depth);
    let arrow = if expanded { "▼ " } else { "▶ " };

    let mut spans = vec![
        Span::raw(indent),
        Span::styled(arrow.to_string(), Style::default().fg(theme::TEXT_MUTED)),
    ];

    if let Some(color) = status {
        spans.push(color.dot_span());
        spans.push(Span::raw(" "));
    }

    spans.push(Span::raw(label.to_string()));

    Line::from(spans)
}
