use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, FilesMode, Panel};
use crate::diff::styled_line;
use crate::repo::ChangeStatus;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    if app.diff_is_open() {
        let overlay = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(chunks[0]);

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(18), Constraint::Percentage(82)])
            .split(overlay[0]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(main[1]);

        draw_refs(frame, app, main[0]);
        draw_history(frame, app, right[0]);
        draw_files(frame, app, right[1]);
        draw_diff(frame, app, overlay[1]);
    } else {
        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(18), Constraint::Percentage(82)])
            .split(chunks[0]);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(main[1]);

        draw_refs(frame, app, main[0]);
        draw_history(frame, app, right[0]);
        draw_files(frame, app, right[1]);
    }

    draw_status(frame, app, chunks[1]);
}

fn panel_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn draw_refs(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.panel() == Panel::Refs;
    let items: Vec<ListItem> = app
        .branches()
        .iter()
        .enumerate()
        .map(|(i, branch)| {
            let marker = if branch.is_head { "HEAD " } else { "     " };
            let style = if i == app.branch_index() && active {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow)
            } else if i == app.branch_index() {
                Style::default().fg(Color::Yellow)
            } else if branch.is_head {
                Style::default().fg(Color::Green)
            } else if branch.is_remote {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{} ", branch.short_id), style.fg(Color::Cyan)),
                Span::styled(branch.name.clone(), style),
            ]))
        })
        .collect();

    let block = Block::default()
        .title(" Refs ")
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_history(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.panel() == Panel::History;
    let branch_name = app
        .selected_branch()
        .map(|b| b.name.as_str())
        .unwrap_or("?");

    let items: Vec<ListItem> = app
        .commits()
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let graph = app.graph_line(i);
            let labels = if commit.branch_labels.is_empty() {
                String::new()
            } else {
                format!(" [{}]", commit.branch_labels.join(", "))
            };

            let style = if i == app.commit_index() && active {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<8}", graph), Style::default().fg(Color::Magenta)),
                Span::styled(format!("{:<12}", commit.date), style.fg(Color::DarkGray)),
                Span::styled(format!("{:<14}", truncate(&commit.author, 14)), style),
                Span::styled(commit.short_id.clone(), style.fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(commit.message.clone(), style.fg(Color::White)),
                Span::styled(labels, style.fg(Color::Blue)),
            ]))
        })
        .collect();

    let block = Block::default()
        .title(format!(" History — {branch_name} "))
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_files(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.panel() == Panel::Files;
    let files = app.selected_files();
    let mode_label = match app.files_mode() {
        FilesMode::All => "All Files",
        FilesMode::Changed => "Changed Files",
    };

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let (prefix, change_style) = match entry.change {
                Some(ChangeStatus::Added) => ("+ ", Style::default().fg(Color::Green)),
                Some(ChangeStatus::Modified) => ("M ", Style::default().fg(Color::Yellow)),
                Some(ChangeStatus::Deleted) => ("- ", Style::default().fg(Color::Red)),
                Some(ChangeStatus::Renamed) => ("R ", Style::default().fg(Color::Cyan)),
                None if entry.is_dir => ("D ", Style::default().fg(Color::Blue)),
                None => ("  ", Style::default()),
            };

            let mut style = if i == app.file_index() && active {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow)
            } else if entry.is_dir {
                Style::default().fg(Color::Blue)
            } else {
                file_style(&entry.path)
            };

            if entry.change.is_some() && !(i == app.file_index() && active) {
                style = change_style;
            }

            ListItem::new(Line::from(vec![
                Span::styled(prefix, change_style),
                Span::styled(entry.display.clone(), style),
            ]))
        })
        .collect();

    let title = app
        .selected_commit()
        .map(|c| format!(" {mode_label} @ {} ", c.short_id))
        .unwrap_or_else(|| format!(" {mode_label} "));

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = if items.is_empty() {
        List::new(vec![ListItem::new("(no files — try c to toggle all/changed)")]).block(block)
    } else {
        List::new(items).block(block)
    };

    frame.render_widget(list, area);
}

fn draw_diff(frame: &mut Frame, app: &App, area: Rect) {
    let Some(diff) = app.diff_view() else {
        return;
    };

    let lines: Vec<Line> = diff.lines.iter().map(styled_line).collect();
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(" {} ", diff.title))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false })
        .scroll((diff.scroll, 0));

    frame.render_widget(paragraph, area);
}

fn draw_status(frame: &mut Frame, app: &mut App, area: Rect) {
    let status = Paragraph::new(app.status_line()).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, area);
}

fn file_style(path: &str) -> Style {
    let ext = path.rsplit('.').next().unwrap_or("");
    let color = match ext {
        "rs" => Color::Rgb(255, 142, 80),
        "toml" | "lock" => Color::Green,
        "md" => Color::Blue,
        "json" | "yaml" | "yml" => Color::Yellow,
        "sh" | "bash" | "zsh" => Color::Cyan,
        "py" => Color::Rgb(55, 118, 171),
        "js" | "ts" | "tsx" | "jsx" => Color::Rgb(247, 223, 30),
        "html" | "css" => Color::Magenta,
        _ => Color::White,
    };
    Style::default().fg(color)
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}…", &value[..max.saturating_sub(1)])
    }
}
