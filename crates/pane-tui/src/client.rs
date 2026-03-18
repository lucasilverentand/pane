use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::event::AppEvent;
use pane_protocol::app::{LeaderState, Mode, ResizeBorder, ResizeState};
use crate::clipboard;
use pane_protocol::config::{self, Action, Config, HubWidget};
use pane_protocol::window_types::{TabKind, WindowId};
use crate::copy_mode::{CopyModeAction, CopyModeState};
use pane_protocol::layout::{Side, SplitDirection, TabId};
use pane_daemon::server::daemon;
use pane_protocol::framing;
use pane_protocol::protocol::{
    ClientRequest, RenderState, SerializableKeyEvent, ServerResponse, WorkspaceSnapshot,
};
use pane_protocol::system_stats::SystemStats;
use crate::tui::Tui;
use crate::ui;
use crate::ui::context_menu::ContextMenuState;
use crate::ui::palette::UnifiedPaletteState;
use crate::ui::tab_picker::{TabPickerEntry, TabPickerState};
use crate::ui::widget_picker::{WidgetPickerMode, WidgetPickerState};

/// Result of hit-testing a tab bar click.
enum TabBarHit {
    /// Clicked on a specific tab within a window.
    Tab { group_index: usize, tab_index: usize },
    /// Clicked on the + button.
    Plus,
}

/// TUI client that connects to a pane daemon via Unix socket.
pub struct Client {
    // Local rendering state (received from server)
    pub mode: Mode,
    pub render_state: RenderState,
    pub screens: HashMap<TabId, vt100::Parser>,
    pub system_stats: SystemStats,
    pub config: Config,
    pub client_count: u32,
    pub plugin_segments: Vec<Vec<pane_protocol::plugin::PluginSegment>>,

    // Client-only UI state
    pub leader_state: Option<LeaderState>,
    pub palette_state: Option<UnifiedPaletteState>,
    pub copy_mode_state: Option<CopyModeState>,
    pub system_programs: Vec<TabPickerEntry>,
    pub favorites: HashSet<String>,
    pub tab_picker_state: Option<TabPickerState>,
    pub context_menu_state: Option<ContextMenuState>,
    pub pending_confirm_action: Option<Action>,
    pub confirm_message: Option<String>,
    pub resize_state: Option<ResizeState>,
    pub focus_location: FocusLocation,
    focus_stack: Vec<FocusLocation>,
    pub hover: Option<(u16, u16)>,
    pub should_quit: bool,
    pub needs_redraw: bool,
    pub rename_input: String,
    pub rename_target: RenameTarget,
    pub new_workspace_input: Option<NewWorkspaceInputState>,
    pub project_hub_state: Option<ProjectHubState>,
    /// User-set sidebar width for the home workspace (None = auto).
    pub sidebar_width: Option<u16>,
    /// Active sidebar border drag state.
    sidebar_drag: Option<SidebarDragState>,
    /// Active widget split drag state (home workspace only).
    home_widget_drag: Option<HomeWidgetDragState>,
    /// State for the widget picker overlay.
    pub widget_picker_state: Option<WidgetPickerState>,
    /// Channel for sending async events (e.g. git info ready) back to the event loop.
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<ServerEvent>>,
}

struct SidebarDragState {
    /// The body rect at the time the drag started.
    body: Rect,
}

