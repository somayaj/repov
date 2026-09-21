use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use git2::{BranchType, Diff, DiffFormat, DiffOptions, Oid, ObjectType, Repository};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::graph::CommitNode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Branch { is_head: bool, is_remote: bool },
}

#[derive(Clone)]
pub struct RefEntry {
    pub kind: RefKind,
    pub name: String,
    pub short_id: String,
    pub tip_oid: String,
}

#[derive(Clone)]
pub struct TreeEntry {
    pub path: String,
    pub display: String,
    pub is_dir: bool,
    pub change: Option<ChangeStatus>,
    pub wt_staged: Option<ChangeStatus>,
    pub wt_unstaged: Option<ChangeStatus>,
}

#[derive(Clone)]
pub struct CommitInfo {
    pub oid: String,
    pub short_id: String,
    pub message: String,
    pub author: String,
    pub date: String,
    pub branch_labels: Vec<String>,
    pub parents: Vec<String>,
}

pub struct RepoData {
    pub repo_name: String,
    pub refs: Vec<RefEntry>,
    head_ref_index: usize,
    pub work_tree_summary: String,
}

impl RepoData {
    pub fn load(repo_path: &str) -> Result<Self> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let repo_name = Path::new(repo_path)
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| repo_path.to_string());

        let head_name = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from));

        let refs = load_branches(&repo, &head_name)?;
        let head_ref_index = refs
            .iter()
            .position(|r| matches!(r.kind, RefKind::Branch { is_head: true, .. }))
            .unwrap_or(0);
        let work_tree_summary = summarize_work_tree(&repo)?;

        Ok(Self {
            repo_name,
            refs,
            head_ref_index,
            work_tree_summary,
        })
    }

    pub fn head_branch_index(&self) -> usize {
        self.head_ref_index
    }

    pub fn load_working_tree(repo_path: &str) -> Result<Vec<TreeEntry>> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        load_working_tree_entries(&repo)
    }

    pub fn load_history(
        repo_path: &str,
        tip_oid: &str,
        limit: usize,
        first_parent: bool,
    ) -> Result<(Vec<CommitInfo>, Vec<crate::graph::GraphLine>)> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let tip = Oid::from_str(tip_oid).context("invalid branch tip")?;
        let raw_commits = walk_commits(&repo, vec![tip], limit, first_parent)?;
        let branch_map = ref_labels_for_commits(&repo, &raw_commits)?;

        let commits = raw_commits
            .into_iter()
            .map(|commit| commit_to_info(&commit, &branch_map))
            .collect::<Vec<_>>();

        let graph_nodes = commits
            .iter()
            .map(|c| CommitNode {
                id: c.oid.clone(),
                parents: c.parents.clone(),
            })
            .collect::<Vec<_>>();
        let graph = crate::graph::render_graph(&graph_nodes);

        Ok((commits, graph))
    }

    pub fn load_files_for_commit(oid: &str, repo_path: &str) -> Result<Vec<TreeEntry>> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let oid = Oid::from_str(oid).context("invalid commit oid")?;
        let commit = repo.find_commit(oid)?;
        load_tree_entries(&repo, &commit)
    }

    pub fn load_changed_files_for_commit(oid: &str, repo_path: &str) -> Result<Vec<TreeEntry>> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let oid = Oid::from_str(oid).context("invalid commit oid")?;
        let commit = repo.find_commit(oid)?;
        load_changed_entries(&repo, &commit)
    }

    pub fn load_commit_diff(oid: &str, repo_path: &str) -> Result<String> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let oid = Oid::from_str(oid).context("invalid commit oid")?;
        let commit = repo.find_commit(oid)?;
        format_commit_diff(&repo, &commit)
    }

    pub fn load_file_diff(oid: &str, path: &str, repo_path: &str) -> Result<String> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let oid = Oid::from_str(oid).context("invalid commit oid")?;
        let commit = repo.find_commit(oid)?;
        format_file_diff(&repo, &commit, path)
    }

    pub fn load_working_tree_file_diff(path: &str, repo_path: &str) -> Result<String> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        format_working_tree_file_diff(&repo, path)
    }
}

fn commit_to_info(
    commit: &git2::Commit,
    branch_map: &HashMap<Oid, Vec<String>>,
) -> CommitInfo {
    let oid = commit.id().to_string();
    let short_id = oid[..7].to_string();
    let message = commit
        .summary()
        .unwrap_or("(no message)")
        .lines()
        .next()
        .unwrap_or("(no message)")
        .to_string();
    let author = commit.author().name().unwrap_or("unknown").to_string();
    let date = format_time(commit.time().seconds());
    let branch_labels = branch_map
        .get(&commit.id())
        .cloned()
        .unwrap_or_default();
    let parents = commit
        .parent_ids()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();

    CommitInfo {
        oid,
        short_id,
        message,
        author,
        date,
        branch_labels,
        parents,
    }
}

