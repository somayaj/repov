use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Header,
    Section,
    Hunk,
    Add,
    Remove,
    Context,
}

#[derive(Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
}

pub fn parse_patch(patch: &str) -> Vec<DiffLine> {
    patch
        .lines()
        .map(|line| {
            let kind = if line.starts_with("--- staged") || line.starts_with("--- unstaged") {
                DiffLineKind::Section
            } else if line.starts_with("+++") || line.starts_with("---") {
                DiffLineKind::Header
            } else if line.starts_with("@@") {
                DiffLineKind::Hunk
            } else if line.starts_with('+') {
                DiffLineKind::Add
            } else if line.starts_with('-') {
                DiffLineKind::Remove
            } else {
                DiffLineKind::Context
            };
            DiffLine {
                kind,
                text: line.to_string(),
            }
        })
        .collect()
}

pub fn styled_line(line: &DiffLine) -> Line<'static> {
    let style = match line.kind {
        DiffLineKind::Header => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        DiffLineKind::Section => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        DiffLineKind::Hunk => Style::default().fg(Color::Cyan),
        DiffLineKind::Add => Style::default().fg(Color::Green),
        DiffLineKind::Remove => Style::default().fg(Color::Red),
        DiffLineKind::Context => Style::default().fg(Color::DarkGray),
    };
    Line::from(Span::styled(line.text.clone(), style))
}