struct HomeWidgetDragState {
    split_path: Vec<Side>,
    direction: SplitDirection,
    layout_body: Rect,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum FocusLocation {
    #[default]
    WindowLayout,
    WorkspaceBar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameTarget {
    Window,
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewWorkspaceStage {
    /// Stage 1: pick a directory
    Directory,
    /// Stage 2: optionally name the workspace
    Name,
}

pub struct NewWorkspaceInputState {
    pub stage: NewWorkspaceStage,
    pub name: String,
    pub browser: DirBrowser,
    /// Track last click for double-click detection: (item index, timestamp).
    last_click: Option<(usize, std::time::Instant)>,
}

pub struct DirBrowser {
    pub current_dir: std::path::PathBuf,
    /// All directory entries in current_dir.
    all_entries: Vec<DirEntry>,
    /// Filtered indices into all_entries.
    pub filtered: Vec<usize>,
    /// Text typed by the user to filter entries.
    pub input: String,
    pub selected: usize,
    pub scroll_offset: usize,
    /// Whether zoxide is available on the system.
    pub has_zoxide: bool,
    /// Zoxide search mode (toggled with Ctrl+F or /).
    pub search_mode: bool,
    /// Zoxide query results (absolute paths, ranked by frecency).
    pub zoxide_results: Vec<String>,
}

pub struct DirEntry {
    pub name: String,
}

impl DirBrowser {
    pub fn new(path: std::path::PathBuf) -> Self {
        let has_zoxide = std::process::Command::new("sh")
            .args(["-c", "command -v zoxide >/dev/null 2>&1"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let mut browser = Self {
            current_dir: path,
            all_entries: Vec::new(),
            filtered: Vec::new(),
            input: String::new(),
            selected: 0,
            scroll_offset: 0,
            has_zoxide,
            search_mode: false,
            zoxide_results: Vec::new(),
        };
        browser.refresh();
        browser
    }

    pub fn refresh(&mut self) {
        self.all_entries.clear();
        if let Ok(read_dir) = std::fs::read_dir(&self.current_dir) {
            let mut dirs: Vec<DirEntry> = read_dir
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                })
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    DirEntry { name }
                })
                .collect();
            dirs.sort_by(|a, b| {
                let a_hidden = a.name.starts_with('.');
                let b_hidden = b.name.starts_with('.');
                a_hidden.cmp(&b_hidden).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            self.all_entries = dirs;
        }
        self.input.clear();
        self.search_mode = false;
        self.update_filter();
    }

    pub fn update_filter(&mut self) {
        if self.search_mode {
            // Zoxide search mode: only zoxide results, no local filtering
            self.filtered.clear();
            if self.input.is_empty() {
                self.zoxide_results.clear();
            } else {
                self.zoxide_results = query_zoxide(&self.input);
            }
        } else {
            // Browse mode: filter local dirs, no zoxide
            self.zoxide_results.clear();
            if self.input.is_empty() {
                self.filtered = (0..self.all_entries.len()).collect();
            } else {
                let query = self.input.to_lowercase();
                self.filtered = self
                    .all_entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.name.to_lowercase().contains(&query))
                    .map(|(i, _)| i)
                    .collect();
            }
        }
        let total = self.total_count();
        if self.selected >= total {
            self.selected = total.saturating_sub(1);
        }
        self.scroll_offset = 0;
    }

    /// Toggle between browse mode and zoxide search mode.
    pub fn toggle_search(&mut self) {
        if !self.has_zoxide {
            return;
        }
        self.search_mode = !self.search_mode;
        self.input.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.update_filter();
    }

    /// Total number of selectable items.
    pub fn total_count(&self) -> usize {
        if self.search_mode {
            self.zoxide_results.len()
        } else {
            self.filtered.len()
        }
    }

    /// Whether the current selection is a zoxide result.
    pub fn selected_is_zoxide(&self) -> bool {
        self.search_mode && self.selected < self.zoxide_results.len()
    }

    /// Get the zoxide result path for the current selection (if applicable).
    pub fn selected_zoxide_path(&self) -> Option<&str> {
        if self.selected_is_zoxide() {
            self.zoxide_results.get(self.selected).map(|s| s.as_str())
        } else {
            None
        }
    }

    /// Get the local dir entry index for the current selection (if applicable).
    pub fn selected_dir_index(&self) -> Option<usize> {
        if self.search_mode {
            None
        } else {
            self.filtered.get(self.selected).copied()
        }
    }

    /// Get the visible (filtered) entries.
    pub fn visible_entries(&self) -> Vec<&DirEntry> {
        self.filtered.iter().map(|&i| &self.all_entries[i]).collect()
    }

    /// Enter the selected filtered entry or zoxide result.
    pub fn enter_selected(&mut self) {
        if let Some(zpath) = self.selected_zoxide_path().map(|s| s.to_string()) {
            // Selected a zoxide result — jump to that directory
            let path = std::path::PathBuf::from(&zpath);
            if path.is_dir() {
                self.current_dir = path;
                self.refresh();
            }
        } else if let Some(idx) = self.selected_dir_index() {
            let entry = &self.all_entries[idx];
            let new_path = self.current_dir.join(&entry.name);
            if new_path.is_dir() {
                self.current_dir = new_path;
                self.refresh();
            }
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            let old_name = self.current_dir.file_name()
                .map(|n| n.to_string_lossy().to_string());
            self.current_dir = parent.to_path_buf();
            self.refresh();
            // Try to select the directory we came from
            if let Some(name) = old_name {
                if let Some(pos) = self.filtered.iter().position(|&i| self.all_entries[i].name == name) {
                    self.selected = pos;
                    self.clamp_scroll(14);
                }
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.clamp_scroll(14);
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.total_count() {
            self.selected += 1;
            self.clamp_scroll(14);
        }
    }

    pub fn clamp_scroll(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.selected >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected + 1 - visible_height;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    pub fn display_path(&self) -> String {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = self.current_dir.to_string_lossy();
        if !home.is_empty() && path.starts_with(&home) {
            format!("~{}", &path[home.len()..])
        } else {
            path.to_string()
        }
    }

    /// Display path including the currently selected entry (for preview).
    pub fn display_path_with_selected(&self) -> String {
        if let Some(zpath) = self.selected_zoxide_path() {
            let home = std::env::var("HOME").unwrap_or_default();
            if !home.is_empty() && zpath.starts_with(&home) {
                return format!("~{}", &zpath[home.len()..]);
            }
            return zpath.to_string();
        }
        let base = self.display_path();
        if let Some(idx) = self.selected_dir_index() {
            let name = &self.all_entries[idx].name;
            if base.ends_with('/') {
                format!("{}{}", base, name)
            } else {
                format!("{}/{}", base, name)
            }
        } else {
            base
        }
    }
}

/// A project entry discovered in a project directory.
#[derive(Clone, Debug)]
pub struct ProjectEntry {
    /// Display name (directory name).
    pub name: String,
    /// Full path to the project.
    pub path: std::path::PathBuf,
}

/// Cached git info for a project.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectGitInfo {
    pub branch: String,
    pub commits: Vec<GitCommit>,
    pub status_lines: Vec<String>,
    pub dirty_count: usize,
    pub staged_count: usize,
    pub untracked_count: usize,
    pub ahead: usize,
    pub behind: usize,
    /// Git remote origin URL (e.g. "git@github.com:user/repo.git" or "https://github.com/user/repo.git").
    pub remote_url: Option<String>,
    // Unit 1: Branches, Stashes, Tags
    pub branches: Vec<BranchInfo>,
    pub stashes: Vec<StashInfo>,
    pub tags: Vec<TagInfo>,
    // Unit 2: Git Graph, Contributors
    pub graph_lines: Vec<String>,
    pub contributors: Vec<ContributorInfo>,
    // Unit 3: Todos, Readme
    pub todos: Vec<TodoItem>,
    pub readme_lines: Vec<String>,
    // Unit 4: Languages, Disk Usage
    pub languages: Vec<LanguageInfo>,
    pub disk_usage: Option<DiskUsageInfo>,
    // Unit 5: CI Status, Open Issues
    pub ci_runs: Vec<CiRun>,
    pub gh_issues: Vec<GhIssue>,
    pub gh_available: bool,
    // Unit 6: Running Processes
    pub processes: Vec<ProcessInfo>,
}

impl ProjectGitInfo {
    /// Derive GitHub web URL from the remote origin URL.
    /// Supports both SSH (git@github.com:user/repo.git) and HTTPS formats.
    #[allow(dead_code)]
    pub fn github_url(&self) -> Option<String> {
        let url = self.remote_url.as_deref()?;
        // SSH: git@github.com:user/repo.git
        if let Some(rest) = url.strip_prefix("git@github.com:") {
            let repo = rest.strip_suffix(".git").unwrap_or(rest);
            return Some(format!("https://github.com/{}", repo));
        }
        // HTTPS: https://github.com/user/repo.git
        if url.starts_with("https://github.com/") {
            let repo = url.strip_suffix(".git").unwrap_or(url);
            return Some(repo.to_string());
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitCommit {
    pub hash: String,
    pub message: String,
    pub author: String,
    pub age: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub last_commit_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StashInfo {
    pub id: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    pub date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContributorInfo {
    pub name: String,
    pub email: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TodoItem {
    pub file: String,
    pub line_num: String,
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LanguageInfo {
    pub extension: String,
    pub file_count: usize,
    pub percentage: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiskUsageInfo {
    pub total: String,
    pub git_size: String,
    pub build_size: Option<String>,
    pub build_dir_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CiRun {
    pub name: String,
    pub status: String,
    pub conclusion: String,
    pub branch: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhIssue {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: String,
    pub cpu: String,
    pub command: String,
}

/// Cache entry state for a project's git info.
pub enum GitCacheEntry {
    /// First load, no data yet.
    Loading,
    /// Has data (possibly stale or partial), background refresh in progress.
    Refreshing(ProjectGitInfo),
    /// Fully loaded and up-to-date.
    Ready(ProjectGitInfo),
}

impl GitCacheEntry {
    pub fn info(&self) -> Option<&ProjectGitInfo> {
        match self {
            GitCacheEntry::Loading => None,
            GitCacheEntry::Refreshing(info) | GitCacheEntry::Ready(info) => Some(info),
        }
    }

    #[allow(dead_code)]
    pub fn is_refreshing(&self) -> bool {
        matches!(self, GitCacheEntry::Loading | GitCacheEntry::Refreshing(_))
    }
}

pub struct ProjectHubState {
    /// All discovered projects.
    pub all_projects: Vec<ProjectEntry>,
    /// Filtered indices into all_projects.
    pub filtered: Vec<usize>,
    /// User's search query.
    pub input: String,
    /// Currently selected index.
    pub selected: usize,
    /// Scroll offset.
    pub scroll_offset: usize,
    /// Cached git info keyed by project index.
    pub git_cache: HashMap<usize, GitCacheEntry>,
    /// Which project index we last fetched git info for.
    pub last_git_fetch: Option<usize>,
    /// Track last click for double-click detection: (selected index, timestamp).
    pub last_click: Option<(usize, std::time::Instant)>,
    /// Which widget window is currently focused (for j/k item navigation).
    pub focused_widget: Option<WindowId>,
    /// Per-widget interaction state (selected item index).
    pub widget_interact: HashMap<WindowId, WidgetInteractState>,
    /// Channel for sending async results back to the event loop.
    event_tx: tokio::sync::mpsc::UnboundedSender<ServerEvent>,
}

impl ProjectHubState {
    fn new(config: &config::Config, event_tx: tokio::sync::mpsc::UnboundedSender<ServerEvent>) -> Self {
        let dirs = config.behavior.resolved_projects_dirs();

        let mut projects = Vec::new();
        for dir in &dirs {
            if let Ok(read_dir) = std::fs::read_dir(dir) {
                for entry in read_dir.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if !path.is_dir() {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    // Check if it's a git repo
                    if !path.join(".git").exists() {
                        continue;
                    }
                    projects.push(ProjectEntry {
                        name,
                        path,
                    });
                }
            }
        }

        projects.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
        });

        let filtered: Vec<usize> = (0..projects.len()).collect();
        let mut state = ProjectHubState {
            all_projects: projects,
            filtered,
            input: String::new(),
            selected: 0,
            scroll_offset: 0,
            git_cache: HashMap::new(),
            last_git_fetch: None,
            last_click: None,
            focused_widget: None,
            widget_interact: HashMap::new(),
            event_tx,
        };
        state.ensure_git_info();
        state
    }

    pub fn update_filter(&mut self) {
        if self.input.is_empty() {
            self.filtered = (0..self.all_projects.len()).collect();
        } else {
            let query = self.input.to_lowercase();
            self.filtered = self
                .all_projects
                .iter()
                .enumerate()
                .filter(|(_, p)| p.name.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect();
        }
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
        self.scroll_offset = 0;
        // Reset last_git_fetch so ensure_git_info re-evaluates
        let new_project_idx = self.filtered.get(self.selected).copied();
        if new_project_idx != self.last_git_fetch {
            self.last_git_fetch = None;
        }
        self.ensure_git_info();
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.ensure_git_info();
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
            self.ensure_git_info();
        }
    }

    /// Select a specific filtered index.
    pub fn select(&mut self, idx: usize) {
        if idx < self.filtered.len() {
            self.selected = idx;
            self.ensure_git_info();
        }
    }

    pub fn selected_project(&self) -> Option<&ProjectEntry> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.all_projects.get(i))
    }

    /// Get git info for the currently selected project (if any data is available).
    /// Returns data for both `Refreshing` and `Ready` states.
    pub fn selected_git_info(&self) -> Option<&ProjectGitInfo> {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.git_cache.get(&i))
            .and_then(|entry| entry.info())
    }

    /// Returns true if the selected project has no data yet (first load).
    pub fn is_loading_git_info(&self) -> bool {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.git_cache.get(&i))
            .is_some_and(|entry| matches!(entry, GitCacheEntry::Loading))
    }

    /// Returns true if the selected project's data is being refreshed
    /// (stale cache shown, or fast phase shown while slow phase loads).
    #[allow(dead_code)]
    pub fn is_refreshing_git_info(&self) -> bool {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.git_cache.get(&i))
            .is_some_and(|entry| entry.is_refreshing())
    }

    /// Kick off an async fetch of git info for the selected project if not already cached.
    pub fn ensure_git_info(&mut self) {
        let project_idx = match self.filtered.get(self.selected) {
            Some(&i) => i,
            None => return,
        };
        if self.last_git_fetch == Some(project_idx) {
            return;
        }
        self.last_git_fetch = Some(project_idx);
        if self.git_cache.contains_key(&project_idx) {
            return;
        }

        let path = self.all_projects[project_idx].path.clone();
        let tx = self.event_tx.clone();

        // Check disk cache — returns data + freshness
        if let Some((info, fresh)) = load_disk_cache(&path) {
            if fresh {
                self.git_cache.insert(project_idx, GitCacheEntry::Ready(info));
                return;
            }
            // Stale cache: show old data immediately, refresh in background
            self.git_cache.insert(project_idx, GitCacheEntry::Refreshing(info));
            tokio::task::spawn_blocking(move || {
                let full = fetch_git_info(&path);
                save_disk_cache(&path, &full);
                let _ = tx.send(ServerEvent::GitInfoReady {
                    project_idx,
                    info: Box::new(full),
                    refreshing: false,
                });
            });
            return;
        }

        // No disk cache: progressive load (fast phase → slow phase)
        self.git_cache.insert(project_idx, GitCacheEntry::Loading);
        tokio::task::spawn_blocking(move || {
            let fast = fetch_git_info_fast(&path);
            let _ = tx.send(ServerEvent::GitInfoReady {
                project_idx,
                info: Box::new(fast.clone()),
                refreshing: true,
            });
            let full = fetch_git_info_slow(&path, fast);
            save_disk_cache(&path, &full);
            let _ = tx.send(ServerEvent::GitInfoReady {
                project_idx,
                info: Box::new(full),
                refreshing: false,
            });
        });
    }
}

/// Per-widget interaction state: tracks which list item is selected for keyboard navigation.
#[derive(Default, Clone)]
pub struct WidgetInteractState {
    pub selected: usize,
}

/// Disk cache TTL for project git info (seconds).
const GIT_CACHE_TTL_SECS: u64 = 30;

#[derive(Serialize, Deserialize)]
struct CachedGitInfo {
    cached_at: u64,
    info: ProjectGitInfo,
}

fn git_cache_path(project_path: &std::path::Path) -> std::path::PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    project_path.hash(&mut hasher);
    let hash = hasher.finish();
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("pane")
        .join("hub");
    cache_dir.join(format!("{:016x}.json", hash))
}

/// Returns `Some((info, is_fresh))` if a disk cache entry exists.
/// `is_fresh` is true if the entry is within the TTL.
fn load_disk_cache(project_path: &std::path::Path) -> Option<(ProjectGitInfo, bool)> {
    let path = git_cache_path(project_path);
    let content = std::fs::read_to_string(&path).ok()?;
    let cached: CachedGitInfo = serde_json::from_str(&content).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let fresh = now - cached.cached_at <= GIT_CACHE_TTL_SECS;
    Some((cached.info, fresh))
}

fn save_disk_cache(project_path: &std::path::Path, info: &ProjectGitInfo) {
    let path = git_cache_path(project_path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cached = CachedGitInfo {
        cached_at: now,
        info: info.clone(),
    };
    if let Ok(json) = serde_json::to_string(&cached) {
        let _ = std::fs::write(&path, json);
    }
}

fn git_run(path: &std::path::Path, args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn git_run_sh(path: &std::path::Path, cmd: &str) -> String {
    std::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(path)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Fast phase: cheap local git operations (~50ms).
/// Returns a partial ProjectGitInfo with slow fields defaulted.
fn fetch_git_info_fast(path: &std::path::Path) -> ProjectGitInfo {
    let branch = git_run(path, &["rev-parse", "--abbrev-ref", "HEAD"]);

    // Remote URL (fast — reads local config)
    let remote_raw = git_run(path, &["remote", "get-url", "origin"]);
    let remote_url = if remote_raw.is_empty() { None } else { Some(remote_raw) };

    let log_output = git_run(path, &[
        "log", "--oneline", "--format=%h\t%s\t%an\t%ar", "-15",
    ]);
    let commits: Vec<GitCommit> = log_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(4, '\t').collect();
            GitCommit {
                hash: parts.first().unwrap_or(&"").to_string(),
                message: parts.get(1).unwrap_or(&"").to_string(),
                author: parts.get(2).unwrap_or(&"").to_string(),
                age: parts.get(3).unwrap_or(&"").to_string(),
            }
        })
        .collect();

    let status_output = git_run(path, &["status", "--porcelain=v1"]);
    let status_lines: Vec<String> = status_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    let mut dirty_count = 0;
    let mut staged_count = 0;
    let mut untracked_count = 0;
    for line in &status_lines {
        let bytes = line.as_bytes();
        if bytes.len() >= 2 {
            let x = bytes[0];
            let y = bytes[1];
            if x == b'?' {
                untracked_count += 1;
            } else {
                if x != b' ' && x != b'?' {
                    staged_count += 1;
                }
                if y != b' ' && y != b'?' {
                    dirty_count += 1;
                }
            }
        }
    }

    let ab_output = git_run(path, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]);
    let (ahead, behind) = if let Some((a, b)) = ab_output.split_once('\t') {
        (
            a.trim().parse::<usize>().unwrap_or(0),
            b.trim().parse::<usize>().unwrap_or(0),
        )
    } else {
        (0, 0)
    };

    let branch_output = git_run(path, &[
        "branch",
        "--format=%(HEAD)\t%(refname:short)\t%(committerdate:relative)",
    ]);
    let branches: Vec<BranchInfo> = branch_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            BranchInfo {
                is_current: parts.first().map(|s| s.trim() == "*").unwrap_or(false),
                name: parts.get(1).unwrap_or(&"").to_string(),
                last_commit_date: parts.get(2).unwrap_or(&"").to_string(),
            }
        })
        .collect();

    let stash_output = git_run(path, &["stash", "list", "--format=%gd\t%gs"]);
    let stashes: Vec<StashInfo> = stash_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (id, message) = line.split_once('\t').unwrap_or((line, ""));
            StashInfo {
                id: id.to_string(),
                message: message.to_string(),
            }
        })
        .collect();

    let readme_lines: Vec<String> = {
        let readme_path = path.join("README.md");
        if readme_path.exists() {
            std::fs::read_to_string(&readme_path)
                .unwrap_or_default()
                .lines()
                .take(50)
                .map(|l| l.to_string())
                .collect()
        } else {
            Vec::new()
        }
    };

    ProjectGitInfo {
        branch,
        commits,
        status_lines,
        dirty_count,
        staged_count,
        untracked_count,
        ahead,
        behind,
        branches,
        stashes,
        remote_url,
        readme_lines,
        ..Default::default()
    }
}

/// Slow phase: expensive operations (tree traversal, network, grep).
/// Takes the fast result and fills in the remaining fields.
fn fetch_git_info_slow(path: &std::path::Path, fast: ProjectGitInfo) -> ProjectGitInfo {
    let tag_output = git_run_sh(path, "git tag -l --sort=-creatordate --format='%(refname:short)\t%(creatordate:relative)' | head -20");
    let tags: Vec<TagInfo> = tag_output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let (name, date) = line.split_once('\t').unwrap_or((line, ""));
            TagInfo {
                name: name.to_string(),
                date: date.to_string(),
            }
        })
        .collect();

    let graph_output = git_run(path, &[
        "log", "--oneline", "--graph", "--all", "-20", "--color=never",
    ]);
    let graph_lines: Vec<String> = graph_output
        .lines()
        .map(|l| l.to_string())
        .collect();

