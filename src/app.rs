use anyhow::Result;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::diff::{parse_patch, DiffLine};
use crate::graph::{render_graph, CommitNode, GraphLine};
use crate::repo::{CommitInfo, RefEntry, RepoData, TreeEntry};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Refs,
    History,
    Files,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FilesMode {
    Changed,
    All,
    Working,
}

pub struct DiffView {
    pub title: String,
    pub lines: Vec<DiffLine>,
    pub scroll: u16,
}

const PAGE_SIZE: usize = 10;

pub struct App {
    repo_path: String,
    data: RepoData,
    panel: Panel,
    ref_index: usize,
    commit_index: usize,
    file_index: usize,
    all_commits: Vec<CommitInfo>,
    all_graph: Vec<GraphLine>,
    display_commits: Vec<CommitInfo>,
    display_graph: Vec<GraphLine>,
    files: Vec<TreeEntry>,
    files_mode: FilesMode,
    files_cache: HashMap<String, Vec<TreeEntry>>,
    diff_view: Option<DiffView>,
    flash: Option<(String, Instant)>,
    search_input: Option<String>,
    search_query: String,
    show_help: bool,
    list_scroll: usize,
}

impl App {
    pub fn open(repo_path: &str) -> Result<Self> {
        let data = RepoData::load(repo_path)?;
        let head_ref_index = data.head_branch_index();

        let mut app = Self {
            repo_path: repo_path.to_string(),
            data,
            panel: Panel::History,
            ref_index: head_ref_index,
            commit_index: 0,
            file_index: 0,
            all_commits: Vec::new(),
            all_graph: Vec::new(),
            display_commits: Vec::new(),
            display_graph: Vec::new(),
            files: Vec::new(),
            files_mode: FilesMode::Changed,
            files_cache: HashMap::new(),
            diff_view: None,
            flash: None,
            search_input: None,
            search_query: String::new(),
            show_help: false,
            list_scroll: 0,
        };
        app.refresh_history()?;
        app.load_selected_files()?;
        Ok(app)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.data = RepoData::load(&self.repo_path)?;
        self.ref_index = self.data.head_branch_index();
        self.commit_index = 0;
        self.file_index = 0;
        self.files_cache.clear();
        self.diff_view = None;
        self.search_query.clear();
        self.search_input = None;
        self.refresh_history()?;
        self.load_selected_files()?;
        Ok(())
    }

    pub fn diff_is_open(&self) -> bool {
        self.diff_view.is_some()
    }

    pub fn show_help(&self) -> bool {
        self.show_help
    }

