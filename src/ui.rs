use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Wrap},
    Frame,
};

use crate::app::{App, FilesMode, Panel};
use crate::diff::styled_line;
use crate::repo::{ChangeStatus, RefKind};

const PAGE_SIZE: usize = 10;

fn scroll_for(selected: usize) -> usize {
    if selected >= PAGE_SIZE {
        selected.saturating_sub(PAGE_SIZE / 2)
    } else {
        0
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let bottom: Vec<Constraint> = if app.search_input().is_some() {
        vec![Constraint::Length(1), Constraint::Length(1)]
    } else {
        vec![Constraint::Length(1)]
    };

    let mut constraints = vec![Constraint::Min(1)];
    constraints.extend(bottom);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    let main_area = chunks[0];

    if app.diff_is_open() {
        let overlay = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(main_area);

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
            .split(main_area);

        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(62), Constraint::Percentage(38)])
            .split(main[1]);

        draw_refs(frame, app, main[0]);
        draw_history(frame, app, right[0]);
        draw_files(frame, app, right[1]);
    }

    if app.search_input().is_some() {
        draw_search_bar(frame, app, chunks[1]);
        draw_status(frame, app, chunks[2]);
    } else {
        draw_status(frame, app, chunks[1]);
    }

    if app.show_help() {
        draw_help(frame);
    }
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
        .refs()
        .iter()
        .enumerate()
        .map(|(i, ref_entry)| {
            let (marker, base_style) = match ref_entry.kind {
                RefKind::Branch { is_head, is_remote } => {
                    let marker = if is_head { "HEAD " } else { "     " };
                    let style = if is_remote {
                        Style::default().fg(Color::Magenta)
                    } else if is_head {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    (marker, style)
                }
                RefKind::Tag => (" tag ", Style::default().fg(Color::Yellow)),
            };

            let style = if i == app.ref_index() && active {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow)
            } else if i == app.ref_index() {
                Style::default().fg(Color::Yellow)
            } else {
                base_style
            };

            ListItem::new(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(format!("{} ", ref_entry.short_id), style.fg(Color::Cyan)),
                Span::styled(ref_entry.name.clone(), style),
            ]))
        })
        .collect();

    let block = Block::default()
        .title(" Refs ")
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = List::new(items).block(block);
    render_list(
        frame,
        area,
        list,
        app.ref_index(),
        scroll_for(app.ref_index()),
        !app.refs().is_empty(),
        app.panel() == Panel::Refs,
    );
}

fn draw_history(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.panel() == Panel::History;
    let ref_name = app.selected_ref().map(|r| r.name.as_str()).unwrap_or("?");
    let filter = app.search_query();
    let graph_mode = if app.first_parent() {
        "branch line"
    } else {
        "full graph"
    };
    let title = if filter.is_empty() {
        format!(" History — {ref_name} ({graph_mode}) ")
    } else {
        format!(" History — {ref_name} ({graph_mode}, filter: {filter}) ")
    };

    let graph_width = app
        .commits()
        .iter()
        .enumerate()
        .map(|(i, _)| app.graph_line(i).len())
        .max()
        .unwrap_or(4)
        .clamp(4, 16);

    let tip_oid = app.branch_tip_oid();

    let items: Vec<ListItem> = app
        .commits()
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let graph = app.graph_line(i);
            let is_tip = tip_oid.is_some_and(|tip| tip == commit.oid);
            let tip_marker = if is_tip { "@" } else { " " };
            let labels = if commit.branch_labels.is_empty() {
                String::new()
            } else {
                format!(" [{}]", commit.branch_labels.join(", "))
            };

            let style = if i == app.commit_index() && active {
                Style::default().bg(Color::DarkGray).fg(Color::Yellow)
            } else if is_tip {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{tip_marker}{graph:>graph_width$} "),
                    Style::default().fg(if is_tip { Color::Green } else { Color::Magenta }),
                ),
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
        .title(title)
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let list = List::new(items).block(block);
    render_list(
        frame,
        area,
        list,
        app.commit_index(),
        scroll_for(app.commit_index()),
        !app.commits().is_empty(),
        active,
    );
}

