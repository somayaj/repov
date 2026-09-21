use std::collections::HashMap;

#[derive(Clone)]
pub struct CommitNode {
    pub id: String,
    pub parents: Vec<String>,
}

#[derive(Clone)]
pub struct GraphLine {
    pub symbols: String,
}

/// Render a gitk-style lane graph for commits (newest first).
pub fn render_graph(commits: &[CommitNode]) -> Vec<GraphLine> {
    if commits.is_empty() {
        return Vec::new();
    }

    let id_to_index: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    let mut lanes: Vec<Option<String>> = Vec::new();
    let mut lines = Vec::with_capacity(commits.len());
    let mut max_width = 0;
    let mut merge_num = 0;

    for commit in commits {
        let col = lanes
            .iter()
            .position(|lane| lane.as_deref() == Some(commit.id.as_str()))
            .unwrap_or_else(|| {
                lanes.push(Some(commit.id.clone()));
                lanes.len() - 1
            });

        max_width = max_width.max(lanes.len());

        let node = if commit.parents.len() > 1 {
            merge_num += 1;
            merge_node_char(merge_num)
        } else {
            '●'
        };

        let mut parts = Vec::with_capacity(lanes.len());
        for (i, lane) in lanes.iter().enumerate() {
            let ch = if i == col {
                node
            } else if lane.is_some() {
                '│'
            } else {
                ' '
            };
            parts.push(ch);
            if i + 1 < lanes.len() {
                parts.push(' ');
            }
        }
        lines.push(GraphLine {
            symbols: parts.into_iter().collect(),
        });

        lanes[col] = None;

        for parent in &commit.parents {
            if !id_to_index.contains_key(parent.as_str()) {
                continue;
            }

            if lanes.iter().any(|lane| lane.as_deref() == Some(parent.as_str())) {
                continue;
            }

            if let Some(empty) = lanes.iter().position(|lane| lane.is_none()) {
                lanes[empty] = Some(parent.clone());
            } else {
                lanes.push(Some(parent.clone()));
            }
        }

        max_width = max_width.max(lanes.len());
    }

    for line in &mut lines {
        pad_graph_line(line, max_width);
    }

    lines
}

/// Merge commits are numbered 1–9 (newest first); 10+ shown as '+'.
fn merge_node_char(n: usize) -> char {
    match n {
        1..=9 => char::from_digit(n as u32, 10).unwrap_or('+'),
        _ => '+',
    }
}

fn pad_graph_line(line: &mut GraphLine, lanes: usize) {
    let target_len = lanes * 2 - 1;
    if line.symbols.len() < target_len {
        let pad = target_len - line.symbols.len();
        line.symbols.push_str(&" ".repeat(pad));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_commits_numbered_newest_first() {
        let commits = vec![
            CommitNode {
                id: "c1".into(),
                parents: vec!["c2".into(), "c3".into()],
            },
            CommitNode {
                id: "c2".into(),
                parents: vec!["c4".into()],
            },
            CommitNode {
                id: "c3".into(),
                parents: vec!["c4".into()],
            },
            CommitNode {
                id: "c4".into(),
                parents: vec!["c5".into(), "c6".into()],
            },
            CommitNode {
                id: "c5".into(),
                parents: vec![],
            },
            CommitNode {
                id: "c6".into(),
                parents: vec![],
            },
        ];

        let lines = render_graph(&commits);
        assert!(lines[0].symbols.contains('1'), "newest merge is 1");
        assert!(lines[3].symbols.contains('2'), "older merge is 2");
        assert!(lines[1].symbols.contains('●'));
    }
}