fn load_branches(repo: &Repository, head_name: &Option<String>) -> Result<Vec<RefEntry>> {
    let mut branches = Vec::new();

    for branch_type in [BranchType::Local, BranchType::Remote] {
        for branch in repo.branches(Some(branch_type))? {
            let (branch, _) = branch?;
            let name = branch.name()?.unwrap_or("?").to_string();
            let is_head = head_name.as_deref() == Some(&name);
            let is_remote = branch_type == BranchType::Remote;
            let (short_id, tip_oid) = branch
                .get()
                .peel_to_commit()
                .map(|c| {
                    let id = c.id().to_string();
                    (id[..7].to_string(), id)
                })
                .unwrap_or_else(|_| ("???????".to_string(), String::new()));

            branches.push(RefEntry {
                kind: RefKind::Branch { is_head, is_remote },
                name,
                short_id,
                tip_oid,
            });
        }
    }

    branches.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(branches)
}

fn summarize_work_tree(repo: &Repository) -> Result<String> {
    let entries = load_working_tree_entries(repo)?;
    if entries.is_empty() {
        return Ok("clean".to_string());
    }
    let staged = entries.iter().filter(|e| e.wt_staged.is_some()).count();
    let unstaged = entries.iter().filter(|e| e.wt_unstaged.is_some()).count();
    Ok(format!("{staged} staged, {unstaged} unstaged"))
}

fn load_working_tree_entries(repo: &Repository) -> Result<Vec<TreeEntry>> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        let path = entry
            .path()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "?".to_string());
        let status = entry.status();
        let wt_staged = status_index_change(status);
        let wt_unstaged = status_workdir_change(status);

        if wt_staged.is_none() && wt_unstaged.is_none() {
            continue;
        }

        let display = match (&wt_staged, &wt_unstaged) {
            (Some(_), Some(_)) => format!("{path} (staged + unstaged)"),
            _ => path.clone(),
        };

        entries.push(TreeEntry {
            path: path.clone(),
            display,
            is_dir: false,
            change: wt_unstaged.or(wt_staged),
            wt_staged,
            wt_unstaged,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn status_index_change(status: git2::Status) -> Option<ChangeStatus> {
    if status.contains(git2::Status::INDEX_NEW) {
        Some(ChangeStatus::Added)
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        Some(ChangeStatus::Modified)
    } else if status.contains(git2::Status::INDEX_DELETED) {
        Some(ChangeStatus::Deleted)
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        Some(ChangeStatus::Renamed)
    } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
        Some(ChangeStatus::Modified)
    } else {
        None
    }
}

fn status_workdir_change(status: git2::Status) -> Option<ChangeStatus> {
    if status.contains(git2::Status::WT_NEW) {
        Some(ChangeStatus::Added)
    } else if status.contains(git2::Status::WT_MODIFIED) {
        Some(ChangeStatus::Modified)
    } else if status.contains(git2::Status::WT_DELETED) {
        Some(ChangeStatus::Deleted)
    } else if status.contains(git2::Status::WT_RENAMED) {
        Some(ChangeStatus::Renamed)
    } else if status.contains(git2::Status::WT_TYPECHANGE) {
        Some(ChangeStatus::Modified)
    } else {
        None
    }
}

fn walk_commits(
    repo: &Repository,
    start_oids: Vec<Oid>,
    limit: usize,
    first_parent: bool,
) -> Result<Vec<git2::Commit<'_>>> {
    let mut seen = HashSet::new();
    let mut queue: VecDeque<Oid> = start_oids.into_iter().collect();
    let mut commits = Vec::new();

    while let Some(oid) = queue.pop_front() {
        if seen.contains(&oid) || commits.len() >= limit {
            continue;
        }
        seen.insert(oid);

        let commit = repo.find_commit(oid)?;
        if first_parent {
            if commit.parent_count() > 0 {
                queue.push_back(commit.parent(0)?.id());
            }
        } else {
            for parent in commit.parents() {
                queue.push_back(parent.id());
            }
        }
        commits.push(commit);
    }

    Ok(commits)
}

