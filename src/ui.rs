use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Panel};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

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
            } else if branch.is_head {
                Style::default().fg(Color::Green)
            } else if branch.is_remote {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default()
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
                Span::styled(format!("{:<8}", graph), style.fg(Color::Magenta)),
                Span::styled(format!("{:<12}", commit.date), style.fg(Color::DarkGray)),
                Span::styled(format!("{:<14}", truncate(&commit.author, 14)), style),
                Span::styled(commit.short_id.clone(), style.fg(Color::Cyan)),
                Span::raw(" "),
                Span::styled(commit.message.clone(), style),
                Span::styled(labels, style.fg(Color::Blue)),
            ]))
        })
        .collect();

    let block = Block::default()
        .title(" Commit History ")
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn draw_files(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.panel() == Panel::Files;
    let files = app.selected_files();

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let prefix = if entry.is_dir { "[D] " } else { "    " };
            let style = if i == app.file_index() && active {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow)
            } else if entry.is_dir {
                Style::default().fg(Color::Blue)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::raw(prefix),
                Span::styled(entry.display.clone(), style),
            ]))
        })
        .collect();

    let title = app
        .selected_commit()
        .map(|c| format!(" Changed Files @ {} ", c.short_id))
        .unwrap_or_else(|| " Changed Files ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = if items.is_empty() {
        List::new(vec![ListItem::new("(select a commit)")]).block(block)
    } else {
        List::new(items).block(block)
    };

    frame.render_widget(list, area);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let status = Paragraph::new(app.status_line()).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, area);
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}…", &value[..max.saturating_sub(1)])
    }
}