    let contrib_output = git_run_sh(path, "git shortlog -sne --all | head -20");
    let contributors: Vec<ContributorInfo> = contrib_output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let line = line.trim();
            let (count_str, rest) = line.split_once('\t')?;
            let count = count_str.trim().parse::<usize>().ok()?;
            let (name, email) = if let Some(start) = rest.find('<') {
                let name = rest[..start].trim().to_string();
                let email = rest[start..].trim_matches(|c| c == '<' || c == '>').to_string();
                (name, email)
            } else {
                (rest.trim().to_string(), String::new())
            };
            Some(ContributorInfo { name, email, count })
        })
        .collect();

    let todo_output = git_run_sh(path,
        "grep -rn 'TODO\\|FIXME\\|HACK' \
         --include='*.rs' --include='*.ts' --include='*.js' \
         --include='*.py' --include='*.go' --include='*.swift' \
         --max-count=50 2>/dev/null | head -50"
    );
    let todos: Vec<TodoItem> = todo_output
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, ':');
            let file = parts.next()?.to_string();
            let line_num = parts.next()?.to_string();
            let text = parts.next().unwrap_or("").to_string();
            let kind = if text.contains("FIXME") {
                "FIXME"
            } else if text.contains("HACK") {
                "HACK"
            } else {
                "TODO"
            }
            .to_string();
            let text = text.trim().to_string();
            Some(TodoItem {
                file,
                line_num,
                kind,
                text,
            })
        })
        .collect();

    let ls_files = git_run(path, &["ls-files"]);
    let languages: Vec<LanguageInfo> = {
        let mut ext_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut total_files = 0usize;
        for file in ls_files.lines().filter(|l| !l.is_empty()) {
            total_files += 1;
            let ext = std::path::Path::new(file)
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "(none)".to_string());
            *ext_counts.entry(ext).or_insert(0) += 1;
        }
        let mut langs: Vec<LanguageInfo> = ext_counts
            .into_iter()
            .map(|(extension, file_count)| {
                let percentage = if total_files > 0 {
                    (file_count as f32 / total_files as f32) * 100.0
                } else {
                    0.0
                };
                LanguageInfo {
                    extension,
                    file_count,
                    percentage,
                }
            })
            .collect();
        langs.sort_by(|a, b| b.file_count.cmp(&a.file_count));
        langs.truncate(15);
        langs
    };

    let disk_usage: Option<DiskUsageInfo> = {
        let du_total = git_run_sh(path, "du -sh . 2>/dev/null | cut -f1");
        let du_git = git_run_sh(path, "du -sh .git 2>/dev/null | cut -f1");
        let (build_size, build_dir_name) = if path.join("target").exists() {
            (
                Some(git_run_sh(path, "du -sh target 2>/dev/null | cut -f1")),
                Some("target".to_string()),
            )
        } else if path.join("node_modules").exists() {
            (
                Some(git_run_sh(path, "du -sh node_modules 2>/dev/null | cut -f1")),
                Some("node_modules".to_string()),
            )
        } else if path.join("build").exists() {
            (
                Some(git_run_sh(path, "du -sh build 2>/dev/null | cut -f1")),
                Some("build".to_string()),
            )
        } else {
            (None, None)
        };
        if !du_total.is_empty() {
            Some(DiskUsageInfo {
                total: du_total,
                git_size: du_git,
                build_size,
                build_dir_name,
            })
        } else {
            None
        }
    };

    let gh_available = std::process::Command::new("sh")
        .args(["-c", "command -v gh >/dev/null 2>&1"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let (ci_runs, gh_issues) = if gh_available {
        let ci_json = git_run_sh(path,
            "gh run list --limit 10 --json status,name,conclusion,headBranch,createdAt 2>/dev/null"
        );
        let ci_runs: Vec<CiRun> = serde_json::from_str::<Vec<serde_json::Value>>(&ci_json)
            .unwrap_or_default()
            .into_iter()
            .map(|v| CiRun {
                name: v["name"].as_str().unwrap_or("").to_string(),
                status: v["status"].as_str().unwrap_or("").to_string(),
                conclusion: v["conclusion"].as_str().unwrap_or("").to_string(),
                branch: v["headBranch"].as_str().unwrap_or("").to_string(),
                created_at: v["createdAt"].as_str().unwrap_or("").to_string(),
            })
            .collect();

        let issue_json = git_run_sh(path,
            "gh issue list --limit 10 --json number,title,author,labels 2>/dev/null"
        );
        let gh_issues: Vec<GhIssue> = serde_json::from_str::<Vec<serde_json::Value>>(&issue_json)
            .unwrap_or_default()
            .into_iter()
            .map(|v| {
                let author = v["author"]["login"].as_str().unwrap_or("").to_string();
                let labels: Vec<String> = v["labels"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                GhIssue {
                    number: v["number"].as_u64().unwrap_or(0),
                    title: v["title"].as_str().unwrap_or("").to_string(),
                    author,
                    labels,
                }
            })
            .collect();

        (ci_runs, gh_issues)
    } else {
        (Vec::new(), Vec::new())
    };

    let processes: Vec<ProcessInfo> = {
        let path_str = path.to_string_lossy().to_string();
        let ps_output = git_run_sh(path, "ps -eo pid,pcpu,command 2>/dev/null");
        ps_output
            .lines()
            .skip(1)
            .filter(|l| l.contains(&path_str))
            .filter_map(|line| {
                let line = line.trim();
                let mut parts = line.splitn(3, char::is_whitespace);
                let pid = parts.next()?.trim().to_string();
                let rest = parts.next().unwrap_or("").trim_start();
                let mut rest_parts = rest.splitn(2, char::is_whitespace);
                let cpu = rest_parts.next()?.trim().to_string();
                let command = rest_parts.next().unwrap_or("").trim().to_string();
                if command.is_empty() {
                    return None;
                }
                Some(ProcessInfo { pid, cpu, command })
            })
            .collect()
    };

    ProjectGitInfo {
        tags,
        graph_lines,
        contributors,
        todos,
        languages,
        disk_usage,
        ci_runs,
        gh_issues,
        gh_available,
        processes,
        // Carry forward the fast fields
        ..fast
    }
}

/// Full fetch (both phases). Used for background refresh of stale cache.
fn fetch_git_info(path: &std::path::Path) -> ProjectGitInfo {
    let fast = fetch_git_info_fast(path);
    fetch_git_info_slow(path, fast)
}

/// Query zoxide for directories matching the input (up to 10 results).
fn query_zoxide(input: &str) -> Vec<String> {
    let output = std::process::Command::new("zoxide")
        .args(["query", "-l", "--", input])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .take(10)
                .map(|s| s.to_string())
                .collect()
        }
        _ => Vec::new(),
    }
}

impl Client {
    pub fn new(config: Config) -> Self {
        Self {
            mode: Mode::Normal,
            render_state: RenderState {
                workspaces: Vec::new(),
                active_workspace: 0,
            },
            screens: HashMap::new(),
            system_stats: SystemStats::default(),
            config,
            client_count: 1,
            plugin_segments: Vec::new(),

            leader_state: None,
            palette_state: None,
            copy_mode_state: None,
            system_programs: crate::ui::tab_picker::scan_system_programs(),
            favorites: crate::ui::tab_picker::load_favorites(),
            tab_picker_state: None,
            context_menu_state: None,
            pending_confirm_action: None,
            confirm_message: None,
            resize_state: None,
            focus_location: FocusLocation::WindowLayout,
            focus_stack: Vec::new(),
            hover: None,
            should_quit: false,
            needs_redraw: true,
            rename_input: String::new(),
            rename_target: RenameTarget::Window,
            new_workspace_input: None,
            project_hub_state: None,
            sidebar_width: None,
            sidebar_drag: None,
            home_widget_drag: None,
            widget_picker_state: None,
            event_tx: None,
        }
    }

    /// Save current focus before entering a modal.
    fn push_focus(&mut self) {
        self.focus_stack.push(self.focus_location.clone());
        self.focus_location = FocusLocation::WindowLayout;
    }

    /// Restore focus saved by the most recent push_focus().
    fn pop_focus(&mut self) {
        self.focus_location = self.focus_stack.pop().unwrap_or_default();
    }

    /// Convenience for rendering code.
    pub fn is_workspace_bar_focused(&self) -> bool {
        self.focus_location == FocusLocation::WorkspaceBar
    }

    /// Returns true if the active workspace is the home workspace.
    pub fn is_home_active(&self) -> bool {
        self.render_state
            .workspaces
            .get(self.render_state.active_workspace)
            .is_some_and(|ws| ws.is_home)
    }

