use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

mod app;
mod diff;
mod graph;
mod repo;
mod ui;

#[derive(Parser)]
#[command(name = "repov", about = "A gitk-style TUI for exploring git repositories", version)]
struct Cli {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    repo: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut app = app::App::open(&cli.repo)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
                {
                    break;
                }

                if app.search_input().is_some() {
                    match key.code {
                        KeyCode::Esc => app.cancel_search(),
                        KeyCode::Enter => app.apply_search(),
                        KeyCode::Backspace => app.search_backspace(),
                        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.clear_search();
                        }
                        KeyCode::Char(c) => app.search_push(c),
                        _ => {}
                    }
                    continue;
                }

                if app.show_help() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('?') => app.close_help(),
                        KeyCode::Char('q') => break,
                        _ => {}
                    }
                    continue;
                }

                if app.diff_is_open() {
                    match key.code {
                        KeyCode::Esc => app.close_diff(),
                        KeyCode::Char('j') | KeyCode::Down => app.scroll_diff_down(),
                        KeyCode::Char('k') | KeyCode::Up => app.scroll_diff_up(),
                        KeyCode::Char('q') => break,
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => app.reload()?,
                    KeyCode::Char('c') => app.toggle_files_mode(),
                    KeyCode::Char('b') => app.show_branch_line(),
                    KeyCode::Char('p') => app.toggle_first_parent(),
                    KeyCode::Char('y') => app.copy_sha(),
                    KeyCode::Char('?') => app.toggle_help(),
                    KeyCode::Char('/') | KeyCode::Char('f') => app.start_search(),
                    KeyCode::Char('g') => app.jump_top(),
                    KeyCode::Char('G') => app.jump_bottom(),
                    KeyCode::PageUp => app.page_up(),
                    KeyCode::PageDown => app.page_down(),
                    KeyCode::Tab => app.next_panel(),
                    KeyCode::BackTab => app.prev_panel(),
                    KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                    KeyCode::Enter => match app.panel() {
                        app::Panel::History => app.open_commit_diff()?,
                        app::Panel::Files => app.open_file_diff()?,
                        app::Panel::Refs => app.select_ref_on_enter(),
                    },
                    KeyCode::Esc => {
                        if !app.search_query().is_empty() {
                            app.clear_search();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