fn ref_labels_for_commits(
    repo: &Repository,
    commits: &[git2::Commit],
) -> Result<HashMap<Oid, Vec<String>>> {
    let commit_ids: HashSet<Oid> = commits.iter().map(|c| c.id()).collect();
    let mut labels: HashMap<Oid, Vec<String>> = HashMap::new();

    for branch_type in [BranchType::Local, BranchType::Remote] {
        for branch in repo.branches(Some(branch_type))? {
            let (branch, _) = branch?;
            let name = branch.name()?.unwrap_or("?").to_string();
            if let Ok(commit) = branch.into_reference().peel_to_commit() {
                if commit_ids.contains(&commit.id()) {
                    labels.entry(commit.id()).or_default().push(name);
                }
            }
        }
    }

    for name in repo.tag_names(None)?.iter().flatten() {
        let Ok(reference) = repo.find_reference(&format!("refs/tags/{name}")) else {
            continue;
        };
        let Ok(commit) = reference.peel_to_commit() else {
            continue;
        };
        if commit_ids.contains(&commit.id()) {
            labels
                .entry(commit.id())
                .or_default()
                .push(format!("tag:{name}"));
        }
    }

    Ok(labels)
}

fn load_tree_entries(repo: &Repository, commit: &git2::Commit) -> Result<Vec<TreeEntry>> {
    let tree = commit.tree()?;
    let mut entries = Vec::new();
    flatten_tree(repo, &tree, PathBuf::new(), &mut entries, None)?;
    Ok(entries)
}

fn load_changed_entries(repo: &Repository, commit: &git2::Commit) -> Result<Vec<TreeEntry>> {
    let diff = commit_tree_diff(repo, commit, None)?;
    let mut by_path: HashMap<String, TreeEntry> = HashMap::new();
    collect_changed_paths(repo, commit, &diff, &mut by_path)?;

    let mut entries: Vec<_> = by_path.into_values().collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// Diff commit tree against parent(s). Merge commits combine diffs vs each parent (git diff-tree -m).
fn commit_tree_diff<'a>(
    repo: &'a Repository,
    commit: &git2::Commit,
    pathspec: Option<&str>,
) -> Result<Diff<'a>> {
    let tree = commit.tree()?;
    let parent_count = commit.parent_count();

    let mut opts = DiffOptions::new();
    if let Some(pathspec) = pathspec {
        opts.pathspec(pathspec);
    }

    if parent_count <= 1 {
        let parent_tree = if parent_count == 1 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };
        return Ok(repo.diff_tree_to_tree(
            parent_tree.as_ref(),
            Some(&tree),
            Some(&mut opts),
        )?);
    }

    let mut combined = repo.diff_tree_to_tree(
        Some(&commit.parent(0)?.tree()?),
        Some(&tree),
        Some(&mut opts),
    )?;

    for i in 1..parent_count {
        let mut parent_opts = DiffOptions::new();
        if let Some(pathspec) = pathspec {
            parent_opts.pathspec(pathspec);
        }
        let parent_tree = commit.parent(i)?.tree()?;
        let extra =
            repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut parent_opts))?;
        combined.merge(&extra)?;
    }

    Ok(combined)
}

fn collect_changed_paths(
    repo: &Repository,
    commit: &git2::Commit,
    diff: &Diff,
    out: &mut HashMap<String, TreeEntry>,
) -> Result<()> {
    diff.foreach(
        &mut |delta, _| {
            let status = map_delta_status(delta.status());
            let old_path = delta
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().to_string());
            let new_path = delta
                .new_file()
                .path()
                .map(|p| p.to_string_lossy().to_string());
            let path = new_path
                .clone()
                .or(old_path.clone())
                .unwrap_or_else(|| "?".to_string());
            let is_dir = delta.new_file().mode() == git2::FileMode::Tree
                || delta.old_file().mode() == git2::FileMode::Tree;

            if is_dir {
                let base = new_path.as_deref().or(old_path.as_deref()).unwrap_or(&path);
                let pathspec = format!("{}/**", base.trim_end_matches('/'));
                if let Ok(sub) = commit_tree_diff(repo, commit, Some(&pathspec)) {
                    let _ = collect_changed_paths(repo, commit, &sub, out);
                }
            } else {
                insert_changed_file(out, path, status);
            }
            true
        },
        None,
        None,
        None,
    )?;
    Ok(())
}

fn insert_changed_file(out: &mut HashMap<String, TreeEntry>, path: String, status: ChangeStatus) {
    out.entry(path.clone()).or_insert_with(|| TreeEntry {
        path: path.clone(),
        display: path,
        is_dir: false,
        change: Some(status),
        wt_staged: None,
        wt_unstaged: None,
    });
}

fn map_delta_status(status: git2::Delta) -> ChangeStatus {
    match status {
        git2::Delta::Added => ChangeStatus::Added,
        git2::Delta::Deleted => ChangeStatus::Deleted,
        git2::Delta::Renamed | git2::Delta::Copied => ChangeStatus::Renamed,
        _ => ChangeStatus::Modified,
    }
}

