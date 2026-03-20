//! Centralized color palette and style helpers for the TUI.
//!
//! Every color used in rendering lives here as a constant.
//! Panels import from this module — no inline `Color::` literals elsewhere.

use ratatui::prelude::*;
use ratatui::widgets::{BorderType, Borders};

// ---------------------------------------------------------------------------
// 256-color palette constants
// ---------------------------------------------------------------------------

/// Bright gold — primary accent (active borders, cursor bg, active tabs).
pub const GOLD: Color = Color::Indexed(220);

/// Amber — secondary accent (keybind keys, step titles, input labels).
pub const AMBER: Color = Color::Indexed(214);

/// Copper — tertiary accent (paths, includes, directories, info tags).
pub const COPPER: Color = Color::Indexed(173);

/// Warm olive green — success / okay status.
pub const SUCCESS: Color = Color::Indexed(107);

/// Warm red — error / danger / disabled status.
pub const ERROR: Color = Color::Indexed(167);

/// Deep amber — search match background.
pub const SEARCH_MATCH: Color = Color::Indexed(208);

/// White — primary text.
pub const TEXT: Color = Color::White;

/// Light warm gray — secondary text (property values, descriptions).
pub const TEXT_SECONDARY: Color = Color::Indexed(250);

/// Medium gray — muted text (hints, separators, type annotations).
pub const TEXT_MUTED: Color = Color::Indexed(245);

/// Dim gray — disabled text, inactive placeholders.
pub const TEXT_DIM: Color = Color::Indexed(240);

/// Dark warm gray — inactive panel borders.
pub const BORDER_INACTIVE: Color = Color::Indexed(238);

/// Dark gray — selection background, subtle highlights.
pub const SELECTION_BG: Color = Color::Indexed(236);

/// True black — cursor foreground, panel backgrounds.
pub const CURSOR_FG: Color = Color::Indexed(16);

/// Near-black — subtle background tint.
pub const SURFACE: Color = Color::Indexed(233);

/// Warm cream — selection text.
pub const SELECTION_FG: Color = Color::Indexed(223);

// ---------------------------------------------------------------------------
// Border helpers
// ---------------------------------------------------------------------------

/// Border type for the active (focused) panel.
pub const ACTIVE_BORDER_TYPE: BorderType = BorderType::Double;

/// Border type for inactive panels.
pub const INACTIVE_BORDER_TYPE: BorderType = BorderType::Rounded;

/// Build a `Block` with the correct border style for the given active state.
pub fn panel_block(title: &str, is_active: bool) -> ratatui::widgets::Block<'_> {
    if is_active {
        ratatui::widgets::Block::default()
            .title(title)
            .title_style(Style::default().fg(GOLD).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_type(ACTIVE_BORDER_TYPE)
            .border_style(Style::default().fg(GOLD))
    } else {
        ratatui::widgets::Block::default()
            .title(title)
            .title_style(Style::default().fg(TEXT_MUTED))
            .borders(Borders::ALL)
            .border_type(INACTIVE_BORDER_TYPE)
            .border_style(Style::default().fg(BORDER_INACTIVE))
    }
}

// ---------------------------------------------------------------------------
// Common style helpers
// ---------------------------------------------------------------------------

/// Cursor line: dark amber background + gold foreground.
pub fn cursor_style() -> Style {
    Style::default().fg(GOLD).bg(Color::Indexed(94))
}

/// Visual selection: dark bg + warm cream foreground.
pub fn selection_style() -> Style {
    Style::default().fg(SELECTION_FG).bg(SELECTION_BG)
}

/// Search match highlight.
pub fn search_match_style() -> Style {
    Style::default().fg(TEXT).bg(SEARCH_MATCH)
}

/// Search match that is also the cursor line — bold to distinguish.
pub fn search_match_cursor_style() -> Style {
    Style::default()
        .fg(TEXT)
        .bg(SEARCH_MATCH)
        .add_modifier(Modifier::BOLD)
}

/// Keybind key text (e.g. "Enter", "j/k").
pub fn keybind_key() -> Style {
    Style::default().fg(AMBER)
}

