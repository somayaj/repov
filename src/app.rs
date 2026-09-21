use anyhow::Result;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::diff::{parse_patch, DiffLine};
use crate::graph::GraphLine;
use crate::repo::{BranchInfo, CommitInfo, RepoData, TreeEntry};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Refs,
    History,
    Files,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FilesMode {
    All,
    Changed,
}

pub struct DiffView {
    pub title: String,
    pub lines: Vec<DiffLine>,
    pub scroll: u16,
}

pub struct App {
    repo_path: String,
    data: RepoData,
    panel: Panel,
    branch_index: usize,
    commit_index: usize,
    file_index: usize,
    display_commits: Vec<CommitInfo>,
    display_graph: Vec<GraphLine>,
    files: Vec<TreeEntry>,
    files_mode: FilesMode,
    files_cache: HashMap<String, Vec<TreeEntry>>,
    diff_view: Option<DiffView>,
    flash: Option<(String, Instant)>,
}

impl App {
    pub fn open(repo_path: &str) -> Result<Self> {
        let data = RepoData::load(repo_path)?;
        let head_branch_index = data.head_branch_index();

        let mut app = Self {
            repo_path: repo_path.to_string(),
            data,
            panel: Panel::History,
            branch_index: head_branch_index,
            commit_index: 0,
            file_index: 0,
            display_commits: Vec::new(),
            display_graph: Vec::new(),
            files: Vec::new(),
            files_mode: FilesMode::Changed,
            files_cache: HashMap::new(),
            diff_view: None,
            flash: None,
        };
        app.refresh_history()?;
        app.load_selected_files()?;
        Ok(app)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.data = RepoData::load(&self.repo_path)?;
        self.branch_index = self.data.head_branch_index();
        self.commit_index = 0;
        self.file_index = 0;
        self.files_cache.clear();
        self.diff_view = None;
        self.refresh_history()?;
        self.load_selected_files()?;
        Ok(())
    }

    pub fn diff_is_open(&self) -> bool {
        self.diff_view.is_some()
    }

    pub fn close_diff(&mut self) {
        self.diff_view = None;
    }

    pub fn open_commit_diff(&mut self) -> Result<()> {
        let Some(commit) = self.selected_commit() else {
            return Ok(());
        };
        let patch = RepoData::load_commit_diff(&commit.oid, &self.repo_path)?;
        if patch.trim().is_empty() {
            self.set_flash("No diff for this commit");
            return Ok(());
        }
        self.diff_view = Some(DiffView {
            title: format!("Diff @ {} — {}", commit.short_id, commit.message),
            lines: parse_patch(&patch),
            scroll: 0,
        });
        Ok(())
    }

    pub fn open_file_diff(&mut self) -> Result<()> {
        let Some(commit) = self.selected_commit() else {
            return Ok(());
        };
        let Some(entry) = self.files.get(self.file_index) else {
            return Ok(());
        };
        if entry.is_dir {
            self.set_flash("Select a file, not a directory");
            return Ok(());
        }

        let patch = RepoData::load_file_diff(&commit.oid, &entry.path, &self.repo_path)?;
        if patch.trim().is_empty() {
            self.set_flash("No diff for this file");
            return Ok(());
        }
        self.diff_view = Some(DiffView {
            title: format!("{} @ {}", entry.path, commit.short_id),
            lines: parse_patch(&patch),
            scroll: 0,
        });
        Ok(())
    }

    pub fn scroll_diff_down(&mut self) {
        if let Some(diff) = &mut self.diff_view {
            diff.scroll = diff.scroll.saturating_add(1);
        }
    }

    pub fn scroll_diff_up(&mut self) {
        if let Some(diff) = &mut self.diff_view {
            diff.scroll = diff.scroll.saturating_sub(1);
        }
    }

    pub fn toggle_files_mode(&mut self) {
        self.files_mode = match self.files_mode {
            FilesMode::All => FilesMode::Changed,
            FilesMode::Changed => FilesMode::All,
        };
        self.files_cache.clear();
        let _ = self.load_selected_files();
    }

    pub fn copy_sha(&mut self) {
        let Some(commit) = self.selected_commit() else {
            return;
        };
        match copy_to_clipboard(&commit.oid) {
            Ok(()) => self.set_flash(&format!("Copied {}", commit.short_id)),
            Err(_) => self.set_flash("Failed to copy to clipboard"),
        }
    }

    fn set_flash(&mut self, message: &str) {
        self.flash = Some((message.to_string(), Instant::now()));
    }

