use ratatui::prelude::*;
use ratatui::widgets::Clear;

use super::app::{App, Panel};
use super::panels::top_bar;

/// Master render function — draws all panels each frame.
pub fn render(frame: &mut Frame, app: &mut App) {
    let size = frame.area();

    // Vertical: top bar (2 rows) | main area [| debug panel].
    let main_constraints = if app.debug.visible {
        vec![
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Percentage(30),
        ]
    } else {
        vec![Constraint::Length(2), Constraint::Min(0)]
    };
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_constraints)
        .split(size);

    // Top bar.
    let zephyr_version = app
        .workspace
        .as_ref()
        .map(|ws| ws.zephyr_version.as_str());
    let zephyr_dir = app
        .workspace
        .as_ref()
        .map(|ws| ws.info.zephyr_dir.as_path());
    top_bar::render(frame, v_chunks[0], &app.search, zephyr_version, zephyr_dir);

    // Main area — horizontal split.
    let main_area = v_chunks[1];

    if app.right.collapsed {
        // Two-panel layout: left | center.
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(app.left_width_pct), Constraint::Min(0)])
            .split(main_area);

        app.left
            .render(frame, h_chunks[0], app.active_panel == Panel::Left);
        app.center
            .render(frame, h_chunks[1], app.active_panel == Panel::Center);
    } else {
        // Three-panel layout: left | center | right.
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(app.left_width_pct),
                Constraint::Min(0),
                Constraint::Percentage(22),
            ])
            .split(main_area);

        app.left
            .render(frame, h_chunks[0], app.active_panel == Panel::Left);
        app.center
            .render(frame, h_chunks[1], app.active_panel == Panel::Center);
        app.right
            .render(frame, h_chunks[2], app.active_panel == Panel::Right);
    }

    // Debug panel.
    if app.debug.visible {
        let is_active = app.active_panel == Panel::Debug;
        app.debug.render(frame, v_chunks[2], is_active);
    }

    // Status message overlay at the bottom.
    if let Some(msg) = &app.status_message {
        let status_area = Rect {
            x: 0,
            y: size.height.saturating_sub(1),
            width: size.width,
            height: 1,
        };
        let status = ratatui::widgets::Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Yellow).bg(Color::Black));
        frame.render_widget(status, status_area);
    } else {
        // Show subtle hint when no status message.
        let hint_area = Rect {
            x: 0,
            y: size.height.saturating_sub(1),
            width: size.width,
            height: 1,
        };
        let hint = ratatui::widgets::Paragraph::new(" ? help")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, hint_area);
    }

    // Help overlay.
    if app.show_help {
        render_help(frame, size);
    }
}

fn render_help(frame: &mut Frame, size: Rect) {
    let help_lines = vec![
        ("Global", vec![
            ("Tab / Shift-Tab", "Switch panel focus"),
            ("?", "Toggle this help"),
            ("q", "Quit"),
            ("Ctrl-D", "Toggle debug log panel"),
            ("g", "Toggle right (generator) panel"),
            ("[ / ]", "Resize left panel"),
        ]),
        ("Left Panel (File Tree)", vec![
            ("j / k", "Move up / down"),
            ("/", "Search / filter files"),
            ("Enter", "Open selected file"),
            ("m", "Cycle mode (Board / Overlays / Bindings)"),
            ("1 / 2 / 3", "Switch mode directly"),
            ("b", "Toggle board picker (Board mode)"),
        ]),
        ("Center Panel (Viewer)", vec![
            ("j / k", "Scroll up / down"),
            ("v", "Toggle raw / tree view"),
            ("V", "Start visual line selection"),
            ("y", "Yank (copy) selection to clipboard"),
            ("/", "Search in file (fuzzy)"),
            ("n / N", "Next / prev search match"),
            ("Enter / Space", "Open include / toggle node"),
            ("h / l", "Collapse / expand node"),
            ("{ / }", "Prev / next open tab"),
            ("Ctrl-W", "Close current tab"),
            ("Esc", "Cancel selection / search"),
        ]),
        ("Debug Panel", vec![
            ("j / k", "Scroll up / down"),
            ("G", "Jump to bottom (follow)"),
            ("g", "Jump to top"),
        ]),
        ("Generator Panel", vec![
            ("→ / Enter", "Next step / expand node"),
            ("← / Esc", "Previous step / back"),
            ("j / k", "Navigate nodes / file browser"),
            ("a", "Add node from viewer (center panel)"),
            ("n", "New reference node / new file (save)"),
            ("c", "Add child node"),
            ("p", "Add property"),
            ("e", "Edit property"),
            ("d", "Delete selected node"),
            ("s", "Save overlay"),
        ]),
    ];

    // Calculate dimensions.
    let content_width: u16 = 60;
    let content_height: u16 = help_lines.iter()
        .map(|(_, items)| items.len() as u16 + 2) // header + blank + items
        .sum::<u16>() + 1; // +1 for trailing blank

    let popup_w = content_width + 4; // borders + padding
    let popup_h = (content_height + 2).min(size.height.saturating_sub(4)); // borders

    let x = size.width.saturating_sub(popup_w) / 2;
    let y = size.height.saturating_sub(popup_h) / 2;
    let area = Rect::new(x, y, popup_w, popup_h);

    // Clear background.
    frame.render_widget(Clear, area);

    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title(" Keybinds (press any key to close) ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for (section, items) in &help_lines {
        lines.push(Line::from(Span::styled(
            format!("  {section}"),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in items {
            lines.push(Line::from(vec![
                Span::styled(format!("    {key:<20}"), Style::default().fg(Color::Yellow)),
                Span::styled(*desc, Style::default().fg(Color::White)),
            ]));
        }
        lines.push(Line::raw(""));
    }

    let paragraph = ratatui::widgets::Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