fn flatten_tree(
    repo: &Repository,
    tree: &git2::Tree,
    prefix: PathBuf,
    out: &mut Vec<TreeEntry>,
    change: Option<ChangeStatus>,
) -> Result<()> {
    for entry in tree.iter() {
        let name = entry.name().unwrap_or("?").to_string();
        let path = prefix.join(&name);
        let path_str = path.to_string_lossy().to_string();
        let is_dir = entry.kind() == Some(ObjectType::Tree);
        let suffix = if is_dir { "/" } else { "" };

        out.push(TreeEntry {
            path: path_str.clone(),
            display: format!("{}{}", path_str, suffix),
            is_dir,
            change,
            wt_staged: None,
            wt_unstaged: None,
        });

        if is_dir {
            let subtree = repo.find_tree(entry.id())?;
            flatten_tree(repo, &subtree, path, out, change)?;
        }
    }

    Ok(())
}

fn format_commit_diff(repo: &Repository, commit: &git2::Commit) -> Result<String> {
    let diff = commit_tree_diff(repo, commit, None)?;
    patch_to_string(&diff)
}

fn format_working_tree_file_diff(repo: &Repository, path: &str) -> Result<String> {
    let mut sections = Vec::new();

    if let Ok(head) = repo.head() {
        if let Ok(head_tree) = head.peel_to_tree() {
            let mut opts = DiffOptions::new();
            opts.pathspec(path);
            let diff = repo.diff_tree_to_index(Some(&head_tree), None, Some(&mut opts))?;
            let staged = patch_to_string(&diff)?;
            if !staged.trim().is_empty() {
                sections.push(format!("--- staged ({path}) ---"));
                sections.push(staged);
            }
        }
    }

    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    let unstaged = patch_to_string(&diff)?;
    if !unstaged.trim().is_empty() {
        sections.push(format!("--- unstaged ({path}) ---"));
        sections.push(unstaged);
    }

    Ok(sections.join("\n"))
}

fn format_file_diff(repo: &Repository, commit: &git2::Commit, path: &str) -> Result<String> {
    let diff = commit_tree_diff(repo, commit, Some(path))?;
    patch_to_string(&diff)
}

fn patch_to_string(diff: &Diff) -> Result<String> {
    let mut output = Vec::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let prefix = match line.origin() {
            '+' | '-' | ' ' => line.origin() as char,
            _ => ' ',
        };
        output.push(format!("{}{}", prefix, std::str::from_utf8(line.content()).unwrap_or("")));
        true
    })?;
    Ok(output.join("\n"))
}

fn format_time(seconds: i64) -> String {
    Local
        .timestamp_opt(seconds, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_numbers_descend_down_history() {
        use crate::graph::render_graph;
        use crate::graph::CommitNode;

        let repo = Repository::open(".").unwrap();
        let tip = repo.head().unwrap().peel_to_commit().unwrap();
        let (commits, _) =
            RepoData::load_history(".", &tip.id().to_string(), 500, false).unwrap();
        let nodes: Vec<CommitNode> = commits
            .iter()
            .map(|c| CommitNode {
                id: c.oid.clone(),
                parents: c.parents.clone(),
            })
            .collect();
        let graph = render_graph(&nodes);
        let mut last_num = usize::MAX;
        for (c, line) in commits.iter().zip(graph.iter()) {
            if c.parents.len() <= 1 {
                continue;
            }
            let num = line
                .symbols
                .chars()
                .find(|ch| ch.is_ascii_digit() || *ch == '+')
                .and_then(|ch| {
                    if ch == '+' {
                        Some(10)
                    } else {
                        ch.to_digit(10).map(|d| d as usize)
                    }
                })
                .expect("merge line should have number");
            assert!(
                num <= last_num,
                "merge numbers should descend down history: {} after {} ({})",
                num,
                last_num,
                c.message
            );
            last_num = num;
        }
    }

    #[test]
    fn merge_commit_lists_changed_files() {
        let repo = Repository::open(".").expect("open repo");
        let oid = Oid::from_str("53760c978398b46d265d93ae9c0da90acaaa5493").expect("oid");
        let commit = repo.find_commit(oid).expect("commit");
        assert!(commit.parent_count() > 1, "expected merge commit");

        let entries = load_changed_entries(&repo, &commit).expect("changed entries");
        assert_eq!(entries.len(), 6, "merge commit changed file count");
        assert!(entries.iter().all(|e| !e.is_dir));
    }
}
