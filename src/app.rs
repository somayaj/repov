use anyhow::Result;
use std::collections::HashMap;

use crate::repo::{BranchInfo, CommitInfo, RepoData, TreeEntry};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Refs,
    History,
    Files,
}

pub struct App {
    repo_path: String,
    data: RepoData,
    panel: Panel,
    branch_index: usize,
    commit_index: usize,
    file_index: usize,
    files: Vec<TreeEntry>,
    files_cache: HashMap<String, Vec<TreeEntry>>,
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
            files: Vec::new(),
            files_cache: HashMap::new(),
        };
        app.load_selected_files()?;
        Ok(app)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.data = RepoData::load(&self.repo_path)?;
        self.branch_index = self.data.head_branch_index();
        self.commit_index = 0;
        self.file_index = 0;
        self.files_cache.clear();
        self.load_selected_files()?;
        Ok(())
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
        match self.panel {
            Panel::Refs => {
                if self.branch_index + 1 < self.data.branches.len() {
                    self.branch_index += 1;
                }
            }
            Panel::History => {
                if self.commit_index + 1 < self.data.commits.len() {
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
        match self.panel {
            Panel::Refs => {
                if self.branch_index > 0 {
                    self.branch_index -= 1;
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

    fn load_selected_files(&mut self) -> Result<()> {
        let oid = self
            .data
            .commits
            .get(self.commit_index)
            .map(|c| c.oid.clone());

        if let Some(oid) = oid {
            if let Some(cached) = self.files_cache.get(&oid) {
                self.files = cached.clone();
                return Ok(());
            }

            let entries = RepoData::load_files_for_commit(&oid, &self.repo_path)?;
            self.files_cache.insert(oid, entries.clone());
            self.files = entries;
        } else {
            self.files.clear();
        }

        Ok(())
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn branches(&self) -> &[BranchInfo] {
        &self.data.branches
    }

    pub fn branch_index(&self) -> usize {
        self.branch_index
    }

    pub fn commits(&self) -> &[CommitInfo] {
        &self.data.commits
    }

    pub fn commit_index(&self) -> usize {
        self.commit_index
    }

    pub fn graph_line(&self, index: usize) -> &str {
        self.data
            .graph
            .get(index)
            .map(|g| g.symbols.as_str())
            .unwrap_or(" ")
    }

    pub fn selected_commit(&self) -> Option<&CommitInfo> {
        self.data.commits.get(self.commit_index)
    }

    pub fn selected_files(&self) -> &[TreeEntry] {
        &self.files
    }

    pub fn file_index(&self) -> usize {
        self.file_index
    }

    pub fn status_line(&self) -> String {
        let commit = self.selected_commit();
        let author = commit.map(|c| c.author.as_str()).unwrap_or("-");
        let date = commit.map(|c| c.date.as_str()).unwrap_or("-");
        let id = commit.map(|c| c.short_id.as_str()).unwrap_or("-");

        format!(
            "repov | {} | {} | {} | {} | Tab: panel | j/k: move | r: refresh | q: quit",
            self.data.repo_name,
            id,
            author,
            date
        )
    }
}
