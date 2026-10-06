use anyhow::Result;
use clap::Parser;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use repov::app::App;
use repov::ui;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "repov-screenshot", about = "Render a PNG screenshot of the repov UI")]
struct Cli {
    /// Repository path
    #[arg(default_value = ".")]
    repo: String,

    /// Output PNG path
    #[arg(short, long, default_value = "docs/screenshot.png")]
    output: PathBuf,

    /// Terminal width (columns)
    #[arg(long, default_value_t = 120)]
    width: u16,

    /// Terminal height (rows)
    #[arg(long, default_value_t = 36)]
    height: u16,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut app = App::open(&cli.repo)?;

    let backend = TestBackend::new(cli.width, cli.height);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, &mut app))?;

    let buffer = terminal.backend().buffer().clone();
    repov::screenshot::render_png(&buffer, &cli.output)?;
    eprintln!("Wrote {}", cli.output.display());
    Ok(())
}