/// Keybind description text.
pub fn keybind_desc() -> Style {
    Style::default().fg(TEXT_MUTED)
}

/// Section header (e.g. "Global", "Center Panel").
pub fn section_header() -> Style {
    Style::default().fg(AMBER).add_modifier(Modifier::BOLD)
}

/// Step title (e.g. "Step 1: Select Board").
pub fn step_title() -> Style {
    Style::default().fg(AMBER).add_modifier(Modifier::BOLD)
}

/// Input field label.
pub fn input_label() -> Style {
    Style::default().fg(AMBER)
}

/// Active input text.
pub fn input_field() -> Style {
    Style::default()
        .fg(TEXT)
        .add_modifier(Modifier::UNDERLINED)
}

/// Status / hint text in the bottom bar.
pub fn status_hint() -> Style {
    Style::default().fg(TEXT_DIM)
}

/// Success message.
pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

/// Error / warning message.
pub fn error() -> Style {
    Style::default().fg(ERROR)
}

/// Muted placeholder / empty text.
pub fn muted() -> Style {
    Style::default().fg(TEXT_DIM)
}

/// Label text (e.g. "Board:", "Dir:").
pub fn label() -> Style {
    Style::default().fg(TEXT_MUTED)
}

/// Active line number in the gutter.
pub fn lineno_active() -> Style {
    Style::default().fg(GOLD)
}

/// Inactive line number in the gutter.
pub fn lineno_inactive() -> Style {
    Style::default().fg(TEXT_DIM)
}

// ---------------------------------------------------------------------------
// Scrollbar helpers
// ---------------------------------------------------------------------------

/// Light shade for the scrollbar track.
pub const SCROLLBAR_TRACK: &str = "\u{2591}";
/// Full block for the scrollbar thumb.
pub const SCROLLBAR_THUMB: &str = "\u{2588}";

/// Track color.
pub const SCROLLBAR_TRACK_COLOR: Color = Color::Indexed(236);
/// Thumb color.
pub const SCROLLBAR_THUMB_COLOR: Color = Color::Indexed(245);

// ---------------------------------------------------------------------------
// DTS syntax token colors (raw mode)
// ---------------------------------------------------------------------------

/// DTS keyword (`/dts-v1/`, `/plugin/`).
pub fn dts_keyword() -> Style {
    Style::default().fg(AMBER).add_modifier(Modifier::BOLD)
}

/// DTS `&reference` label.
pub fn dts_reference() -> Style {
    Style::default().fg(GOLD)
}

/// DTS `#include` keyword.
pub fn dts_include_keyword() -> Style {
    Style::default().fg(COPPER)
}

/// DTS string values (`"..."`) and angle-bracket values (`<...>`).
pub fn dts_string() -> Style {
    Style::default().fg(TEXT_SECONDARY)
}

/// DTS comments (`/* ... */`, `//`).
pub fn dts_comment() -> Style {
    Style::default()
        .fg(TEXT_MUTED)
        .add_modifier(Modifier::ITALIC)
}

/// DTS node names (before `{`).
pub fn dts_node_name() -> Style {
    Style::default().fg(GOLD)
}

/// DTS property names (before `=` or `;`).
pub fn dts_property_name() -> Style {
    Style::default().fg(TEXT)
}

// ---------------------------------------------------------------------------
// Step progress indicator
// ---------------------------------------------------------------------------

/// Render a 3-step progress line: [1 Select] -- [2 Edit] -- [3 Save]
pub fn step_progress_line(current: usize, completed: usize) -> Line<'static> {
    let steps = ["Select", "Edit", "Save"];
    let mut spans: Vec<Span> = Vec::new();

    for (i, name) in steps.iter().enumerate() {
        let step_num = i + 1;
        let text = format!(" {step_num} {name} ");
        let style = if i == current {
            Style::default().fg(CURSOR_FG).bg(GOLD).add_modifier(Modifier::BOLD)
        } else if i < completed {
            Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_DIM)
        };
        spans.push(Span::styled(text, style));

        if i < steps.len() - 1 {
            spans.push(Span::styled(" --- ", Style::default().fg(BORDER_INACTIVE)));
        }
    }

    Line::from(spans)
}