    fn take_flash(&mut self) -> Option<String> {
        let expired = self
            .flash
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(2));
        if expired {
            self.flash = None;
        }
        self.flash.as_ref().map(|(msg, _)| msg.clone())
    }

    pub fn next_panel(&mut self) {
        self.panel = match self.panel {
            Panel::Refs => Panel::History,
            Panel::History => Panel::Files,
            Panel::Files => Panel::Refs,
        };
    }

    pub fn prev_panel(&mut self) {
        self.panel = match self.panel {
            Panel::Refs => Panel::Files,
            Panel::History => Panel::Refs,
            Panel::Files => Panel::History,
        };
    }

    pub fn move_down(&mut self) {
        if self.diff_view.is_some() {
            self.scroll_diff_down();
            return;
        }

        match self.panel {
            Panel::Refs => {
                if self.branch_index + 1 < self.data.branches.len() {
                    self.branch_index += 1;
                    let _ = self.refresh_history();
                    let _ = self.load_selected_files();
                }
            }
            Panel::History => {
                if self.commit_index + 1 < self.display_commits.len() {
                    self.commit_index += 1;
                    self.file_index = 0;
                    let _ = self.load_selected_files();
                }
            }
            Panel::Files => {
                if self.file_index + 1 < self.files.len() {
                    self.file_index += 1;
                }
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.diff_view.is_some() {
            self.scroll_diff_up();
            return;
        }

        match self.panel {
            Panel::Refs => {
                if self.branch_index > 0 {
                    self.branch_index -= 1;
                    let _ = self.refresh_history();
                    let _ = self.load_selected_files();
                }
            }
            Panel::History => {
                if self.commit_index > 0 {
                    self.commit_index -= 1;
                    self.file_index = 0;
                    let _ = self.load_selected_files();
                }
            }
            Panel::Files => {
                if self.file_index > 0 {
                    self.file_index -= 1;
                }
            }
        }
    }

    fn refresh_history(&mut self) -> Result<()> {
        let Some(branch) = self.data.branches.get(self.branch_index) else {
            self.display_commits.clear();
            self.display_graph.clear();
            return Ok(());
        };

        if branch.tip_oid.is_empty() {
            self.display_commits.clear();
            self.display_graph.clear();
            return Ok(());
        }

        let (commits, graph) =
            RepoData::load_history(&self.repo_path, &branch.tip_oid, 500)?;
        self.display_commits = commits;
        self.display_graph = graph;
        self.commit_index = 0;
        Ok(())
    }

    fn load_selected_files(&mut self) -> Result<()> {
        let oid = self
            .display_commits
            .get(self.commit_index)
            .map(|c| c.oid.clone());

        let mode_key = match self.files_mode {
            FilesMode::All => "all",
            FilesMode::Changed => "changed",
        };

        if let Some(oid) = oid {
            let cache_key = format!("{}:{}", oid, mode_key);
            if let Some(cached) = self.files_cache.get(&cache_key) {
                self.files = cached.clone();
                return Ok(());
            }

            let entries = match self.files_mode {
                FilesMode::All => RepoData::load_files_for_commit(&oid, &self.repo_path)?,
                FilesMode::Changed => {
                    RepoData::load_changed_files_for_commit(&oid, &self.repo_path)?
                }
            };
            self.files_cache.insert(cache_key, entries.clone());
            self.files = entries;
        } else {
            self.files.clear();
        }

        Ok(())
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn files_mode(&self) -> FilesMode {
        self.files_mode
    }

    pub fn diff_view(&self) -> Option<&DiffView> {
        self.diff_view.as_ref()
    }

    pub fn branches(&self) -> &[BranchInfo] {
        &self.data.branches
    }

    pub fn branch_index(&self) -> usize {
        self.branch_index
    }

    pub fn selected_branch(&self) -> Option<&BranchInfo> {
        self.data.branches.get(self.branch_index)
    }

    pub fn commits(&self) -> &[CommitInfo] {
        &self.display_commits
    }

    pub fn commit_index(&self) -> usize {
        self.commit_index
    }

    pub fn graph_line(&self, index: usize) -> &str {
        self.display_graph
            .get(index)
            .map(|g| g.symbols.as_str())
            .unwrap_or(" ")
    }

    pub fn selected_commit(&self) -> Option<&CommitInfo> {
        self.display_commits.get(self.commit_index)
    }

    pub fn selected_files(&self) -> &[TreeEntry] {
        &self.files
    }

    pub fn file_index(&self) -> usize {
        self.file_index
    }

    pub fn status_line(&mut self) -> String {
        if let Some(msg) = self.take_flash() {
            return format!("{msg} | Esc: close diff | q: quit");
        }

        if self.diff_view.is_some() {
            return "Diff | j/k: scroll | Esc: close | q: quit".to_string();
        }

        let branch = self
            .selected_branch()
            .map(|b| b.name.as_str())
            .unwrap_or("-");
        let commit = self.selected_commit();
        let author = commit.map(|c| c.author.as_str()).unwrap_or("-");
        let date = commit.map(|c| c.date.as_str()).unwrap_or("-");
        let id = commit.map(|c| c.short_id.as_str()).unwrap_or("-");
        let files_mode = match self.files_mode {
            FilesMode::All => "all files",
            FilesMode::Changed => "changed",
        };

        format!(
            "repov | {} | {branch} | {id} | {author} | {date} | {files_mode} | Enter: diff | c: files | y: copy sha | Tab/j/k/r/q",
            self.data.repo_name
        )
    }
}

fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}