fn draw_files(frame: &mut Frame, app: &App, area: Rect) {
    let active = app.panel() == Panel::Files;
    let files = app.selected_files();
    let mode_label = match app.files_mode() {
        FilesMode::All => "All Files",
        FilesMode::Changed => "Changed Files",
        FilesMode::Working => "Working Tree",
    };

    let items: Vec<ListItem> = files
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            if app.files_mode() == FilesMode::Working {
                return draw_worktree_item(i, entry, active, app.file_index());
            }

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

    let title = match app.files_mode() {
        FilesMode::Working => format!(" {mode_label} — {} ", app.work_tree_summary()),
        _ => app
            .selected_commit()
            .map(|c| format!(" {mode_label} @ {} ", c.short_id))
            .unwrap_or_else(|| format!(" {mode_label} ")),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(panel_style(active));

    let empty_msg = match app.files_mode() {
        FilesMode::Working => "(working tree clean)",
        _ => "(no files — try c to toggle all/changed/working)",
    };

    let list = if items.is_empty() {
        List::new(vec![ListItem::new(empty_msg)]).block(block)
    } else {
        List::new(items).block(block)
    };

    render_list(
        frame,
        area,
        list,
        app.file_index(),
        scroll_for(app.file_index()),
        !files.is_empty(),
        active,
    );
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    list: List,
    selected: usize,
    scroll: usize,
    has_items: bool,
    highlight: bool,
) {
    let mut state = ListState::default();
    if highlight && has_items {
        state.select(Some(selected));
    }
    *state.offset_mut() = scroll;
    StatefulWidget::render(list, area, frame.buffer_mut(), &mut state);
}

fn draw_worktree_item(
    i: usize,
    entry: &crate::repo::TreeEntry,
    active: bool,
    file_index: usize,
) -> ListItem<'static> {
    let staged = status_label(entry.wt_staged);
    let unstaged = status_label(entry.wt_unstaged);
    let style = if i == file_index && active {
        Style::default().bg(Color::DarkGray).fg(Color::Yellow)
    } else {
        file_style(&entry.path)
    };

    ListItem::new(Line::from(vec![
        Span::styled(format!("S{staged} "), Style::default().fg(Color::Green)),
        Span::styled(format!("U{unstaged} ",), Style::default().fg(Color::Red)),
        Span::styled(entry.path.clone(), style),
    ]))
}

fn status_label(status: Option<ChangeStatus>) -> &'static str {
    match status {
        Some(ChangeStatus::Added) => "+",
        Some(ChangeStatus::Modified) => "M",
        Some(ChangeStatus::Deleted) => "-",
        Some(ChangeStatus::Renamed) => "R",
        None => " ",
    }
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

fn draw_search_bar(frame: &mut Frame, app: &App, area: Rect) {
    let query = app.search_input().unwrap_or("");
    let bar = Paragraph::new(format!(" / {query}_ "))
        .style(Style::default().fg(Color::Black).bg(Color::Cyan));
    frame.render_widget(bar, area);
}

fn draw_status(frame: &mut Frame, app: &mut App, area: Rect) {
    let status = Paragraph::new(app.status_line()).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, area);
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);

    let text = vec![
        Line::from(Span::styled(" repov — key bindings ", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(" Navigation"),
        Line::from("   Tab / Shift+Tab   switch panel (Refs → History → Files)"),
        Line::from("   j / k             move up / down"),
        Line::from("   g / G             jump to top / bottom of list"),
        Line::from("   PgUp / PgDn       page up / down"),
        Line::from(""),
        Line::from(" Search & view"),
        Line::from("   / or f            search commits (message, author, sha)"),
        Line::from("   Enter             open diff (History / Files)"),
        Line::from("   b                 branch line view (linear branch history)"),
        Line::from("   p                 toggle branch line / full merge graph"),
        Line::from("   c                 cycle files: changed → all → working tree"),
        Line::from("   y                 copy commit SHA"),
        Line::from("   Esc               close diff / search / help"),
        Line::from(""),
        Line::from(" Other"),
        Line::from("   r                 reload repository"),
        Line::from("   ?                 toggle this help"),
        Line::from("   q / Ctrl+C        quit"),
        Line::from(""),
        Line::from(" Refs: pick a branch (j/k) — History updates automatically. @ = branch tip."),
        Line::from(" Branches: green=HEAD, magenta=remote, yellow=tag. Enter on Refs → History."),
        Line::from(" Working tree mode (c) shows staged (S) and unstaged (U) changes."),
    ];

    let help = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true });

    frame.render_widget(help, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup[1])[1]
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