    /// Connect to a daemon and run the TUI event loop.
    pub async fn run(config: Config) -> Result<()> {
        let sock = daemon::socket_path();
        if !sock.exists() {
            anyhow::bail!("no running daemon. Start one with: pane");
        }

        let mut stream = UnixStream::connect(&sock).await?;

        // Attach with timeout — if the daemon is stuck, don't hang forever
        let handshake = async {
            framing::send(&mut stream, &ClientRequest::Attach).await?;

            let resp: ServerResponse = framing::recv_required(&mut stream).await?;
            match resp {
                ServerResponse::Attached => {}
                ServerResponse::Error(e) => anyhow::bail!("server error: {}", e),
                _ => anyhow::bail!("unexpected response: {:?}", resp),
            };

            let resp: ServerResponse = framing::recv_required(&mut stream).await?;
            Ok::<_, anyhow::Error>(resp)
        };

        let resp = tokio::time::timeout(std::time::Duration::from_secs(5), handshake)
            .await
            .map_err(|_| anyhow::anyhow!("daemon handshake timed out — is the daemon healthy?"))?
            ?;

        let mut client = Client::new(config);

        // Apply initial LayoutChanged
        if let ServerResponse::LayoutChanged { render_state } = resp {
            client.apply_layout(render_state);
        }

        // Set up TUI
        let mut tui = Tui::new()?;
        tui.enter()?;

        // Send initial resize
        let size = tui.size()?;
        framing::send(
            &mut stream,
            &ClientRequest::Resize {
                width: size.width,
                height: size.height,
            },
        )
        .await?;

        // Split stream
        let (read_half, write_half) = stream.into_split();
        let writer = Arc::new(Mutex::new(write_half));

        // Event loop
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();

        // Initialize the project hub (needs event_tx for async git info fetching)
        client.event_tx = Some(event_tx.clone());
        client.project_hub_state = Some(ProjectHubState::new(&client.config, event_tx.clone()));
        // Home workspace is always at index 0 in render_state.workspaces

        // Start terminal event reader — bridge AppEvent → ServerEvent
        let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::event::start_event_loop(app_tx);
        let term_tx = event_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = app_rx.recv().await {
                if term_tx.send(ServerEvent::Terminal(event)).is_err() {
                    break;
                }
            }
        });

        // Spawn server message reader
        let server_tx = event_tx.clone();
        let server_reader = tokio::spawn(async move {
            let mut reader = read_half;
            loop {
                let mut len_buf = [0u8; 4];
                match reader.read_exact(&mut len_buf).await {
                    Ok(_) => {}
                    Err(_) => break,
                }
                let len = u32::from_be_bytes(len_buf);
                if len > 16 * 1024 * 1024 {
                    break;
                }
                let mut buf = vec![0u8; len as usize];
                if reader.read_exact(&mut buf).await.is_err() {
                    break;
                }
                let response: ServerResponse = match serde_json::from_slice(&buf) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if server_tx.send(ServerEvent::Server(response)).is_err() {
                    break;
                }
            }
            // Server disconnected
            let _ = server_tx.send(ServerEvent::Disconnected);
        });

        loop {
            if client.needs_redraw {
                client.needs_redraw = false;
                tui.draw(|frame| ui::render_client(&mut client, frame))?;
            }

            if let Some(event) = event_rx.recv().await {
                client.handle_event(event, &tui, &writer).await?;
            }
            while let Ok(event) = event_rx.try_recv() {
                client.handle_event(event, &tui, &writer).await?;
                if client.should_quit {
                    break;
                }
            }

            if client.should_quit {
                break;
            }
        }

        // Clean up
        server_reader.abort();
        // Try to send Detach, but don't hang if the server is already gone
        let detach = async {
            let mut w = writer.lock().await;
            let _ = send_request(&mut w, &ClientRequest::Detach).await;
        };
        let _ = tokio::time::timeout(std::time::Duration::from_millis(500), detach).await;

        // Restore terminal before printing
        tui.exit();

        // Print summary of what's still running in the daemon
        print_detach_summary(&client.render_state);

        Ok(())
    }

    fn apply_layout(&mut self, render_state: RenderState) {
        // Preserve our own active_workspace across broadcasts from other clients.
        // On first layout (no workspaces yet), accept the server's value.
        let preserved_ws = if self.render_state.workspaces.is_empty() {
            render_state.active_workspace
        } else {
            self.render_state
                .active_workspace
                .min(render_state.workspaces.len().saturating_sub(1))
        };

        // Reconcile screen map: add new panes, remove dead ones
        let mut live_pane_ids: std::collections::HashSet<TabId> = std::collections::HashSet::new();
        for ws in &render_state.workspaces {
            for group in &ws.groups {
                for pane in &group.tabs {
                    live_pane_ids.insert(pane.id);
                    let rows = pane.rows.max(1);
                    let cols = pane.cols.max(1);
                    let parser = self
                        .screens
                        .entry(pane.id)
                        .or_insert_with(|| vt100::Parser::new(rows, cols, 1000));
                    // Resize existing parsers when dimensions change
                    let (cur_rows, cur_cols) = parser.screen().size();
                    if cur_rows != rows || cur_cols != cols {
                        parser.screen_mut().set_size(rows, cols);
                    }
                }
            }
        }
        // Remove screens for panes that no longer exist
        self.screens.retain(|id, _| live_pane_ids.contains(id));
        self.render_state = render_state;
        self.render_state.active_workspace = preserved_ws;

        // Show hub when no workspaces exist
        if self.render_state.workspaces.is_empty() {
            self.render_state.active_workspace = 0;
        }

        // If workspace bar was focused but workspaces are gone, reset focus
        if self.focus_location == FocusLocation::WorkspaceBar
            && self.render_state.workspaces.is_empty()
        {
            self.focus_location = FocusLocation::WindowLayout;
            self.focus_stack.clear();
        }
    }

    async fn handle_event(
        &mut self,
        event: ServerEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match event {
            ServerEvent::Terminal(AppEvent::Tick) => {
                // Redraw on tick for animations (tab picker placeholder, interact label).
                if self.tab_picker_state.is_some() || self.mode == Mode::Interact {
                    self.needs_redraw = true;
                }
            }
            ServerEvent::Terminal(app_event) => {
                self.handle_terminal_event(app_event, tui, writer).await?;
                self.needs_redraw = true;
            }
            ServerEvent::Server(response) => {
                self.handle_server_response(response);
                self.needs_redraw = true;
            }
            ServerEvent::Disconnected => {
                self.should_quit = true;
            }
            ServerEvent::GitInfoReady { project_idx, info, refreshing } => {
                if let Some(ref mut hub) = self.project_hub_state {
                    let entry = if refreshing {
                        GitCacheEntry::Refreshing(*info)
                    } else {
                        GitCacheEntry::Ready(*info)
                    };
                    hub.git_cache.insert(project_idx, entry);
                }
                self.needs_redraw = true;
            }
        }
        Ok(())
    }

    fn handle_server_response(&mut self, response: ServerResponse) {
        match response {
            ServerResponse::PaneOutput { pane_id, data } => {
                if let Some(parser) = self.screens.get_mut(&pane_id) {
                    parser.process(&data);
                }
            }
            ServerResponse::FullScreenDump { pane_id, data } => {
                if let Some(parser) = self.screens.get_mut(&pane_id) {
                    parser.process(&data);
                }
            }
            ServerResponse::LayoutChanged { render_state } => {
                self.apply_layout(render_state);
                self.update_terminal_title();
            }
            ServerResponse::PaneExited { pane_id } => {
                // Mark locally if needed — the server handles cleanup
                let _ = pane_id;
            }
            ServerResponse::StatsUpdate(stats) => {
                self.system_stats = stats.into();
            }
            ServerResponse::SessionEnded => {
                self.should_quit = true;
            }
            ServerResponse::ClientCountChanged(count) => {
                self.client_count = count;
            }
            ServerResponse::PluginSegments(segments) => {
                self.plugin_segments = segments;
            }
            ServerResponse::Error(_)
            | ServerResponse::Attached
            | ServerResponse::CommandOutput { .. } => {}
        }
    }

    async fn handle_terminal_event(
        &mut self,
        event: crate::event::AppEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        use crate::event::AppEvent;
        match event {
            AppEvent::Key(key) => {
                self.handle_key_event(key, tui, writer).await?;
            }
            AppEvent::Resize(w, h) => {
                let mut w_guard = writer.lock().await;
                let _ = send_request(
                    &mut w_guard,
                    &ClientRequest::Resize {
                        width: w,
                        height: h,
                    },
                )
                .await;
            }
            AppEvent::MouseDown { x, y } => {
                if self.mode == Mode::ContextMenu {
                    let size = tui.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    if let Some(ref cm) = self.context_menu_state {
                        if let Some(idx) = crate::ui::context_menu::hit_test(cm, area, x, y) {
                            let action = cm.items[idx].action.clone();
                            self.context_menu_state = None;
                            self.mode = Mode::Normal;
                            self.execute_action(action, tui, writer).await?;
                        } else {
                            // Click outside menu — dismiss
                            self.context_menu_state = None;
                            self.mode = Mode::Normal;
                        }
                    }
                } else if self.mode == Mode::Confirm {
                    let size = tui.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    let msg = self.confirm_message.as_deref().unwrap_or("Are you sure?");
                    if let Some(click) = ui::dialog::confirm_hit_test(area, msg, x, y) {
                        match click {
                            ui::ConfirmDialogClick::Confirm => {
                                if let Some(action) = self.pending_confirm_action.take() {
                                    if let Some(cmd) = action_to_command(&action) {
                                        let mut w = writer.lock().await;
                                        let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                                    }
                                }
                                self.confirm_message = None;
                                self.mode = Mode::Normal;
                            }
                            ui::ConfirmDialogClick::Cancel => {
                                self.pending_confirm_action = None;
                                self.confirm_message = None;
                                self.mode = Mode::Normal;
                            }
                        }
                    }
                } else if self.mode == Mode::TabPicker {
                    let picker_area = {
                        let size = tui.size()?;
                        let full_area = Rect::new(0, 0, size.width, size.height);
                        ui::tab_picker_area(self, full_area)
                    };
                    if crate::ui::tab_picker::is_inside_popup(picker_area, x, y) {
                        // Check if we hit a list item
                        let click = self.tab_picker_state.as_ref()
                            .and_then(|s| crate::ui::tab_picker::hit_test(s, picker_area, x, y));
                        if let Some(crate::ui::tab_picker::TabPickerClick::Item(idx)) = click {
                            let tp = self.tab_picker_state.as_mut().unwrap();
                            tp.selected = idx;
                            let cmd = tp.selected_command();
                            if let Some(cmd) = cmd {
                                let mut w = writer.lock().await;
                                let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                            }
                            self.tab_picker_state = None;
                            self.mode = Mode::Interact;
                        }
                        // Click inside popup but not on an item — keep picker open
                    } else {
                        // Click outside popup — dismiss
                        self.tab_picker_state = None;
                        self.mode = Mode::Normal;
                    }
                } else if self.mode == Mode::NewWorkspaceInput {
                    let size = tui.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    if let Some(ref mut state) = self.new_workspace_input {
                        match state.stage {
                            NewWorkspaceStage::Directory => {
                                if let Some(hit) =
                                    ui::dir_picker_hit_test(&state.browser, area, x, y)
                                {
                                    match hit {
                                        ui::DirPickerClick::Item(idx) => {
                                            let now = std::time::Instant::now();
                                            let is_double = state.last_click
                                                .map(|(prev_idx, prev_time)| {
                                                    prev_idx == idx && now.duration_since(prev_time).as_millis() < 400
                                                })
                                                .unwrap_or(false);
                                            state.browser.selected = idx;
                                            state.browser.clamp_scroll(14);
                                            if is_double {
                                                state.browser.enter_selected();
                                                state.last_click = None;
                                            } else {
                                                state.last_click = Some((idx, now));
                                            }
                                        }
                                        ui::DirPickerClick::Back => {
                                            state.browser.go_up();
                                            state.last_click = None;
                                        }
                                        ui::DirPickerClick::Confirm | ui::DirPickerClick::HintEnter => {
                                            // Enter the highlighted folder first (same as Enter key)
                                            if state.browser.total_count() > 0 {
                                                state.browser.enter_selected();
                                            }
                                            state.name = auto_workspace_name_suggestion(&state.browser.current_dir);
                                            state.stage = NewWorkspaceStage::Name;
                                        }
                                        ui::DirPickerClick::HintOpen => {
                                            state.browser.enter_selected();
                                            state.last_click = None;
                                        }
                                        ui::DirPickerClick::HintSearch => {
                                            state.browser.toggle_search();
                                        }
                                        ui::DirPickerClick::HintEsc => {
                                            if state.browser.search_mode {
                                                state.browser.toggle_search();
                                            } else {
                                                self.new_workspace_input = None;
                                                self.mode = Mode::Normal;
                                            }
                                        }
                                    }
                                } else if !ui::dir_picker_is_inside(area, x, y) {
                                    self.new_workspace_input = None;
                                    self.mode = Mode::Normal;
                                }
                            }
                            NewWorkspaceStage::Name => {
                                if let Some(hit) = ui::name_picker_hit_test(area, x, y) {
                                    match hit {
                                        ui::NamePickerClick::HintEnter => {
                                            let name = state.name.clone();
                                            let dir = state.browser.current_dir.to_string_lossy().to_string();
                                            self.new_workspace_input = None;
                                            self.mode = Mode::Normal;
                                            self.render_state.active_workspace = self.render_state.workspaces.len();
                                            let cmd = format!("new-workspace -c \"{}\"", dir);
                                            let mut w = writer.lock().await;
                                            let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                                            if !name.is_empty() {
                                                let rename_cmd = format!("rename-workspace {}", name);
                                                let _ = send_request(&mut w, &ClientRequest::Command(rename_cmd)).await;
                                            }
                                        }
                                        ui::NamePickerClick::HintEsc => {
                                            state.stage = NewWorkspaceStage::Directory;
                                        }
                                    }
                                } else if !ui::name_picker_is_inside(area, x, y) {
                                    self.new_workspace_input = None;
                                    self.mode = Mode::Normal;
                                }
                            }
                        }
                    }
                } else if self.mode == Mode::Normal
                    || self.mode == Mode::Interact
                {
                    // Check workspace bar clicks (client-side)
                    let show_workspace_bar = self.is_home_active() || !self.render_state.workspaces.is_empty();
                    if show_workspace_bar && y < crate::ui::workspace_bar::HEIGHT {
                        let names: Vec<String> = self.render_state
                            .workspaces
                            .iter()
                            .map(|ws| ws.name.clone())
                            .collect();
                        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                        let active_idx = self.render_state.active_workspace;
                        let bar_area =
                            Rect::new(0, 0, tui.size()?.width, crate::ui::workspace_bar::HEIGHT);
                        if let Some(click) = crate::ui::workspace_bar::hit_test(
                            &name_refs,
                            active_idx,
                            bar_area,
                            x,
                            y,
                        ) {
                            self.focus_location = FocusLocation::WorkspaceBar;
                            match click {
                                crate::ui::workspace_bar::WorkspaceBarClick::Tab(i) => {
                                    // Ensure hub state exists when switching to home
                                    if i == 0 && self.project_hub_state.is_none() {
                                        self.project_hub_state = Some(ProjectHubState::new(&self.config, self.event_tx.as_ref().unwrap().clone()));
                                    }
                                    self.render_state.active_workspace = i;
                                    let mut w = writer.lock().await;
                                    let _ = send_request(
                                        &mut w,
                                        &ClientRequest::Command(format!(
                                            "select-workspace -t {}",
                                            i
                                        )),
                                    )
                                    .await;
                                }
                            }
                            return Ok(());
                        }
                    }

                    // Check status bar clicks (client-side)
                    let size = tui.size()?;
                    let footer_y = size.height.saturating_sub(1);
                    if y == footer_y {
                        let buttons = crate::ui::status_bar::get_buttons(self);
                        let footer_area = Rect::new(0, footer_y, size.width, 1);
                        if let Some(idx) = crate::ui::status_bar::hit_test(buttons, footer_area, x, y) {
                            let (_key, label) = buttons[idx];
                            self.handle_status_bar_click(label, tui, writer).await?;
                            return Ok(());
                        }
                    }

                    // Hub sidebar click handling
                    if self.is_home_active() {
                        let body = crate::ui::body_rect(self, tui.size()?);
                        let sw = crate::ui::project_hub::sidebar_width(body.width, self.sidebar_width);
                        // Check if click is on the sidebar border (right edge ±1)
                        if body.width >= sw + 20 {
                            let border_x = body.x + sw;
                            if x >= border_x.saturating_sub(1) && x <= border_x + 1
                                && y >= body.y && y < body.y + body.height
                            {
                                self.sidebar_drag = Some(SidebarDragState { body });
                                return Ok(());
                            }

                            // Check if click is on a widget split border (layout_body area)
                            let layout_body = Rect::new(
                                body.x + sw,
                                body.y,
                                body.width.saturating_sub(sw),
                                body.height,
                            );
                            if let Some(ws) = self.active_workspace() {
                                if let Some((path, direction)) =
                                    ws.layout.hit_test_split_border(layout_body, x, y)
                                {
                                    self.home_widget_drag = Some(HomeWidgetDragState {
                                        split_path: path,
                                        direction,
                                        layout_body,
                                    });
                                    return Ok(());
                                }
                            }
                        }
                        // Track sidebar interaction (borrow ends before the async call below)
                        let (sidebar_was_clicked, double_click_project) =
                            if let Some(ref mut hub) = self.project_hub_state {
                                if let Some(idx) = crate::ui::project_hub::sidebar_hit_test(
                                    hub, body, x, y, self.sidebar_width,
                                ) {
                                    let now = std::time::Instant::now();
                                    let is_double = hub
                                        .last_click
                                        .map(|(prev_idx, prev_time)| {
                                            prev_idx == idx
                                                && now.duration_since(prev_time).as_millis() < 400
                                        })
                                        .unwrap_or(false);
                                    hub.focused_widget = None;
                                    if is_double {
                                        hub.last_click = None;
                                        (true, hub.selected_project().cloned())
                                    } else {
                                        hub.select(idx);
                                        hub.last_click = Some((idx, now));
                                        (true, None)
                                    }
                                } else {
                                    hub.last_click = None;
                                    (false, None)
                                }
                            } else {
                                (false, None)
                            };

                        if let Some(project) = double_click_project {
                            let dir = project.path.to_string_lossy().to_string();
                            self.open_project(&dir, writer).await?;
                        } else if !sidebar_was_clicked {
                            // Widget click: focus the window that was clicked
                            let sw = crate::ui::project_hub::sidebar_width(
                                body.width,
                                self.sidebar_width,
                            );
                            let layout_body = Rect::new(
                                body.x + sw,
                                body.y,
                                body.width.saturating_sub(sw),
                                body.height,
                            );
                            let clicked_id = self.active_workspace().and_then(|ws| {
                                let resolved =
                                    ws.layout.resolve_with_folds(layout_body, &ws.folded_windows);
                                resolved.into_iter().find_map(|rp| {
                                    if let pane_protocol::layout::ResolvedPane::Visible {
                                        id,
                                        rect,
                                    } = rp
                                    {
                                        if x >= rect.x
                                            && x < rect.x + rect.width
                                            && y >= rect.y
                                            && y < rect.y + rect.height
                                        {
                                            return Some(id);
                                        }
                                    }
                                    None
                                })
                            });
                            if let Some(id) = clicked_id {
                                if let Some(ref mut hub) = self.project_hub_state {
                                    hub.focused_widget = Some(id);
                                }
                            }
                        }
                        self.focus_location = FocusLocation::WindowLayout;
                        return Ok(());
                    }

                    // Check if click hit a tab bar + button (open picker client-side)
                    if self.hit_test_tab_bar_plus(tui, x, y) {
                        self.open_tab_picker(crate::ui::tab_picker::TabPickerMode::NewTab);
                        return Ok(());
                    }

                    // Forward mouse to server (click on body clears workspace bar focus)
                    self.focus_location = FocusLocation::WindowLayout;
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::MouseDown { x, y }).await;
                }
            }
            AppEvent::MouseDrag { x, y } => {
                if let Some(ref drag) = self.sidebar_drag {
                    let new_width = x.saturating_sub(drag.body.x);
                    self.sidebar_width = Some(new_width.clamp(
                        crate::ui::project_hub::SIDEBAR_MIN_WIDTH,
                        crate::ui::project_hub::SIDEBAR_MAX_WIDTH
                            .min(drag.body.width.saturating_sub(20)),
                    ));
                    self.needs_redraw = true;
                } else if let Some(ref drag) = self.home_widget_drag {
                    // Compute new ratio based on cursor position within layout_body
                    let lb = drag.layout_body;
                    let new_ratio = match drag.direction {
                        SplitDirection::Horizontal => {
                            let clamped = x.clamp(lb.x, lb.x + lb.width);
                            (clamped - lb.x) as f64 / lb.width.max(1) as f64
                        }
                        SplitDirection::Vertical => {
                            let clamped = y.clamp(lb.y, lb.y + lb.height);
                            (clamped - lb.y) as f64 / lb.height.max(1) as f64
                        }
                    };
                    let path = drag.split_path.clone();
                    // Update local cached layout for immediate visual feedback
                    let ws_idx = self.render_state.active_workspace;
                    if let Some(ws) = self.render_state.workspaces.get_mut(ws_idx) {
                        ws.layout.set_ratio_at_path(&path, new_ratio);
                    }
                    // Encode path as "0,1" (0=First, 1=Second)
                    let path_str: String = path.iter().map(|s| match s {
                        Side::First => "0",
                        Side::Second => "1",
                    }).collect::<Vec<_>>().join(",");
                    let path_arg = if path_str.is_empty() { "root".to_string() } else { path_str };
                    let cmd = format!("set-split-ratio {} {:.6}", path_arg, new_ratio);
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                    self.needs_redraw = true;
                } else {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::MouseDrag { x, y }).await;
                }
            }
            AppEvent::MouseUp { x, y } => {
                if self.sidebar_drag.take().is_some() {
                    // Sidebar drag finished — width is already set
                } else if self.home_widget_drag.take().is_some() {
                    // Widget split drag finished — ratio already sent to daemon
                } else {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::MouseUp { x, y }).await;
                }
            }
            AppEvent::MouseScroll { up } => {
                if self.is_home_active() {
                    if let Some(ref mut hub) = self.project_hub_state {
                        if up { hub.move_up(); } else { hub.move_down(); }
                    }
                } else if self.mode == Mode::NewWorkspaceInput {
                    if let Some(ref mut state) = self.new_workspace_input {
                        if matches!(state.stage, NewWorkspaceStage::Directory) {
                            if up { state.browser.move_up(); } else { state.browser.move_down(); }
                        }
                    }
                } else if self.mode == Mode::TabPicker {
                    if let Some(ref mut tp) = self.tab_picker_state {
                        if up { tp.move_up(); } else { tp.move_down(); }
                    }
                } else {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::MouseScroll { up }).await;
                }
            }
            AppEvent::MouseMove { x, y } => {
                self.hover = Some((x, y));
                let mut w = writer.lock().await;
                let _ = send_request(&mut w, &ClientRequest::MouseMove { x, y }).await;
            }
            AppEvent::MouseRightDown { x, y } => {
                if self.mode == Mode::Normal || self.mode == Mode::Interact {
                    let show_workspace_bar = self.is_home_active() || !self.render_state.workspaces.is_empty();

                    if show_workspace_bar && y < crate::ui::workspace_bar::HEIGHT {
                        // Right-click on workspace bar — select the clicked workspace first
                        let names: Vec<String> = self.render_state
                            .workspaces
                            .iter()
                            .map(|ws| ws.name.clone())
                            .collect();
                        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                        let active_idx = self.render_state.active_workspace;
                        let bar_area =
                            Rect::new(0, 0, tui.size()?.width, crate::ui::workspace_bar::HEIGHT);
                        if let Some(crate::ui::workspace_bar::WorkspaceBarClick::Tab(i)) = crate::ui::workspace_bar::hit_test(
                            &name_refs,
                            active_idx,
                            bar_area,
                            x,
                            y,
                        ) {
                            self.render_state.active_workspace = i;
                            // Right-click on home — just select, no close menu
                            if self.render_state.workspaces.get(i).is_some_and(|ws| ws.is_home) {
                                return Ok(());
                            }
                            let mut w = writer.lock().await;
                            let _ = send_request(
                                &mut w,
                                &ClientRequest::Command(format!("select-workspace -t {}", i)),
                            )
                            .await;
                        }
                        self.context_menu_state =
                            Some(crate::ui::context_menu::workspace_bar_menu(x, y));
                        self.push_focus();
                        self.mode = Mode::ContextMenu;
                    } else if let Some(TabBarHit::Tab { group_index, tab_index }) = self.hit_test_tab_bar(tui, x, y) {
                        // Right-click on a tab — select that tab first, then show tab bar menu
                        let mut w = writer.lock().await;
                        let _ = send_request(&mut w, &ClientRequest::MouseDown { x, y }).await;
                        drop(w);
                        // Update local render state to reflect the selected tab
                        if let Some(ws) = self.render_state.workspaces.get_mut(self.render_state.active_workspace) {
                            if let Some(group) = ws.groups.get_mut(group_index) {
                                group.active_tab = tab_index;
                            }
                        }
                        self.context_menu_state =
                            Some(crate::ui::context_menu::tab_bar_menu(x, y));
                        self.push_focus();
                        self.mode = Mode::ContextMenu;
                    } else {
                        // Right-click on pane body — use home-workspace menu when appropriate
                        self.context_menu_state = Some(if self.is_home_active() {
                            crate::ui::context_menu::home_body_menu(x, y)
                        } else {
                            crate::ui::context_menu::pane_body_menu(x, y)
                        });
                        self.push_focus();
                        self.mode = Mode::ContextMenu;
                    }
                }
            }
            AppEvent::Tick => {}
            AppEvent::PtyOutput { .. } | AppEvent::PtyExited { .. } | AppEvent::SystemStats(_) | AppEvent::ForegroundPoll => {
                // These come from the server/daemon, not terminal
            }
        }
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        // Modal modes handled client-side
        match &self.mode {
            Mode::Scroll => return self.handle_scroll_key(key, writer).await,
            Mode::Copy => self.handle_copy_mode_key(key),
            Mode::Palette => return self.handle_palette_key(key, tui, writer).await,
            Mode::TabPicker => return self.handle_tab_picker_key(key, writer).await,
            Mode::Confirm => return self.handle_confirm_key(key, writer).await,
            Mode::Leader => return self.handle_leader_key(key, tui, writer).await,
            Mode::Rename => return self.handle_rename_key(key, writer).await,
            Mode::NewWorkspaceInput => return self.handle_new_workspace_key(key, writer).await,
            Mode::ProjectHub => {
                // Legacy: should not be reached, but handle gracefully
                self.mode = Mode::Normal;
                Ok(())
            }
            Mode::ContextMenu => return self.handle_context_menu_key(key, tui, writer).await,
            Mode::WidgetPicker => return self.handle_widget_picker_key(key, writer).await,
            Mode::Resize => return self.handle_resize_key(key, writer).await,
            Mode::Normal => return self.handle_normal_key(key, tui, writer).await,
            Mode::Interact => return self.handle_interact_key(key, tui, writer).await,
        }
    }

    /// Interact mode: forward all keys to PTY except global bindings.
    /// Use Ctrl+Space to exit back to Normal mode.
    async fn handle_interact_key(
        &mut self,
        key: KeyEvent,
        _tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let normalized = config::normalize_key(key);

        // Check global keymap (ctrl+space, shift+pageup, etc.)
        if let Some(action) = self.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, _tui, writer).await;
        }

        // Forward to PTY
        let mut w = writer.lock().await;
        let _ = send_request(
            &mut w,
            &ClientRequest::Key(SerializableKeyEvent::from(key)),
        )
        .await;
        Ok(())
    }

    /// Normal mode: strict vim-style. No PTY fallback.
    async fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let normalized = config::normalize_key(key);

        // Esc clears transient state but stays in Normal mode
        if normalized.code == KeyCode::Esc {
            self.focus_location = FocusLocation::WindowLayout;
            // On home workspace, defocus widget → clear search → switch away
            if self.is_home_active() {
                if let Some(ref mut hub) = self.project_hub_state {
                    if hub.focused_widget.is_some() {
                        hub.focused_widget = None;
                        return Ok(());
                    }
                    if !hub.input.is_empty() {
                        hub.input.clear();
                        hub.update_filter();
                        return Ok(());
                    }
                }
                if self.render_state.workspaces.len() > 1 {
                    self.render_state.active_workspace = 1;
                }
            }
            return Ok(());
        }

        // Leader key
        let leader_key = config::normalize_key(self.config.leader.key);
        if normalized == leader_key {
            self.enter_leader_mode();
            return Ok(());
        }

        // Global keymap (ctrl+q, shift+pageup, etc.)
        if let Some(action) = self.config.keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui, writer).await;
        }

        // Normal mode data-driven keymap
        if let Some(action) = self.config.normal_keys.lookup(&normalized).cloned() {
            return self.execute_action(action, tui, writer).await;
        }

        // 1-9 → FocusGroupN (number keys are dynamic, keep outside the keymap)
        if let KeyCode::Char(c) = normalized.code {
            if c.is_ascii_digit()
                && c != '0'
                && normalized.modifiers == KeyModifiers::NONE
            {
                let n = c as u8 - b'0';
                return self
                    .execute_action(Action::FocusGroupN(n), tui, writer)
                    .await;
            }
        }

        // Home workspace sidebar: arrow keys navigate, Enter opens, typing searches
        if self.is_home_active() {
            // If a widget is focused, route j/k/Enter to widget item navigation
            let widget_focused = self.project_hub_state.as_ref().is_some_and(|h| h.focused_widget.is_some());
            if widget_focused {
                match normalized.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        let widget = self.focused_widget_kind();
                        let count = widget.as_ref().and_then(|w| {
                            self.project_hub_state.as_ref().map(|h| {
                                crate::ui::project_hub::widget_item_count(w, h.selected_git_info())
                            })
                        }).unwrap_or(0);
                        if let Some(ref mut hub) = self.project_hub_state {
                            if let Some(id) = hub.focused_widget {
                                let state = hub.widget_interact.entry(id).or_default();
                                if state.selected + 1 < count {
                                    state.selected += 1;
                                }
                            }
                        }
                        return Ok(());
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if let Some(ref mut hub) = self.project_hub_state {
                            if let Some(id) = hub.focused_widget {
                                let state = hub.widget_interact.entry(id).or_default();
                                state.selected = state.selected.saturating_sub(1);
                            }
                        }
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        let text = self.focused_widget_kind().and_then(|w| {
                            self.project_hub_state.as_ref().and_then(|h| {
                                let id = h.focused_widget?;
                                let selected = h.widget_interact.get(&id).map(|s| s.selected).unwrap_or(0);
                                crate::ui::project_hub::widget_selected_text(&w, h.selected_git_info(), selected)
                            })
                        });
                        if let Some(text) = text {
                            let _ = crate::clipboard::copy_to_clipboard(&text);
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }

            let hub_action = self.project_hub_state.as_mut().and_then(|hub| {
                if hub.focused_widget.is_some() {
                    return None; // handled above
                }
                match normalized.code {
                    KeyCode::Up => { hub.move_up(); Some(()) }
                    KeyCode::Down => { hub.move_down(); Some(()) }
                    KeyCode::Backspace => {
                        if hub.input.pop().is_some() {
                            hub.update_filter();
                        }
                        Some(())
                    }
                    KeyCode::Char(c) if normalized.modifiers == KeyModifiers::NONE => {
                        hub.input.push(c);
                        hub.update_filter();
                        Some(())
                    }
                    _ => None,
                }
            });
            if hub_action.is_some() {
                return Ok(());
            }
            // Enter: open project (needs &mut self after hub borrow ends)
            if normalized.code == KeyCode::Enter {
                let dir = self.project_hub_state.as_ref()
                    .and_then(|hub| hub.selected_project())
                    .map(|p| p.path.to_string_lossy().to_string());
                if let Some(dir) = dir {
                    self.open_project(&dir, writer).await?;
                }
                return Ok(());
            }
        }

        // No PTY fallback in Normal mode — keys are consumed
        Ok(())
    }

    async fn handle_status_bar_click(
        &mut self,
        label: &str,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match label {
            "leader" => self.enter_leader_mode(),
            "normal" => {
                self.mode = Mode::Normal;
                self.focus_location = FocusLocation::WindowLayout;
            }
            "commands" => {
                self.execute_action(Action::CommandPalette, tui, writer).await?;
            }
            "new tab" => {
                self.execute_action(Action::NewTab, tui, writer).await?;
            }
            "split" => {
                self.execute_action(Action::SplitHorizontal, tui, writer).await?;
            }
            "new ws" | "new" => {
                self.execute_action(Action::NewWorkspace, tui, writer).await?;
            }
            "quit" => {
                self.execute_action(Action::Quit, tui, writer).await?;
            }
            "close" => {
                self.execute_action(Action::CloseTab, tui, writer).await?;
            }
            "exit bar" => {
                self.focus_location = FocusLocation::WindowLayout;
            }
            "open" => {
                let dir = self.project_hub_state.as_ref()
                    .and_then(|hub| hub.selected_project())
                    .map(|p| p.path.to_string_lossy().to_string());
                if let Some(dir) = dir {
                    self.open_project(&dir, writer).await?;
                }
            }
            _ => {} // Buttons like "search", "switch" that need keyboard input
        }
        Ok(())
    }

    fn enter_leader_mode(&mut self) {
        self.push_focus();
        let root = self.config.leader.root.clone();
        self.leader_state = Some(LeaderState {
            path: Vec::new(),
            current_node: root,
            popup_visible: true,
        });
        self.mode = Mode::Leader;
    }

    async fn execute_action(
        &mut self,
        action: Action,
        _tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        // Workspace bar focus mode
        if self.focus_location == FocusLocation::WorkspaceBar {
            match &action {
                Action::FocusLeft => {
                    if self.is_home_active() {
                        // Already at leftmost (Hub)
                    } else {
                        let idx = self.render_state.active_workspace;
                        if idx > 0 {
                            self.render_state.active_workspace = idx - 1;
                            let mut w = writer.lock().await;
                            let _ = send_request(
                                &mut w,
                                &ClientRequest::Command(format!("select-workspace -t {}", idx - 1)),
                            )
                            .await;
                        } else {
                            // At first daemon workspace, go to hub
                            self.render_state.active_workspace = 0;
                            if self.project_hub_state.is_none() {
                                self.project_hub_state = Some(ProjectHubState::new(&self.config, self.event_tx.as_ref().unwrap().clone()));
                            }
                        }
                    }
                    return Ok(());
                }
                Action::FocusRight => {
                    if self.is_home_active() {
                        // From hub, go to first non-home workspace if exists
                        if self.render_state.workspaces.len() > 1 {
                            self.render_state.active_workspace = 1;
                            let mut w = writer.lock().await;
                            let _ = send_request(
                                &mut w,
                                &ClientRequest::Command("select-workspace -t 1".to_string()),
                            )
                            .await;
                        }
                    } else {
                        let idx = self.render_state.active_workspace;
                        if idx + 1 < self.render_state.workspaces.len() {
                            self.render_state.active_workspace = idx + 1;
                            let mut w = writer.lock().await;
                            let _ = send_request(
                                &mut w,
                                &ClientRequest::Command(format!("select-workspace -t {}", idx + 1)),
                            )
                            .await;
                        }
                    }
                    return Ok(());
                }
                Action::FocusDown | Action::FocusUp => {
                    self.focus_location = FocusLocation::WindowLayout;
                    return Ok(());
                }
                Action::CloseTab => {
                    if self.is_home_active() {
                        // Can't close the hub workspace
                        return Ok(());
                    }
                    // Remap to close workspace when bar is focused
                    let is_last = self.render_state.workspaces.len() <= 1;
                    self.pending_confirm_action = Some(Action::CloseWorkspace);
                    self.confirm_message = Some(if is_last {
                        "Close last workspace? This will end the session.".into()
                    } else {
                        "Close this workspace?".into()
                    });
                    self.push_focus();
                    self.mode = Mode::Confirm;
                    return Ok(());
                }
                Action::EnterInteract => {
                    // Widget tabs have no PTY — stay in Normal mode
                    if !self.is_home_active() {
                        self.focus_stack.clear();
                        self.focus_location = FocusLocation::WindowLayout;
                        self.mode = Mode::Interact;
                    }
                    return Ok(());
                }
                Action::NewTab => {
                    // On the workspace bar, "n" creates a new workspace, not a new tab
                    self.push_focus();
                    let home = std::env::var("HOME")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|_| std::path::PathBuf::from("/"));
                    self.new_workspace_input = Some(NewWorkspaceInputState {
                        stage: NewWorkspaceStage::Directory,
                        name: String::new(),
                        browser: DirBrowser::new(home),
                        last_click: None,
                    });
                    self.mode = Mode::NewWorkspaceInput;
                    return Ok(());
                }
                _ => {
                    self.focus_location = FocusLocation::WindowLayout;
                    // Fall through to normal action handling
                }
            }
        }

        // Sidebar resize on home workspace (Shift+H / Shift+L)
        if self.is_home_active() && self.project_hub_state.is_some() {
            let step: i16 = 4;
            match &action {
                Action::ResizeShrinkH => {
                    let current = self.sidebar_width.unwrap_or(30);
                    let new_w = (current as i16 - step).max(crate::ui::project_hub::SIDEBAR_MIN_WIDTH as i16) as u16;
                    self.sidebar_width = Some(new_w);
                    return Ok(());
                }
                Action::ResizeGrowH => {
                    let current = self.sidebar_width.unwrap_or(30);
                    let new_w = (current as i16 + step).min(crate::ui::project_hub::SIDEBAR_MAX_WIDTH as i16) as u16;
                    self.sidebar_width = Some(new_w);
                    return Ok(());
                }
                _ => {}
            }
        }

        // Entering workspace bar focus: FocusUp at the top of the layout
        if action == Action::FocusUp && !self.render_state.workspaces.is_empty() {
            if let Some(ws) = self.active_workspace() {
                let at_top = ws.layout.find_neighbor(
                    ws.active_group,
                    SplitDirection::Vertical,
                    Side::First,
                ).is_none();
                if at_top {
                    self.focus_location = FocusLocation::WorkspaceBar;
                    return Ok(());
                }
            }
        }

        // Client-only actions
        match &action {
            Action::Quit => {
                self.should_quit = true;
                return Ok(());
            }
            Action::Help => {
                // Help opens the command palette (unified palette)
                self.push_focus();
                self.palette_state = Some(UnifiedPaletteState::new_full_search(&self.config.keys, &self.config.leader));
                self.mode = Mode::Palette;
                return Ok(());
            }
            Action::ScrollMode => {
                self.push_focus();
                self.mode = Mode::Scroll;
                return Ok(());
            }
            Action::CopyMode => {
                // Set up copy mode from current screen
                if let Some(ws) = self.active_workspace() {
                    if let Some(group) = ws.groups.iter().find(|g| g.id == ws.active_group) {
                        if let Some(pane) = group.tabs.get(group.active_tab) {
                            if let Some(parser) = self.screens.get(&pane.id) {
                                let screen = parser.screen();
                                let (cursor_row, cursor_col) = screen.cursor_position();
                                let (rows, cols) = screen.size();
                                self.copy_mode_state = Some(CopyModeState::new(
                                    rows as usize,
                                    cols as usize,
                                    cursor_row as usize,
                                    cursor_col as usize,
                                ));
                                self.push_focus();
                                self.mode = Mode::Copy;
                            }
                        }
                    }
                }
                return Ok(());
            }
            Action::CommandPalette => {
                self.push_focus();
                self.palette_state = Some(UnifiedPaletteState::new_full_search(&self.config.keys, &self.config.leader));
                self.mode = Mode::Palette;
                return Ok(());
            }
            Action::EnterInteract => {
                if !self.is_home_active() {
                    self.focus_stack.clear();
                    self.focus_location = FocusLocation::WindowLayout;
                    self.mode = Mode::Interact;
                }
                return Ok(());
            }
            Action::EnterNormal => {
                self.focus_location = FocusLocation::WindowLayout;
                self.mode = Mode::Normal;
                return Ok(());
            }
            Action::Detach => {
                self.should_quit = true;
                return Ok(());
            }
            Action::NewTab => {
                self.open_tab_picker(crate::ui::tab_picker::TabPickerMode::NewTab);
                return Ok(());
            }
            Action::SplitHorizontal => {
                self.open_tab_picker(crate::ui::tab_picker::TabPickerMode::SplitHorizontal);
                return Ok(());
            }
            Action::SplitVertical => {
                self.open_tab_picker(crate::ui::tab_picker::TabPickerMode::SplitVertical);
                return Ok(());
            }
            Action::ChangeWidget => {
                self.open_widget_picker(WidgetPickerMode::Change);
                return Ok(());
            }
            Action::AddWidgetRight => {
                self.open_widget_picker(WidgetPickerMode::SplitHorizontal);
                return Ok(());
            }
            Action::AddWidgetBelow => {
                self.open_widget_picker(WidgetPickerMode::SplitVertical);
                return Ok(());
            }
            Action::RenameWindow => {
                // Pre-populate with the current window name (if set) or the
                // active tab title so the user can edit instead of retyping.
                self.rename_input = self
                    .active_workspace()
                    .and_then(|ws| {
                        let group = ws.groups.iter().find(|g| g.id == ws.active_group)?;
                        group
                            .name
                            .clone()
                            .or_else(|| group.tabs.get(group.active_tab).map(|t| t.title.clone()))
                    })
                    .unwrap_or_default();
                self.rename_target = RenameTarget::Window;
                self.push_focus();
                self.mode = Mode::Rename;
                return Ok(());
            }
            Action::RenameWorkspace => {
                // Pre-populate with the current workspace name.
                self.rename_input = self
                    .active_workspace()
                    .map(|ws| ws.name.clone())
                    .unwrap_or_default();
                self.rename_target = RenameTarget::Workspace;
                self.push_focus();
                self.mode = Mode::Rename;
                return Ok(());
            }
            Action::NewWorkspace => {
                let home = std::env::var("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from("/"));
                self.new_workspace_input = Some(NewWorkspaceInputState {
                    stage: NewWorkspaceStage::Directory,
                    name: String::new(),
                    browser: DirBrowser::new(home),
                    last_click: None,
                });
                self.push_focus();
                self.mode = Mode::NewWorkspaceInput;
                return Ok(());
            }
            Action::ProjectHub => {
                self.render_state.active_workspace = 0;
                self.focus_location = FocusLocation::WindowLayout;
                if self.project_hub_state.is_none() {
                    self.project_hub_state = Some(ProjectHubState::new(&self.config, self.event_tx.as_ref().unwrap().clone()));
                }
                return Ok(());
            }
            Action::ResizeMode => {
                self.resize_state = Some(ResizeState { selected: None });
                self.push_focus();
                self.mode = Mode::Resize;
                return Ok(());
            }
            Action::PasteClipboard => {
                if let Ok(text) = clipboard::paste_from_clipboard() {
                    if !text.is_empty() {
                        let mut w = writer.lock().await;
                        let _ = send_request(
                            &mut w,
                            &ClientRequest::Paste(text),
                        )
                        .await;
                    }
                }
                return Ok(());
            }
            _ => {}
        }

        // Destructive actions — smart confirm: only prompt if foreground process
        match &action {
            Action::CloseTab => {
                let has_fg = self
                    .active_workspace()
                    .and_then(|ws| ws.groups.iter().find(|g| g.id == ws.active_group))
                    .and_then(|g| g.tabs.get(g.active_tab))
                    .and_then(|tab| tab.foreground_process.as_ref())
                    .is_some();

                if has_fg {
                    self.pending_confirm_action = Some(Action::CloseTab);
                    self.confirm_message = Some("Close this tab? (process running)".into());
                    self.push_focus();
                    self.mode = Mode::Confirm;
                } else {
                    // Idle shell — close immediately
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::Command("kill-pane".to_string())).await;
                }
                return Ok(());
            }
            Action::CloseWorkspace => {
                let is_last = self.render_state.workspaces.len() <= 1;
                let has_any_fg = self
                    .active_workspace()
                    .map(|ws| {
                        ws.groups.iter().any(|g| {
                            g.tabs.iter().any(|tab| tab.foreground_process.is_some())
                        })
                    })
                    .unwrap_or(false);

                if is_last {
                    self.pending_confirm_action = Some(Action::CloseWorkspace);
                    self.confirm_message = Some("Close last workspace? This will end the session.".into());
                    self.push_focus();
                    self.mode = Mode::Confirm;
                } else if has_any_fg {
                    self.pending_confirm_action = Some(Action::CloseWorkspace);
                    self.confirm_message = Some("Close workspace? (processes running)".into());
                    self.push_focus();
                    self.mode = Mode::Confirm;
                } else {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::Command("close-workspace".to_string())).await;
                }
                return Ok(());
            }
            _ => {}
        }

        // Workspace switch — update locally before sending to server
        if let Action::SwitchWorkspace(n) = &action {
            let idx = (*n as usize).saturating_sub(1);
            if idx < self.render_state.workspaces.len() {
                self.render_state.active_workspace = idx;
                self.focus_stack.clear();
                self.focus_location = FocusLocation::WindowLayout;
                self.mode = Mode::Normal;
            }
        }

        // Server-mutating actions — translate to commands
        if let Some(cmd) = action_to_command(&action) {
            let mut w = writer.lock().await;
            let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
        }
        Ok(())
    }

    async fn handle_tab_picker_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let state = match self.tab_picker_state.as_mut() {
            Some(s) => s,
            None => {
                self.pop_focus();
                self.mode = Mode::Normal;
                return Ok(());
            }
        };

        match key.code {
            KeyCode::Esc => {
                self.tab_picker_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let cmd = if let Some(cmd) = state.selected_command() {
                    Some(cmd)
                } else if !state.input.trim().is_empty() {
                    // No match — run the typed input as a custom command wrapped in user's shell
                    let user_shell = std::env::var("SHELL")
                        .unwrap_or_else(|_| "/bin/bash".to_string());
                    let base = match state.mode {
                        crate::ui::tab_picker::TabPickerMode::NewTab => "new-window",
                        crate::ui::tab_picker::TabPickerMode::SplitHorizontal => "split-window -h",
                        crate::ui::tab_picker::TabPickerMode::SplitVertical => "split-window -v",
                    };
                    let escaped_input = state.input.trim().replace('\\', "\\\\").replace('"', "\\\"");
                    let escaped_shell = user_shell.replace('\\', "\\\\").replace('"', "\\\"");
                    Some(format!("{} -c \"{}\" -s \"{}\"", base, escaped_input, escaped_shell))
                } else {
                    None
                };
                if let Some(cmd) = cmd {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                }
                self.tab_picker_state = None;
                self.focus_stack.clear();
                self.focus_location = FocusLocation::WindowLayout;
                self.mode = Mode::Interact;
            }
            KeyCode::Up => state.move_up(),
            KeyCode::Down => state.move_down(),
            KeyCode::Tab => {
                if let Some((name, is_fav)) = state.toggle_favorite() {
                    if is_fav {
                        self.favorites.insert(name);
                    } else {
                        self.favorites.remove(&name);
                    }
                    crate::ui::tab_picker::save_favorites(&self.favorites);
                }
            }
            KeyCode::Char(' ') if state.input.is_empty() => {
                // Space on empty input → open command palette (leader key)
                self.tab_picker_state = None;
                self.palette_state = Some(UnifiedPaletteState::new_full_search(
                    &self.config.keys,
                    &self.config.leader,
                ));
                self.mode = Mode::Palette;
            }
            _ => {
                if ui::dialog::handle_text_input(key.code, &mut state.input) {
                    state.update_filter();
                }
            }
        }
        Ok(())
    }

    async fn handle_confirm_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                if let Some(action) = self.pending_confirm_action.take() {
                    if let Some(cmd) = action_to_command(&action) {
                        let mut w = writer.lock().await;
                        let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                    }
                }
                self.confirm_message = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc | KeyCode::Char('n') => {
                self.pending_confirm_action = None;
                self.confirm_message = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_rename_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.rename_input.clear();
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                let name = self.rename_input.clone();
                self.rename_input.clear();
                self.pop_focus();
                self.mode = Mode::Normal;
                if !name.is_empty() {
                    let cmd = match self.rename_target {
                        RenameTarget::Window => format!("rename-window {}", name),
                        RenameTarget::Workspace => format!("rename-workspace {}", name),
                    };
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                }
            }
            _ => {
                ui::dialog::handle_text_input(key.code, &mut self.rename_input);
            }
        }
        Ok(())
    }

    async fn handle_new_workspace_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let state = match self.new_workspace_input.as_mut() {
            Some(s) => s,
            None => {
                self.pop_focus();
                self.mode = Mode::Normal;
                return Ok(());
            }
        };

        match state.stage {
            NewWorkspaceStage::Directory => match (key.code, key.modifiers) {
                // Ctrl+F toggles zoxide search mode
                (KeyCode::Char('f'), m) if m.contains(KeyModifiers::CONTROL) => {
                    state.browser.toggle_search();
                }
                (KeyCode::Esc, _) => {
                    if state.browser.search_mode {
                        // Exit search mode back to browse
                        state.browser.toggle_search();
                    } else if !state.browser.input.is_empty() {
                        state.browser.input.clear();
                        state.browser.update_filter();
                    } else {
                        self.new_workspace_input = None;
                        self.pop_focus();
                        self.mode = Mode::Normal;
                    }
                }
                (KeyCode::Enter, _) => {
                    if state.browser.search_mode {
                        // In search mode: select zoxide result as workspace dir
                        if let Some(zpath) = state.browser.selected_zoxide_path().map(|s| s.to_string()) {
                            let path = std::path::PathBuf::from(&zpath);
                            if path.is_dir() {
                                state.browser.current_dir = path;
                            }
                        }
                        state.browser.search_mode = false;
                        state.browser.input.clear();
                        state.browser.zoxide_results.clear();
                    } else if state.browser.total_count() > 0 {
                        // Browse mode: include the highlighted folder
                        state.browser.enter_selected();
                    }
                    // Confirm directory and advance to Name stage
                    state.name = auto_workspace_name_suggestion(&state.browser.current_dir);
                    state.stage = NewWorkspaceStage::Name;
                }
                (KeyCode::Up, _) => state.browser.move_up(),
                (KeyCode::Down, _) => state.browser.move_down(),
                (KeyCode::Tab, _) | (KeyCode::Right, _) if !state.browser.search_mode => {
                    state.browser.enter_selected();
                }
                (KeyCode::Left, _) if !state.browser.search_mode => {
                    state.browser.go_up();
                }
                (KeyCode::Backspace, _) => {
                    if state.browser.input.is_empty() {
                        if state.browser.search_mode {
                            // Empty backspace in search mode: exit search
                            state.browser.toggle_search();
                        } else {
                            state.browser.go_up();
                        }
                    } else {
                        state.browser.input.pop();
                        state.browser.update_filter();
                    }
                }
                (KeyCode::Char(c), _) => {
                    state.browser.input.push(c);
                    state.browser.update_filter();
                }
                _ => {}
            },
            NewWorkspaceStage::Name => match key.code {
                KeyCode::Esc => {
                    // Go back to directory stage
                    state.stage = NewWorkspaceStage::Directory;
                }
                KeyCode::Enter => {
                    // Create the workspace
                    let name = state.name.clone();
                    let dir = state.browser.current_dir.to_string_lossy().to_string();
                    self.new_workspace_input = None;
                    self.focus_stack.clear();
                    self.focus_location = FocusLocation::WindowLayout;
                    self.mode = Mode::Normal;

                    // New workspace will be appended and become active
                    self.render_state.active_workspace = self.render_state.workspaces.len();
                    let cmd = format!("new-workspace -c \"{}\"", dir);
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
                    if !name.is_empty() {
                        let _ = send_request(
                            &mut w,
                            &ClientRequest::Command(format!("rename-workspace {}", name)),
                        )
                        .await;
                    }
                }
                _ => {
                    ui::dialog::handle_text_input(key.code, &mut state.name);
                }
            },
        }
        Ok(())
    }

    /// Open a project by path: switch to an existing workspace if one matches,
    /// otherwise create a new workspace at that path.
    async fn open_project(
        &mut self,
        dir: &str,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        // Check if a workspace already exists for this path
        let existing = self
            .render_state
            .workspaces
            .iter()
            .position(|ws| ws.cwd == dir);

        if let Some(idx) = existing {
            self.render_state.active_workspace = idx;
            let mut w = writer.lock().await;
            let _ = send_request(
                &mut w,
                &ClientRequest::Command(format!("select-workspace -t {}", idx)),
            )
            .await;
        } else {
            self.render_state.active_workspace = self.render_state.workspaces.len();
            let cmd = format!("new-workspace -c \"{}\"", dir);
            let mut w = writer.lock().await;
            let _ = send_request(&mut w, &ClientRequest::Command(cmd)).await;
        }
        Ok(())
    }

    async fn handle_resize_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        let state = match self.resize_state.as_mut() {
            Some(s) => s,
            None => {
                self.pop_focus();
                self.mode = Mode::Normal;
                return Ok(());
            }
        };

        match key.code {
            KeyCode::Esc => {
                self.resize_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('=') => {
                let mut w = writer.lock().await;
                let _ =
                    send_request(&mut w, &ClientRequest::Command("equalize-layout".to_string()))
                        .await;
            }
            KeyCode::Char(c @ ('h' | 'j' | 'k' | 'l')) => {
                if let Some(selected) = state.selected {
                    // Border already selected → h/l or j/k move it.
                    // -R = grow active pane, -L = shrink. For the Left border,
                    // pressing 'h' (push border left) grows the pane, so invert.
                    let cmd = match selected {
                        ResizeBorder::Right => match c {
                            'l' => "resize-pane -R",
                            'h' => "resize-pane -L",
                            _ => return Ok(()),
                        },
                        ResizeBorder::Left => match c {
                            'h' => "resize-pane -R",
                            'l' => "resize-pane -L",
                            _ => return Ok(()),
                        },
                        ResizeBorder::Bottom => match c {
                            'j' => "resize-pane -D",
                            'k' => "resize-pane -U",
                            _ => return Ok(()),
                        },
                        ResizeBorder::Top => match c {
                            'k' => "resize-pane -D",
                            'j' => "resize-pane -U",
                            _ => return Ok(()),
                        },
                    };
                    let mut w = writer.lock().await;
                    let _ =
                        send_request(&mut w, &ClientRequest::Command(cmd.to_string())).await;
                } else {
                    // No border selected yet → select this one
                    state.selected = Some(match c {
                        'h' => ResizeBorder::Left,
                        'l' => ResizeBorder::Right,
                        'j' => ResizeBorder::Bottom,
                        'k' => ResizeBorder::Top,
                        _ => unreachable!(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_context_menu_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.context_menu_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut cm) = self.context_menu_state {
                    cm.move_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut cm) = self.context_menu_state {
                    cm.move_down();
                }
            }
            KeyCode::Enter => {
                if let Some(cm) = self.context_menu_state.take() {
                    self.pop_focus();
                    self.mode = Mode::Normal;
                    if let Some(action) = cm.selected_action().cloned() {
                        self.execute_action(action, tui, writer).await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_copy_mode_key(&mut self, key: KeyEvent) -> Result<()> {
        // Get the pane_id for the active pane so we can borrow screen and cms separately
        let pane_id = self
            .active_workspace()
            .and_then(|ws| ws.groups.iter().find(|g| g.id == ws.active_group))
            .and_then(|g| g.tabs.get(g.active_tab))
            .map(|p| p.id);

        let pane_id = match pane_id {
            Some(id) => id,
            None => {
                self.pop_focus();
                self.mode = Mode::Normal;
                self.copy_mode_state = None;
                return Ok(());
            }
        };

        let screen = match self.screens.get(&pane_id) {
            Some(parser) => parser.screen(),
            None => {
                self.pop_focus();
                self.mode = Mode::Normal;
                self.copy_mode_state = None;
                return Ok(());
            }
        };

        if let Some(ref mut cms) = self.copy_mode_state {
            match cms.handle_key(key, screen) {
                CopyModeAction::None => {}
                CopyModeAction::YankSelection(text) => {
                    let _ = clipboard::copy_to_clipboard(&text);
                    self.copy_mode_state = None;
                    self.pop_focus();
                    self.mode = Mode::Normal;
                }
                CopyModeAction::Exit => {
                    self.copy_mode_state = None;
                    self.pop_focus();
                    self.mode = Mode::Normal;
                }
            }
        }
        Ok(())
    }

    async fn handle_scroll_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Char('g') | KeyCode::Home => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut w, &ClientRequest::Command("scroll-to-top".to_string())).await;
            }
            KeyCode::Char('G') | KeyCode::End => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut w, &ClientRequest::Command("scroll-to-bottom".to_string())).await;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut w, &ClientRequest::MouseScroll { up: true }).await;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let mut w = writer.lock().await;
                let _ = send_request(&mut w, &ClientRequest::MouseScroll { up: false }).await;
            }
            KeyCode::PageUp | KeyCode::Char('u') => {
                for _ in 0..10 {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::MouseScroll { up: true }).await;
                }
            }
            KeyCode::PageDown | KeyCode::Char('d') => {
                for _ in 0..10 {
                    let mut w = writer.lock().await;
                    let _ = send_request(&mut w, &ClientRequest::MouseScroll { up: false }).await;
                }
            }
            _ => {
                self.pop_focus();
                self.mode = Mode::Normal;
                // Forward the key to PTY
                let mut w = writer.lock().await;
                let _ = send_request(
                    &mut w,
                    &ClientRequest::Key(SerializableKeyEvent::from(key)),
                )
                .await;
            }
        }
        Ok(())
    }

    async fn handle_palette_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        if let Some(ref mut cp) = self.palette_state {
            match key.code {
                KeyCode::Esc => {
                    self.palette_state = None;
                    self.pop_focus();
                    self.mode = Mode::Normal;
                }
                KeyCode::Enter => {
                    if let Some(action) = cp.selected_action() {
                        self.palette_state = None;
                        self.pop_focus();
                        self.mode = Mode::Normal;
                        return self.execute_action(action, tui, writer).await;
                    }
                }
                KeyCode::Up => cp.move_up(),
                KeyCode::Down => cp.move_down(),
                _ => {
                    if ui::dialog::handle_text_input(key.code, &mut cp.input) {
                        cp.update_filter();
                    }
                }
            }
        } else {
            self.pop_focus();
            self.mode = Mode::Normal;
        }
        Ok(())
    }

    async fn handle_leader_key(
        &mut self,
        key: KeyEvent,
        tui: &Tui,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        use pane_protocol::config::LeaderNode;

        if key.code == KeyCode::Esc {
            self.leader_state = None;
            self.pop_focus();
            self.mode = Mode::Normal;
            return Ok(());
        }

        let normalized = config::normalize_key(key);
        let next = {
            let ls = self.leader_state.as_ref().unwrap();
            if let LeaderNode::Group { children, .. } = &ls.current_node {
                children.get(&normalized).cloned()
            } else {
                None
            }
        };

        match next {
            Some(LeaderNode::Leaf { action, .. }) => {
                self.leader_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
                return self.execute_action(action, tui, writer).await;
            }
            Some(LeaderNode::PassThrough) => {
                self.leader_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
                let leader_key = self.config.leader.key;
                let mut w = writer.lock().await;
                let _ = send_request(
                    &mut w,
                    &ClientRequest::Key(SerializableKeyEvent::from(leader_key)),
                )
                .await;
            }
            Some(group @ LeaderNode::Group { .. }) => {
                let ls = self.leader_state.as_mut().unwrap();
                ls.path.push(normalized);
                ls.current_node = group;
            }
            None => {
                self.leader_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
        }
        Ok(())
    }

    // --- Accessors for UI rendering ---

    pub fn active_workspace(&self) -> Option<&WorkspaceSnapshot> {
        self.render_state
            .workspaces
            .get(self.render_state.active_workspace)
    }

    /// Return the `HubWidget` kind for the currently focused widget window, if any.
    fn focused_widget_kind(&self) -> Option<HubWidget> {
        let hub = self.project_hub_state.as_ref()?;
        let focused_id = hub.focused_widget?;
        let ws = self.active_workspace()?;
        let group = ws.groups.iter().find(|g| g.id == focused_id)?;
        let tab = group.tabs.get(group.active_tab)?;
        if let TabKind::Widget(w) = &tab.kind {
            Some(w.clone())
        } else {
            None
        }
    }

    /// Get the current workspace CWD, if any.
    fn current_cwd(&self) -> Option<&str> {
        self.active_workspace()
            .map(|ws| ws.cwd.as_str())
            .filter(|s| !s.is_empty())
    }

    /// Scan project scripts for the current workspace and return entries.
    fn current_project_scripts(&self) -> Vec<crate::ui::tab_picker::TabPickerEntry> {
        self.current_cwd()
            .map(|cwd| {
                crate::ui::tab_picker::scan_project_scripts(std::path::Path::new(cwd))
            })
            .unwrap_or_default()
    }

    fn open_tab_picker(&mut self, mode: crate::ui::tab_picker::TabPickerMode) {
        let scripts = self.current_project_scripts();
        self.tab_picker_state = Some(TabPickerState::with_scripts(
            &self.system_programs,
            &self.config.tab_picker_entries,
            &self.favorites,
            mode,
            &scripts,
        ));
        self.push_focus();
        self.mode = Mode::TabPicker;
    }

    fn open_widget_picker(&mut self, mode: WidgetPickerMode) {
        self.widget_picker_state = Some(WidgetPickerState::new(mode));
        self.push_focus();
        self.mode = Mode::WidgetPicker;
    }

    async fn handle_widget_picker_key(
        &mut self,
        key: KeyEvent,
        writer: &Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.widget_picker_state = None;
                self.pop_focus();
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut wp) = self.widget_picker_state {
                    wp.move_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut wp) = self.widget_picker_state {
                    wp.move_down();
                }
            }
            KeyCode::Enter => {
                if let Some(wp) = self.widget_picker_state.take() {
                    self.pop_focus();
                    self.mode = Mode::Normal;
                    let widget_name = wp.selected_widget().map(|w| w.as_str().to_string());
                    if let Some(name) = widget_name {
                        let mut w = writer.lock().await;
                        match wp.mode {
                            WidgetPickerMode::Change => {
                                let _ = send_request(
                                    &mut w,
                                    &ClientRequest::Command(format!("set-widget {}", name)),
                                )
                                .await;
                            }
                            WidgetPickerMode::SplitHorizontal => {
                                let _ = send_request(
                                    &mut w,
                                    &ClientRequest::Command("split-window -h".to_string()),
                                )
                                .await;
                                let _ = send_request(
                                    &mut w,
                                    &ClientRequest::Command(format!("set-widget {}", name)),
                                )
                                .await;
                            }
                            WidgetPickerMode::SplitVertical => {
                                let _ = send_request(
                                    &mut w,
                                    &ClientRequest::Command("split-window -v".to_string()),
                                )
                                .await;
                                let _ = send_request(
                                    &mut w,
                                    &ClientRequest::Command(format!("set-widget {}", name)),
                                )
                                .await;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn pane_screen(&self, pane_id: TabId) -> Option<&vt100::Screen> {
        self.screens.get(&pane_id).map(|p| p.screen())
    }

    /// Hit-test the tab bar across all visible windows.
    /// Returns which tab or + button was clicked, along with the window index.
    fn hit_test_tab_bar(&self, tui: &Tui, x: u16, y: u16) -> Option<TabBarHit> {
        let ws = self.active_workspace()?;
        let size = tui.size().ok()?;
        let show_workspace_bar = !self.render_state.workspaces.is_empty();
        let bar_h = if show_workspace_bar {
            crate::ui::workspace_bar::HEIGHT
        } else {
            0
        };
        let body_height = size.height.saturating_sub(bar_h + 1); // 1 for status bar
        let body = Rect::new(0, bar_h, size.width, body_height);

        let resolved = ws
            .layout
            .resolve_with_folds(body, &ws.folded_windows);
        for rp in &resolved {
            if let pane_protocol::layout::ResolvedPane::Visible { id: group_id, rect } = rp {
                if let Some(group) = ws.groups.iter().find(|g| g.id == *group_id) {
                    // Compute tab bar area: same as tab_bar_area() in daemon
                    let block = ratatui::widgets::Block::default()
                        .borders(ratatui::widgets::Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded);
                    let inner = block.inner(*rect);
                    if inner.width <= 2 || inner.height == 0 {
                        continue;
                    }
                    let padded = Rect::new(inner.x + 1, inner.y, inner.width - 2, 1);
                    if y != padded.y {
                        continue;
                    }
                    let max_x = padded.x + padded.width;
                    let plus_reserve: u16 = 3;

                    // Check + button first (right-aligned)
                    if plus_reserve <= max_x.saturating_sub(padded.x) {
                        let plus_start = max_x - plus_reserve;
                        if x >= plus_start && x < max_x && !group.tabs.is_empty() {
                            return Some(TabBarHit::Plus);
                        }
                    }

                    // Check individual tabs (sliding window layout matching render)
                    let sep_width: u16 = 3;
                    let indicator_width: u16 = 2;
                    let n = group.tabs.len();

                    let label_widths: Vec<u16> = group
                        .tabs
                        .iter()
                        .map(|tab| tab.title.len() as u16 + 2)
                        .collect();

                    let total: u16 = label_widths.iter().sum::<u16>()
                        + if n > 1 { sep_width * (n as u16 - 1) } else { 0 }
                        + plus_reserve;

                    let (lo, hi) = if n == 0 || total <= padded.width {
                        (0, n.saturating_sub(1))
                    } else {
                        let active = group.active_tab.min(n - 1);
                        let range_w = |lo: usize, hi: usize| -> u16 {
                            let mut w: u16 = 0;
                            for (j, lw) in label_widths[lo..=hi].iter().enumerate() {
                                w += lw;
                                if j > 0 { w += sep_width; }
                            }
                            if lo > 0 { w += indicator_width; }
                            if hi < n - 1 { w += indicator_width; }
                            w + plus_reserve
                        };
                        let mut lo = active;
                        let mut hi = active;
                        loop {
                            let mut expanded = false;
                            if lo > 0 && range_w(lo - 1, hi) <= padded.width {
                                lo -= 1;
                                expanded = true;
                            }
                            if hi + 1 < n && range_w(lo, hi + 1) <= padded.width {
                                hi += 1;
                                expanded = true;
                            }
                            if !expanded { break; }
                        }
                        (lo, hi)
                    };

                    let mut cursor = padded.x;
                    if lo > 0 {
                        cursor += indicator_width;
                    }
                    if n > 0 {
                        for (j, &lw) in label_widths[lo..=hi].iter().enumerate() {
                            if j > 0 {
                                cursor += sep_width;
                            }
                            let tab_start = cursor;
                            cursor += lw;

                            if x >= tab_start && x < cursor {
                                return Some(TabBarHit::Tab {
                                    group_index: ws.groups.iter().position(|g| g.id == *group_id).unwrap_or(0),
                                    tab_index: lo + j,
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Check if a click hits the + button in any visible window's tab bar.
    fn hit_test_tab_bar_plus(&self, tui: &Tui, x: u16, y: u16) -> bool {
        matches!(self.hit_test_tab_bar(tui, x, y), Some(TabBarHit::Plus))
    }

    fn update_terminal_title(&self) {
        if let Some(ref fmt) = self.config.behavior.terminal_title_format {
            let workspace = self
                .active_workspace()
                .map(|ws| ws.name.as_str())
                .unwrap_or("");
            let title = fmt
                .replace("{session}", "pane")
                .replace("{workspace}", workspace);
            // OSC 0 - set terminal title
            print!("\x1b]0;{}\x07", title);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
    }
}

/// Events fed into the client event loop.
enum ServerEvent {
    Terminal(crate::event::AppEvent),
    Server(ServerResponse),
    Disconnected,
    GitInfoReady { project_idx: usize, info: Box<ProjectGitInfo>, refreshing: bool },
}

// Implement From so the event loop channel works
impl From<crate::event::AppEvent> for ServerEvent {
    fn from(e: crate::event::AppEvent) -> Self {
        ServerEvent::Terminal(e)
    }
}

/// Send a client request using length-prefixed framing on the write half.
async fn send_request(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request: &ClientRequest,
) -> Result<()> {
    let json = serde_json::to_vec(request)?;
    let len = json.len() as u32;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Translate an Action to a server command string.
fn action_to_command(action: &Action) -> Option<String> {
    match action {
        Action::CloseWorkspace => Some("close-workspace".to_string()),
        Action::SwitchWorkspace(n) => Some(format!("select-workspace -t {}", (*n as usize) - 1)),
        // Action::NewTab is handled client-side (opens picker)
        Action::NextTab => Some("next-tab".to_string()),
        Action::PrevTab => Some("prev-tab".to_string()),
        Action::CloseTab => Some("kill-pane".to_string()),
        // SplitHorizontal/SplitVertical handled client-side (opens picker)
        Action::RestartPane => Some("restart-pane".to_string()),
        Action::FocusLeft => Some("select-pane -L".to_string()),
        Action::FocusDown => Some("select-pane -D".to_string()),
        Action::FocusUp => Some("select-pane -U".to_string()),
        Action::FocusRight => Some("select-pane -R".to_string()),
        Action::MoveTabLeft => Some("move-tab -L".to_string()),
        Action::MoveTabDown => Some("move-tab -D".to_string()),
        Action::MoveTabUp => Some("move-tab -U".to_string()),
        Action::MoveTabRight => Some("move-tab -R".to_string()),
        Action::ResizeShrinkH => Some("resize-pane -L".to_string()),
        Action::ResizeGrowH => Some("resize-pane -R".to_string()),
        Action::ResizeGrowV => Some("resize-pane -D".to_string()),
        Action::ResizeShrinkV => Some("resize-pane -U".to_string()),
        Action::Equalize => Some("equalize-layout".to_string()),
        Action::ToggleSyncPanes => Some("toggle-sync".to_string()),
        Action::SelectLayout(name) => Some(format!("select-layout {}", name)),
        Action::FocusGroupN(n) => {
            let ws_idx = (*n as usize) - 1;
            Some(format!("select-window -t {}", ws_idx))
        }
        Action::ReloadConfig => Some("reload-config".to_string()),
        Action::MaximizeFocused => Some("maximize-focused".to_string()),
        Action::ToggleZoom => Some("toggle-zoom".to_string()),
        Action::ToggleFloat => Some("toggle-float".to_string()),
        Action::NewFloat => Some("new-float".to_string()),
        Action::ToggleFold => Some("toggle-fold".to_string()),
        Action::RenameWindow | Action::RenameWorkspace => None,
        // Client-only actions handled before this function is called
        Action::Quit
        | Action::Help
        | Action::ScrollMode
        | Action::CopyMode
        | Action::CommandPalette
        | Action::PasteClipboard
        | Action::EnterInteract
        | Action::EnterNormal
        | Action::Detach
        | Action::NewWorkspace // opens input dialog client-side
        | Action::ProjectHub // opens project hub client-side
        | Action::NewTab // NewTab opens picker client-side
        | Action::SplitHorizontal // opens picker client-side
        | Action::SplitVertical // opens picker client-side
        | Action::ResizeMode
        | Action::ChangeWidget // opens widget picker client-side
        | Action::AddWidgetRight // opens widget picker client-side
        | Action::AddWidgetBelow => None, // opens widget picker client-side
    }
}

/// Suggest a workspace name from a directory: git repo name, then folder name.
/// Converts kebab-case/snake_case to Title Case.
fn auto_workspace_name_suggestion(dir: &std::path::Path) -> String {
    let raw = git_repo_name_for_dir(dir)
        .or_else(|| dir.file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_default();
    titlecase_name(&raw)
}

/// Convert a kebab-case or snake_case name to Title Case.
fn titlecase_name(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{}{}", upper, chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn print_detach_summary(state: &RenderState) {
    let total_tabs: usize = state.workspaces.iter()
        .flat_map(|ws| &ws.groups)
        .map(|g| g.tabs.len())
        .sum();

    if total_tabs == 0 {
        return;
    }

    eprintln!("\x1b[2m[detached — {} tab{} running across {} workspace{}]\x1b[0m",
        total_tabs,
        if total_tabs == 1 { "" } else { "s" },
        state.workspaces.len(),
        if state.workspaces.len() == 1 { "" } else { "s" },
    );

    for ws in &state.workspaces {
        let window_count = ws.groups.len();
        let tab_count: usize = ws.groups.iter().map(|g| g.tabs.len()).sum();
        eprintln!("\x1b[2m  {} — {} window{}, {} tab{}\x1b[0m",
            ws.name,
            window_count,
            if window_count == 1 { "" } else { "s" },
            tab_count,
            if tab_count == 1 { "" } else { "s" },
        );
    }
}

/// Get the repository name from a directory by finding the git root and
/// reading the origin remote URL.
fn git_repo_name_for_dir(dir: &std::path::Path) -> Option<String> {
    let mut d = dir.to_path_buf();
    loop {
        if d.join(".git").exists() {
            break;
        }
        if !d.pop() {
            return None;
        }
    }
    if let Ok(config) = std::fs::read_to_string(d.join(".git/config")) {
        for line in config.lines() {
            let trimmed = line.trim();
            if let Some(url) = trimmed.strip_prefix("url = ") {
                let url = url.trim();
                let path = url.strip_suffix(".git").unwrap_or(url);
                let name = path.rsplit('/').next()
                    .or_else(|| path.rsplit(':').next())
                    .filter(|n| !n.is_empty());
                if let Some(n) = name {
                    return Some(n.to_string());
                }
            }
        }
    }
    // Fall back to repo root directory name
    d.file_name().map(|f| f.to_string_lossy().to_string())
}
