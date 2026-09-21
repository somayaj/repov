use anyhow::{Context, Result};
use git2::{BranchType, Oid, ObjectType, Repository};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::graph::CommitNode;

#[derive(Clone)]
pub struct BranchInfo {
    pub name: String,
    pub short_id: String,
    pub is_head: bool,
    pub is_remote: bool,
}

#[derive(Clone)]
pub struct TreeEntry {
    pub display: String,
    pub is_dir: bool,
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
    pub branches: Vec<BranchInfo>,
    pub commits: Vec<CommitInfo>,
    pub graph: Vec<crate::graph::GraphLine>,
    head_branch_index: usize,
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

        let branches = load_branches(&repo, &head_name)?;
        let head_branch_index = branches.iter().position(|b| b.is_head).unwrap_or(0);

        let start_oids = collect_branch_tips(&repo)?;
        let raw_commits = walk_commits(&repo, start_oids, 500)?;
        let branch_map = branch_labels_for_commits(&repo, &raw_commits)?;

        let commits = raw_commits
            .into_iter()
            .map(|commit| {
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
            })
            .collect::<Vec<_>>();

        let graph_nodes = commits
            .iter()
            .map(|c| CommitNode {
                id: c.oid.clone(),
                parents: c.parents.clone(),
            })
            .collect::<Vec<_>>();
        let graph = crate::graph::render_graph(&graph_nodes);

        Ok(Self {
            repo_name,
            branches,
            commits,
            graph,
            head_branch_index,
        })
    }

    pub fn head_branch_index(&self) -> usize {
        self.head_branch_index
    }

    pub fn load_files_for_commit(oid: &str, repo_path: &str) -> Result<Vec<TreeEntry>> {
        let repo = Repository::open(repo_path).context("failed to open repository")?;
        let oid = Oid::from_str(oid).context("invalid commit oid")?;
        let commit = repo.find_commit(oid)?;
        load_tree_entries(&repo, &commit)
    }
}

fn load_branches(repo: &Repository, head_name: &Option<String>) -> Result<Vec<BranchInfo>> {
    let mut branches = Vec::new();

    for branch_type in [BranchType::Local, BranchType::Remote] {
        for branch in repo.branches(Some(branch_type))? {
            let (branch, _) = branch?;
            let name = branch.name()?.unwrap_or("?").to_string();
            let is_head = head_name.as_deref() == Some(&name);
            let is_remote = branch_type == BranchType::Remote;
            let short_id = branch
                .get()
                .peel_to_commit()
                .map(|c| c.id().to_string()[..7].to_string())
                .unwrap_or_else(|_| "???????".to_string());

            branches.push(BranchInfo {
                name,
                short_id,
                is_head,
                is_remote,
            });
        }
    }

    branches.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(branches)
}

fn collect_branch_tips(repo: &Repository) -> Result<Vec<Oid>> {
    let mut tips = HashSet::new();

    for branch_type in [BranchType::Local, BranchType::Remote] {
        for branch in repo.branches(Some(branch_type))? {
            let (branch, _) = branch?;
            if let Ok(commit) = branch.into_reference().peel_to_commit() {
                tips.insert(commit.id());
            }
        }
    }

    if tips.is_empty() {
        if let Ok(head) = repo.head() {
            if let Ok(commit) = head.peel_to_commit() {
                tips.insert(commit.id());
            }
        }
    }

    Ok(tips.into_iter().collect())
}

fn walk_commits(
    repo: &Repository,
    start_oids: Vec<Oid>,
    limit: usize,
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
        for parent in commit.parents() {
            queue.push_back(parent.id());
        }
        commits.push(commit);
    }

    Ok(commits)
}

fn branch_labels_for_commits(
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

    Ok(labels)
}

fn load_tree_entries(repo: &Repository, commit: &git2::Commit) -> Result<Vec<TreeEntry>> {
    let tree = commit.tree()?;
    let mut entries = Vec::new();
    flatten_tree(repo, &tree, PathBuf::new(), &mut entries)?;
    Ok(entries)
}

fn flatten_tree(
    repo: &Repository,
    tree: &git2::Tree,
    prefix: PathBuf,
    out: &mut Vec<TreeEntry>,
) -> Result<()> {
    for entry in tree.iter() {
        let name = entry.name().unwrap_or("?").to_string();
        let path = prefix.join(&name);
        let is_dir = entry.kind() == Some(ObjectType::Tree);
        let suffix = if is_dir { "/" } else { "" };

        out.push(TreeEntry {
            display: format!("{}{}", path.display(), suffix),
            is_dir,
        });

        if is_dir {
            let subtree = repo.find_tree(entry.id())?;
            flatten_tree(repo, &subtree, path, out)?;
        }
    }

    Ok(())
}

fn format_time(seconds: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let datetime = UNIX_EPOCH + Duration::from_secs(seconds as u64);
    format!("{:?}", datetime).replace(" 00:00:00 UTC", "")
}