    pub fn search_input(&self) -> Option<&str> {
        self.search_input.as_deref()
    }

    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    pub fn list_scroll(&self) -> usize {
        self.list_scroll
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn close_help(&mut self) {
        self.show_help = false;
    }

    pub fn start_search(&mut self) {
        self.search_input = Some(self.search_query.clone());
        self.show_help = false;
    }

    pub fn cancel_search(&mut self) {
        self.search_input = None;
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_input = None;
        self.apply_search_filter();
    }

    pub fn search_push(&mut self, ch: char) {
        if let Some(buf) = &mut self.search_input {
            buf.push(ch);
        }
    }

    pub fn search_backspace(&mut self) {
        if let Some(buf) = &mut self.search_input {
            buf.pop();
        }
    }

    pub fn apply_search(&mut self) {
        if let Some(q) = self.search_input.take() {
            self.search_query = q;
            self.apply_search_filter();
        }
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
        let Some(entry) = self.files.get(self.file_index) else {
            return Ok(());
        };
        if entry.is_dir {
            self.set_flash("Select a file, not a directory");
            return Ok(());
        }

        let (title, patch) = if self.files_mode == FilesMode::Working {
            let patch = RepoData::load_working_tree_file_diff(&entry.path, &self.repo_path)?;
            (format!("{} (working tree)", entry.path), patch)
        } else {
            let Some(commit) = self.selected_commit() else {
                return Ok(());
            };
            let patch = RepoData::load_file_diff(&commit.oid, &entry.path, &self.repo_path)?;
            (format!("{} @ {}", entry.path, commit.short_id), patch)
        };

        if patch.trim().is_empty() {
            self.set_flash("No diff for this file");
            return Ok(());
        }
        self.diff_view = Some(DiffView {
            title,
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
            FilesMode::Changed => FilesMode::All,
            FilesMode::All => FilesMode::Working,
            FilesMode::Working => FilesMode::Changed,
        };
        self.files_cache.clear();
        self.file_index = 0;
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

    pub fn jump_top(&mut self) {
        match self.panel {
            Panel::Refs => self.ref_index = 0,
            Panel::History => self.commit_index = 0,
            Panel::Files => self.file_index = 0,
        }
        self.sync_scroll();
        if self.panel == Panel::Refs {
            let _ = self.refresh_history();
            let _ = self.load_selected_files();
        } else if self.panel == Panel::History {
            self.file_index = 0;
            let _ = self.load_selected_files();
        }
    }

    pub fn jump_bottom(&mut self) {
        match self.panel {
            Panel::Refs => {
                if !self.data.refs.is_empty() {
                    self.ref_index = self.data.refs.len() - 1;
                }
            }
            Panel::History => {
                if !self.display_commits.is_empty() {
                    self.commit_index = self.display_commits.len() - 1;
                }
            }
            Panel::Files => {
                if !self.files.is_empty() {
                    self.file_index = self.files.len() - 1;
                }
            }
        }
        self.sync_scroll();
        if self.panel == Panel::Refs {
            let _ = self.refresh_history();
            let _ = self.load_selected_files();
        } else if self.panel == Panel::History {
            self.file_index = 0;
            let _ = self.load_selected_files();
        }
    }

    pub fn page_up(&mut self) {
        match self.panel {
            Panel::Refs => self.ref_index = self.ref_index.saturating_sub(PAGE_SIZE),
            Panel::History => {
                self.commit_index = self.commit_index.saturating_sub(PAGE_SIZE);
                self.file_index = 0;
                let _ = self.load_selected_files();
            }
            Panel::Files => self.file_index = self.file_index.saturating_sub(PAGE_SIZE),
        }
        self.sync_scroll();
        if self.panel == Panel::Refs {
            let _ = self.refresh_history();
            let _ = self.load_selected_files();
        }
    }

    pub fn page_down(&mut self) {
        match self.panel {
            Panel::Refs => {
                if !self.data.refs.is_empty() {
                    self.ref_index = (self.ref_index + PAGE_SIZE).min(self.data.refs.len() - 1);
                }
            }
            Panel::History => {
                if !self.display_commits.is_empty() {
                    self.commit_index =
                        (self.commit_index + PAGE_SIZE).min(self.display_commits.len() - 1);
                    self.file_index = 0;
                    let _ = self.load_selected_files();
                }
            }
            Panel::Files => {
                if !self.files.is_empty() {
                    self.file_index = (self.file_index + PAGE_SIZE).min(self.files.len() - 1);
                }
            }
        }
        self.sync_scroll();
        if self.panel == Panel::Refs {
            let _ = self.refresh_history();
            let _ = self.load_selected_files();
        }
    }

    fn sync_scroll(&mut self) {
        let selected = match self.panel {
            Panel::Refs => self.ref_index,
            Panel::History => self.commit_index,
            Panel::Files => self.file_index,
        };
        if selected >= PAGE_SIZE {
            self.list_scroll = selected.saturating_sub(PAGE_SIZE / 2);
        } else {
            self.list_scroll = 0;
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
        self.sync_scroll();
    }

    pub fn prev_panel(&mut self) {
        self.panel = match self.panel {
            Panel::Refs => Panel::Files,
            Panel::History => Panel::Refs,
            Panel::Files => Panel::History,
        };
        self.sync_scroll();
    }

    pub fn move_down(&mut self) {
        if self.diff_view.is_some() {
            self.scroll_diff_down();
            return;
        }

        match self.panel {
            Panel::Refs => {
                if self.ref_index + 1 < self.data.refs.len() {
                    self.ref_index += 1;
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
        self.sync_scroll();
    }

    pub fn move_up(&mut self) {
        if self.diff_view.is_some() {
            self.scroll_diff_up();
            return;
        }

        match self.panel {
            Panel::Refs => {
                if self.ref_index > 0 {
                    self.ref_index -= 1;
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
        self.sync_scroll();
    }

    fn apply_search_filter(&mut self) {
        let query = self.search_query.to_lowercase();
        if query.is_empty() {
            self.display_commits = self.all_commits.clone();
            self.display_graph = self.all_graph.clone();
        } else {
            self.display_commits = self
                .all_commits
                .iter()
                .filter(|c| {
                    c.message.to_lowercase().contains(&query)
                        || c.author.to_lowercase().contains(&query)
                        || c.short_id.to_lowercase().contains(&query)
                        || c.oid.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();

            let graph_nodes = self
                .display_commits
                .iter()
                .map(|c| CommitNode {
                    id: c.oid.clone(),
                    parents: c.parents.clone(),
                })
                .collect::<Vec<_>>();
            self.display_graph = render_graph(&graph_nodes);
        }
        self.commit_index = 0;
        self.file_index = 0;
        self.sync_scroll();
        let _ = self.load_selected_files();
    }

    fn refresh_history(&mut self) -> Result<()> {
        let Some(ref_entry) = self.data.refs.get(self.ref_index) else {
            self.all_commits.clear();
            self.all_graph.clear();
            self.display_commits.clear();
            self.display_graph.clear();
            return Ok(());
        };

        if ref_entry.tip_oid.is_empty() {
            self.all_commits.clear();
            self.all_graph.clear();
            self.display_commits.clear();
            self.display_graph.clear();
            return Ok(());
        }

        let (commits, graph) =
            RepoData::load_history(&self.repo_path, &ref_entry.tip_oid, 500)?;
        self.all_commits = commits;
        self.all_graph = graph;
        self.apply_search_filter();
        Ok(())
    }

    fn load_selected_files(&mut self) -> Result<()> {
        if self.files_mode == FilesMode::Working {
            let entries = RepoData::load_working_tree(&self.repo_path)?;
            self.files = entries;
            return Ok(());
        }

        let oid = self
            .display_commits
            .get(self.commit_index)
            .map(|c| c.oid.clone());

        let mode_key = match self.files_mode {
            FilesMode::All => "all",
            FilesMode::Changed => "changed",
            FilesMode::Working => "working",
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
                FilesMode::Working => unreachable!(),
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

    pub fn refs(&self) -> &[RefEntry] {
        &self.data.refs
    }

    pub fn ref_index(&self) -> usize {
        self.ref_index
    }

    pub fn selected_ref(&self) -> Option<&RefEntry> {
        self.data.refs.get(self.ref_index)
    }

    pub fn work_tree_summary(&self) -> &str {
        &self.data.work_tree_summary
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
        if let Some(buf) = &self.search_input {
            return format!("Search: {buf}_ | Enter: apply | Esc: cancel");
        }

        if let Some(msg) = self.take_flash() {
            return format!("{msg} | ? help | q: quit");
        }

        if self.show_help {
            return "? Help open | Esc/? close | q: quit".to_string();
        }

        if self.diff_view.is_some() {
            return "Diff | j/k: scroll | Esc: close | q: quit".to_string();
        }

        let ref_name = self.selected_ref().map(|r| r.name.as_str()).unwrap_or("-");
        let commit = self.selected_commit();
        let author = commit.map(|c| c.author.as_str()).unwrap_or("-");
        let date = commit.map(|c| c.date.as_str()).unwrap_or("-");
        let id = commit.map(|c| c.short_id.as_str()).unwrap_or("-");
        let files_mode = match self.files_mode {
            FilesMode::All => "all files",
            FilesMode::Changed => "changed",
            FilesMode::Working => "working tree",
        };
        let search = if self.search_query.is_empty() {
            String::new()
        } else {
            format!(" | filter: {}", self.search_query)
        };

        format!(
            "repov | {} | {ref_name} | {id} | {author} | {date} | {files_mode} | wt: {}{search} | / f ? help",
            self.data.repo_name,
            self.data.work_tree_summary
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
