mod app;
mod event;
pub mod log;
mod panels;
mod render;
mod widgets;
mod workspace;

use std::io;
use std::path::PathBuf;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;

use app::App;
use event::EventLoop;

use crate::dts;

/// Run the TUI application. This is the sole entry point for zdtwalk.
pub async fn run_tui(workspace_override: Option<PathBuf>) -> Result<(), dts::Error> {
    // Set up a panic hook that restores terminal before printing the panic.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = restore_terminal();
        original_hook(panic_info);
    }));

    setup_terminal()?;
    let result = run_app(workspace_override).await;
    restore_terminal()?;
    result
}

fn setup_terminal() -> Result<(), dts::Error> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    Ok(())
}

fn restore_terminal() -> Result<(), dts::Error> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

async fn run_app(workspace_override: Option<PathBuf>) -> Result<(), dts::Error> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Spawn the crossterm event loop.
    let (event_loop, mut rx) = EventLoop::new();
    let _event_task = tokio::spawn(event_loop.run());

    // Kick off workspace discovery in the background.
    let ws_tx = app.message_tx();
    let ws_override = workspace_override.clone();
    tokio::spawn(async move {
        let result = workspace::discover_workspace(ws_override).await;
        let msg = match result {
            Ok(state) => app::Message::WorkspaceReady(state),
            Err(e) => app::Message::Error(format!("Workspace discovery failed: {e}")),
        };
        let _ = ws_tx.send(msg).await;
    });

    loop {
        terminal.draw(|frame| render::render(frame, &mut app))?;

        // Wait for an event from either the crossterm event reader or internal messages.
        tokio::select! {
            Some(event_msg) = rx.recv() => {
                app.update(event_msg).await;
            }
            Some(internal_msg) = app.recv_message() => {
                app.update(internal_msg).await;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
