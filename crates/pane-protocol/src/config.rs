use std::collections::HashMap;
use std::sync::OnceLock;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Action enum — all bindable actions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum Action {
    Quit,
    NewWorkspace,
    CloseWorkspace,
    SwitchWorkspace(u8), // 1-indexed
    NewTab,
    NextTab,
    PrevTab,
    CloseTab,
    SplitHorizontal,
    SplitVertical,
    RestartPane,
    FocusLeft,
    FocusDown,
    FocusUp,
    FocusRight,
    FocusGroupN(u8), // 1-indexed
    MoveTabLeft,
    MoveTabDown,
    MoveTabUp,
    MoveTabRight,
    ResizeShrinkH,
    ResizeGrowH,
    ResizeGrowV,
    ResizeShrinkV,
    Equalize,
    Help,
    ScrollMode,
    CopyMode,
    PasteClipboard,
    SelectLayout(String),
    ToggleSyncPanes,
    CommandPalette,
    RenameWindow,
    RenameWorkspace,
    Detach,
    EnterInteract,
    EnterNormal,
    MaximizeFocused,
    ToggleZoom,
    ToggleFloat,
    NewFloat,
    ToggleFold,
    ReloadConfig,
    ResizeMode,
    ProjectHub,
    // Widget management (home workspace)
    ChangeWidget,
    AddWidgetRight,
    AddWidgetBelow,
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Theme {
    pub accent: Color,
    pub border_inactive: Color,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
}

/// Detect whether the terminal uses a light background (cached).
fn is_light_terminal() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        // Check COLORFGBG env var (format "fg;bg")
        if let Ok(val) = std::env::var("COLORFGBG") {
            if let Some(bg) = val.rsplit(';').next().and_then(|s| s.parse::<u8>().ok()) {
                // ANSI palette: 7 = white, 9-15 = bright colors → light background
                return bg == 7 || bg >= 9;
            }
        }
        // macOS: absence of AppleInterfaceStyle means light mode
        #[cfg(target_os = "macos")]
        {
            return std::process::Command::new("defaults")
                .args(["read", "-g", "AppleInterfaceStyle"])
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(false);
        }
        #[cfg(not(target_os = "macos"))]
        false
    })
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            border_inactive: Color::Rgb(70, 70, 70),
            bg: Color::Reset,
            fg: Color::Reset,
            dim: Color::DarkGray,
            tab_active: Color::Cyan,
            tab_inactive: Color::DarkGray,
        }
    }
}

impl Theme {
    /// Light-mode border color for the default theme.
    const BORDER_INACTIVE_LIGHT: Color = Color::Rgb(195, 195, 195);
}

impl Theme {
    /// Load a built-in theme preset by name. Returns `None` for unknown names.
    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "default" => Some(Self::default()),
            "dracula" => Some(Self {
                accent: Color::Rgb(189, 147, 249),       // purple
                border_inactive: Color::Rgb(60, 62, 74),
                bg: Color::Rgb(40, 42, 54),
                fg: Color::Rgb(248, 248, 242),
                dim: Color::Rgb(98, 114, 164),
                tab_active: Color::Rgb(189, 147, 249),
                tab_inactive: Color::Rgb(98, 114, 164),
            }),
            "catppuccin" => Some(Self {
                accent: Color::Rgb(203, 166, 247),       // mauve
                border_inactive: Color::Rgb(55, 55, 71),
                bg: Color::Rgb(30, 30, 46),
                fg: Color::Rgb(205, 214, 244),
                dim: Color::Rgb(108, 112, 134),
                tab_active: Color::Rgb(203, 166, 247),
                tab_inactive: Color::Rgb(108, 112, 134),
            }),
            "tokyo-night" => Some(Self {
                accent: Color::Rgb(122, 162, 247),       // blue
                border_inactive: Color::Rgb(50, 51, 62),
                bg: Color::Rgb(26, 27, 38),
                fg: Color::Rgb(192, 202, 245),
                dim: Color::Rgb(86, 95, 137),
                tab_active: Color::Rgb(122, 162, 247),
                tab_inactive: Color::Rgb(86, 95, 137),
            }),
            _ => None,
        }
    }

    /// Dim a color by a factor (0.0–1.0). Named ANSI colors are mapped to RGB
    /// first. `Color::Reset` and `Color::Indexed` pass through unchanged.
    pub fn dim_color(color: Color, factor: f32) -> Color {
        let (r, g, b) = match color {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Black => (0, 0, 0),
            Color::Red => (205, 0, 0),
            Color::Green => (0, 205, 0),
            Color::Yellow => (205, 205, 0),
            Color::Blue => (0, 0, 238),
            Color::Magenta => (205, 0, 205),
            Color::Cyan => (0, 205, 205),
            Color::Gray => (229, 229, 229),
            Color::DarkGray => (127, 127, 127),
            Color::White => (255, 255, 255),
            Color::LightRed => (255, 0, 0),
            Color::LightGreen => (0, 255, 0),
            Color::LightYellow => (255, 255, 0),
            Color::LightBlue => (92, 92, 255),
            Color::LightMagenta => (255, 0, 255),
            Color::LightCyan => (0, 255, 255),
            _ => return color,
        };
        Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        )
    }
}

// ---------------------------------------------------------------------------
// Behavior
// ---------------------------------------------------------------------------

/// A widget that can be displayed in the project hub detail panel.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HubWidget {
    /// Project name, path, branch, and working tree status.
    ProjectInfo,
    /// Recent git commits.
    RecentCommits,
    /// Changed files from git status.
    ChangedFiles,
    /// All local branches with current branch highlighted.
    Branches,
    /// Git stash list.
    Stashes,
    /// Recent tags.
    Tags,
    /// ASCII git graph (--oneline --graph --all).
    GitGraph,
    /// Top contributors by commit count.
    Contributors,
    /// TODO/FIXME/HACK comments found in source files.
    Todos,
    /// First ~50 lines of README.md.
    Readme,
    /// File count by language/extension.
    Languages,
    /// Disk usage breakdown (total, .git, build dir).
    DiskUsage,
    /// Recent CI runs from GitHub Actions (requires `gh`).
    CiStatus,
    /// Open issues from GitHub (requires `gh`).
    OpenIssues,
    /// Processes running in the project directory.
    RunningProcesses,
}

impl HubWidget {
    pub fn label(&self) -> &str {
        match self {
            Self::ProjectInfo => "Info",
            Self::RecentCommits => "Commits",
            Self::ChangedFiles => "Changes",
            Self::Branches => "Branches",
            Self::Stashes => "Stashes",
            Self::Tags => "Tags",
            Self::GitGraph => "Graph",
            Self::Contributors => "Contributors",
            Self::Todos => "TODOs",
            Self::Readme => "README",
            Self::Languages => "Languages",
            Self::DiskUsage => "Disk",
            Self::CiStatus => "CI",
            Self::OpenIssues => "Issues",
            Self::RunningProcesses => "Processes",
        }
    }

    /// Return the canonical string name used in commands (e.g. "project_info").
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProjectInfo => "project_info",
            Self::RecentCommits => "recent_commits",
            Self::ChangedFiles => "changed_files",
            Self::Branches => "branches",
            Self::Stashes => "stashes",
            Self::Tags => "tags",
            Self::GitGraph => "git_graph",
            Self::Contributors => "contributors",
            Self::Todos => "todos",
            Self::Readme => "readme",
            Self::Languages => "languages",
            Self::DiskUsage => "disk_usage",
            Self::CiStatus => "ci_status",
            Self::OpenIssues => "open_issues",
            Self::RunningProcesses => "running_processes",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "project_info" => Some(Self::ProjectInfo),
            "recent_commits" => Some(Self::RecentCommits),
            "changed_files" => Some(Self::ChangedFiles),
            "branches" => Some(Self::Branches),
            "stashes" => Some(Self::Stashes),
            "tags" => Some(Self::Tags),
            "git_graph" => Some(Self::GitGraph),
            "contributors" => Some(Self::Contributors),
            "todos" => Some(Self::Todos),
            "readme" => Some(Self::Readme),
            "languages" => Some(Self::Languages),
            "disk_usage" => Some(Self::DiskUsage),
            "ci_status" => Some(Self::CiStatus),
            "open_issues" => Some(Self::OpenIssues),
            "running_processes" => Some(Self::RunningProcesses),
            _ => None,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::ProjectInfo,
            Self::RecentCommits,
            Self::ChangedFiles,
            Self::Branches,
            Self::Stashes,
            Self::Tags,
            Self::GitGraph,
            Self::Contributors,
            Self::Todos,
            Self::Readme,
            Self::Languages,
            Self::DiskUsage,
            Self::CiStatus,
            Self::OpenIssues,
            Self::RunningProcesses,
        ]
    }
}

/// Layout configuration for the hub detail panel.
/// Each inner Vec is a row of widgets displayed side-by-side.
/// Rows are stacked vertically.
#[derive(Clone, Debug)]
pub struct HubLayout {
    pub rows: Vec<Vec<HubWidget>>,
}

impl Default for HubLayout {
    fn default() -> Self {
        Self {
            rows: vec![
                vec![HubWidget::ProjectInfo],
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Behavior {
    pub fold_bar_size: u16,
    pub vim_navigator: bool,
    pub mouse: bool,
    pub default_shell: Option<String>,
    /// Seconds of no connected clients before auto-saving and exiting (default: 86400 = 24h).
    pub auto_suspend_secs: u64,
    /// Format string for outer terminal title (e.g., "{session} - {workspace}").
    pub terminal_title_format: Option<String>,
    /// Directories to scan for project repos in the project hub.
    /// Auto-detected from common locations if empty.
    pub projects_dirs: Vec<String>,
    /// Whether to show the project hub on startup.
    pub show_project_hub_on_start: bool,
    /// Widget layout for the hub detail panel.
    pub hub_layout: HubLayout,
    /// Whether the terminal font supports Nerd Font glyphs (resolved at startup).
    pub nerd_fonts: bool,
}

/// Config value for `nerd_fonts`: explicit on/off, or auto-detect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NerdFontsOption {
    On,
    Off,
    Auto,
}

impl NerdFontsOption {
    /// Resolve to a concrete bool.  `Auto` checks common Nerd Font env
    /// variables and font names.
    pub fn resolve(self) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Auto => detect_nerd_font(),
        }
    }
}

/// Best-effort detection of a Nerd Font.
///
/// Checks (in order):
/// 1. `NERD_FONTS` env var (`1`/`true`/`yes` → true, `0`/`false`/`no` → false)
/// 2. Common terminal-specific font env vars for "Nerd" in the name
/// 3. kitty `TERM_PROGRAM` + `macos_titlebar_color` (kitty usually ships NF)
fn detect_nerd_font() -> bool {
    // Explicit env override
    if let Ok(val) = std::env::var("NERD_FONTS") {
        match val.to_lowercase().as_str() {
            "1" | "true" | "yes" => return true,
            "0" | "false" | "no" => return false,
            _ => {}
        }
    }

    // Check iTerm2 font profile
    if let Ok(font) = std::env::var("ITERM_PROFILE") {
        if font.to_lowercase().contains("nerd") {
            return true;
        }
    }

    // Kitty and WezTerm are commonly set up with Nerd Fonts
    if let Ok(prog) = std::env::var("TERM_PROGRAM") {
        let p = prog.to_lowercase();
        if p == "wezterm" {
            return true;
        }
    }

    // Ghostty sets TERM_PROGRAM=ghostty and commonly uses NF
    // but we can't be sure, so don't default to true.

    false
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            fold_bar_size: 1,
            vim_navigator: false,
            mouse: true,
            default_shell: None,
            auto_suspend_secs: 86400,
            terminal_title_format: Some("{session} - {workspace}".to_string()),
            projects_dirs: Vec::new(),
            show_project_hub_on_start: false,
            hub_layout: HubLayout::default(),
            nerd_fonts: NerdFontsOption::Auto.resolve(),
        }
    }
}

impl Behavior {
    /// Resolve project directories: use configured dirs if set, otherwise auto-detect.
    pub fn resolved_projects_dirs(&self) -> Vec<std::path::PathBuf> {
        if !self.projects_dirs.is_empty() {
            return self
                .projects_dirs
                .iter()
                .map(|s| {
                    let expanded = if let Some(rest) = s.strip_prefix('~') {
                        if let Ok(home) = std::env::var("HOME") {
                            format!("{}{}", home, rest)
                        } else {
                            s.clone()
                        }
                    } else {
                        s.clone()
                    };
                    std::path::PathBuf::from(expanded)
                })
                .filter(|p| p.is_dir())
                .collect();
        }

        // Auto-detect common project directories
        let home = match std::env::var("HOME") {
            Ok(h) => std::path::PathBuf::from(h),
            Err(_) => return Vec::new(),
        };

        let candidates = [
            "Developer",
            "Projects",
            "repos",
            "src",
            "code",
            "workspace",
            "work",
        ];

        candidates
            .iter()
            .map(|name| home.join(name))
            .filter(|p| p.is_dir())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PaneDecoration — per-process visual overrides
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct PaneDecoration {
    pub process: String,
    pub border_color: Color,
}

impl PaneDecoration {
    fn defaults() -> Vec<Self> {
        vec![
            // AI agents
            PaneDecoration { process: "claude".into(),        border_color: Color::Rgb(249, 115, 22) }, // orange
            PaneDecoration { process: "codex".into(),         border_color: Color::Rgb(16, 163, 127) }, // openai green
            PaneDecoration { process: "aider".into(),         border_color: Color::Rgb(0, 191, 255) },  // deep sky blue
            PaneDecoration { process: "goose".into(),         border_color: Color::Rgb(168, 85, 247) }, // purple
            PaneDecoration { process: "gemini".into(),        border_color: Color::Rgb(66, 133, 244) }, // google blue
            // Editors
            PaneDecoration { process: "nvim".into(),          border_color: Color::Rgb(86, 156, 48) },  // green
            PaneDecoration { process: "vim".into(),           border_color: Color::Rgb(86, 156, 48) },  // green
            PaneDecoration { process: "hx".into(),            border_color: Color::Rgb(168, 85, 247) }, // helix purple
            PaneDecoration { process: "emacs".into(),         border_color: Color::Rgb(126, 92, 171) }, // emacs purple
            // Monitors
            PaneDecoration { process: "htop".into(),          border_color: Color::Rgb(59, 130, 246) }, // blue
            PaneDecoration { process: "btop".into(),          border_color: Color::Rgb(59, 130, 246) }, // blue
            PaneDecoration { process: "btm".into(),           border_color: Color::Rgb(59, 130, 246) }, // blue
            // Git
            PaneDecoration { process: "lazygit".into(),       border_color: Color::Rgb(240, 80, 51) },  // git red-orange
            PaneDecoration { process: "gitui".into(),         border_color: Color::Rgb(240, 80, 51) },  // git red-orange
            // Kubernetes
            PaneDecoration { process: "k9s".into(),           border_color: Color::Rgb(50, 108, 229) }, // k8s blue
            PaneDecoration { process: "kdash".into(),         border_color: Color::Rgb(50, 108, 229) }, // k8s blue
            PaneDecoration { process: "kubectl".into(),       border_color: Color::Rgb(50, 108, 229) }, // k8s blue
            // Docker
            PaneDecoration { process: "lazydocker".into(),    border_color: Color::Rgb(29, 99, 237) },  // docker blue
            // Languages / runtimes
            PaneDecoration { process: "python3".into(),       border_color: Color::Rgb(250, 204, 21) }, // yellow
            PaneDecoration { process: "python".into(),        border_color: Color::Rgb(250, 204, 21) }, // yellow
            PaneDecoration { process: "node".into(),          border_color: Color::Rgb(74, 222, 128) }, // node green
            PaneDecoration { process: "bun".into(),           border_color: Color::Rgb(203, 178, 121) }, // bun tan
            PaneDecoration { process: "deno".into(),          border_color: Color::Rgb(112, 255, 175) }, // deno mint
            PaneDecoration { process: "ruby".into(),          border_color: Color::Rgb(204, 52, 45) },  // ruby red
            PaneDecoration { process: "irb".into(),           border_color: Color::Rgb(204, 52, 45) },  // ruby red
            // File managers
            PaneDecoration { process: "yazi".into(),          border_color: Color::Rgb(202, 158, 230) }, // lavender
            // SSH
            PaneDecoration { process: "ssh".into(),           border_color: Color::Rgb(148, 163, 184) }, // slate
        ]
    }
}

// ---------------------------------------------------------------------------
// StatusBarConfig
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct StatusBarConfig {
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_load: bool,
    pub show_disk: bool,
    pub update_interval_secs: u64,
    pub left: String,
    pub right: String,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            show_cpu: true,
            show_memory: true,
            show_load: true,
            show_disk: false,
            update_interval_secs: 3,
            left: "".to_string(),
            right: "#{cpu} #{mem} #{load}  ^⎵ normal  ⎵ leader ".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// KeyMap
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct KeyMap {
    map: HashMap<KeyEvent, Action>,
}

impl KeyMap {
    pub fn from_defaults() -> Self {
        let mut map = HashMap::new();

        // Merge data-driven global defaults
        for (key_str, action) in crate::default_keys::global_defaults() {
            if let Some(key) = parse_key(key_str) {
                map.insert(key, action);
            }
        }

        Self { map }
    }

    pub fn from_pairs(pairs: Vec<(&str, Action)>) -> Self {
        let mut map = HashMap::new();
        for (key_str, action) in pairs {
            if let Some(key) = parse_key(key_str) {
                map.insert(key, action);
            }
        }
        Self { map }
    }

    pub fn lookup(&self, key: &KeyEvent) -> Option<&Action> {
        self.map.get(key)
    }

    /// Build a reverse map: Action → Vec<KeyEvent> for display purposes.
    pub fn reverse_map(&self) -> HashMap<Action, Vec<KeyEvent>> {
        let mut reverse: HashMap<Action, Vec<KeyEvent>> = HashMap::new();
        for (key, action) in &self.map {
            reverse.entry(action.clone()).or_default().push(*key);
        }
        reverse
    }

    /// Apply user overrides: for each (name, key_str), parse both, remove the old
    /// binding for that action, and insert the new one.
    pub fn merge(&mut self, raw: &HashMap<String, String>) {
        // Build a name→action mapping for all known action names
        let name_to_action = action_name_map();

        for (name, key_str) in raw {
            let action = match name_to_action.get(name.as_str()) {
                Some(a) => a.clone(),
                None => continue,
            };
            let new_key = match parse_key(key_str) {
                Some(k) => k,
                None => continue,
            };

            // Remove any existing bindings for this action
            self.map.retain(|_, v| *v != action);
            self.map.insert(new_key, action);
        }
    }
}

fn action_name_map() -> HashMap<&'static str, Action> {
    let mut m: HashMap<&'static str, Action> = crate::registry::action_registry()
        .iter()
        .map(|meta| (meta.name, meta.action.clone()))
        .collect();
    // Aliases
    m.insert("next_tab_alt", Action::NextTab);
    m.insert("prev_tab_alt", Action::PrevTab);
    // Parameterized variants still need Box::leak
    for n in 1..=9u8 {
        let name: &'static str = Box::leak(format!("focus_group_{}", n).into_boxed_str());
        m.insert(name, Action::FocusGroupN(n));
        let ws_name: &'static str = Box::leak(format!("switch_workspace_{}", n).into_boxed_str());
        m.insert(ws_name, Action::SwitchWorkspace(n));
    }
    m
}

// ---------------------------------------------------------------------------
// Leader key
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum LeaderNode {
    Leaf {
        action: Action,
        label: String,
    },
    Group {
        label: String,
        children: HashMap<KeyEvent, LeaderNode>,
    },
    PassThrough,
}

#[derive(Clone, Debug)]
pub struct LeaderConfig {
    pub key: KeyEvent,
    pub timeout_ms: u64,
    pub root: LeaderNode,
}

impl Default for LeaderConfig {
    fn default() -> Self {
        Self {
            key: KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            timeout_ms: 300,
            root: default_leader_tree(),
        }
    }
}

fn default_leader_tree() -> LeaderNode {
    let mut root = HashMap::new();

    // \w → +Window
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "h", Action::FocusLeft, "Focus Left");
        insert_leaf(&mut children, "j", Action::FocusDown, "Focus Down");
        insert_leaf(&mut children, "k", Action::FocusUp, "Focus Up");
        insert_leaf(&mut children, "l", Action::FocusRight, "Focus Right");
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            children.insert(
                key,
                LeaderNode::Leaf {
                    action: Action::FocusGroupN(n),
                    label: format!("Focus {}", n),
                },
            );
        }
        insert_leaf(&mut children, "d", Action::SplitHorizontal, "Split H");
        insert_leaf(&mut children, "shift+d", Action::SplitVertical, "Split V");
        insert_leaf(&mut children, "c", Action::CloseTab, "Close");
        insert_leaf(&mut children, "=", Action::Equalize, "Equalize");
        insert_leaf(&mut children, "r", Action::RestartPane, "Restart");
        insert_leaf(&mut children, "m", Action::MaximizeFocused, "Maximize");
        insert_leaf(&mut children, "z", Action::ToggleZoom, "Zoom");
        insert_leaf(&mut children, "f", Action::ToggleFloat, "Float");
        insert_leaf(&mut children, "F", Action::NewFloat, "New Float");
        insert_leaf(&mut children, "n", Action::RenameWindow, "Rename");
        let key = parse_key("w").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Window".into(),
                children,
            },
        );
    }

    // \t → +Tab
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "n", Action::NewTab, "New Tab");
        insert_leaf(&mut children, "c", Action::CloseTab, "Close Tab");
        insert_leaf(&mut children, "]", Action::NextTab, "Next Tab");
        insert_leaf(&mut children, "[", Action::PrevTab, "Prev Tab");
        insert_leaf(&mut children, "h", Action::MoveTabLeft, "Move Left");
        insert_leaf(&mut children, "j", Action::MoveTabDown, "Move Down");
        insert_leaf(&mut children, "k", Action::MoveTabUp, "Move Up");
        insert_leaf(&mut children, "l", Action::MoveTabRight, "Move Right");
        let key = parse_key("t").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Tab".into(),
                children,
            },
        );
    }

    // \s → +Session
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "p", Action::CommandPalette, "Palette");
        insert_leaf(&mut children, "d", Action::Detach, "Detach");
        let key = parse_key("s").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Session".into(),
                children,
            },
        );
    }

    // \W → +Workspace
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "n", Action::NewWorkspace, "New");
        insert_leaf(&mut children, "c", Action::CloseWorkspace, "Close");
        insert_leaf(&mut children, "r", Action::RenameWorkspace, "Rename");
        insert_leaf(&mut children, "p", Action::ProjectHub, "Projects");
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            children.insert(
                key,
                LeaderNode::Leaf {
                    action: Action::SwitchWorkspace(n),
                    label: format!("Switch {}", n),
                },
            );
        }
        let key = parse_key("shift+w").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Workspace".into(),
                children,
            },
        );
    }

    // \r → Resize mode
    insert_leaf(&mut root, "r", Action::ResizeMode, "Resize");

    // Quick splits at root level (2-keystroke access)
    insert_leaf(&mut root, "d", Action::SplitHorizontal, "Split H");
    insert_leaf(&mut root, "shift+d", Action::SplitVertical, "Split V");

    // \y → Paste
    insert_leaf(&mut root, "y", Action::PasteClipboard, "Paste");
    // \/ → Help
    insert_leaf(&mut root, "/", Action::Help, "Help");
    // \q → Quit
    insert_leaf(&mut root, "q", Action::Quit, "Quit");

    // space space → Command palette
    let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
    root.insert(
        space_key,
        LeaderNode::Leaf {
            action: Action::CommandPalette,
            label: "Command Palette".into(),
        },
    );

    LeaderNode::Group {
        label: "Leader".into(),
        children: root,
    }
}

fn insert_leaf(
    map: &mut HashMap<KeyEvent, LeaderNode>,
    key_str: &str,
    action: Action,
    label: &str,
) {
    if let Some(key) = parse_key(key_str) {
        map.insert(
            key,
            LeaderNode::Leaf {
                action,
                label: label.to_string(),
            },
        );
    }
}

/// Build a leader tree from TOML config, merging on top of defaults.
fn build_leader_tree(raw_keys: &HashMap<String, String>, defaults: LeaderNode) -> LeaderNode {
    let name_to_action = action_name_map();
    let mut root = defaults;

    for (key_path, value) in raw_keys {
        let segments: Vec<&str> = key_path.split_whitespace().collect();
        let parsed_keys: Vec<KeyEvent> = segments.iter().filter_map(|s| parse_key(s)).collect();
        if parsed_keys.len() != segments.len() || parsed_keys.is_empty() {
            continue;
        }

        if value == "passthrough" {
            insert_into_tree(&mut root, &parsed_keys, LeaderNode::PassThrough);
        } else if let Some(stripped) = value.strip_prefix('+') {
            // Group label — ensure the group exists
            if parsed_keys.len() == 1 {
                let existing = get_or_create_group(&mut root, &parsed_keys[0], stripped);
                let _ = existing; // just ensure it exists
            }
        } else if let Some(action) = name_to_action.get(value.as_str()) {
            let label = value.replace('_', " ");
            let node = LeaderNode::Leaf {
                action: action.clone(),
                label,
            };
            insert_into_tree(&mut root, &parsed_keys, node);
        }
    }

    root
}

fn get_or_create_group<'a>(
    tree: &'a mut LeaderNode,
    key: &KeyEvent,
    label: &str,
) -> &'a mut HashMap<KeyEvent, LeaderNode> {
    if let LeaderNode::Group { children, .. } = tree {
        children.entry(*key).or_insert_with(|| LeaderNode::Group {
            label: label.to_string(),
            children: HashMap::new(),
        });
        if let Some(LeaderNode::Group {
            children: inner, ..
        }) = children.get_mut(key)
        {
            return inner;
        }
    }
    // Fallback — shouldn't happen if root is always a Group
    panic!("root must be a Group");
}

fn insert_into_tree(tree: &mut LeaderNode, keys: &[KeyEvent], node: LeaderNode) {
    if keys.is_empty() {
        return;
    }
    if keys.len() == 1 {
        if let LeaderNode::Group { children, .. } = tree {
            children.insert(keys[0], node);
        }
        return;
    }
    // Descend
    if let LeaderNode::Group { children, .. } = tree {
        let child = children
            .entry(keys[0])
            .or_insert_with(|| LeaderNode::Group {
                label: String::new(),
                children: HashMap::new(),
            });
        insert_into_tree(child, &keys[1..], node);
    }
}

// ---------------------------------------------------------------------------
// Config (top-level)
// ---------------------------------------------------------------------------

/// A custom entry for the tab picker, defined in config.
#[derive(Clone, Debug)]
pub struct TabPickerEntryConfig {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    /// Shell to run the command in (e.g. "/bin/zsh"). When set, the command is
    /// executed as `shell -c "command"` instead of being run directly.
    pub shell: Option<String>,
    /// Category to place this entry in (e.g. "editors", "agents", "repls",
    /// "system", "cluster"). Defaults to "other" if omitted.
    pub category: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub theme: Theme,
    pub behavior: Behavior,
    pub keys: KeyMap,
    pub normal_keys: KeyMap,
    pub status_bar: StatusBarConfig,
    pub decorations: Vec<PaneDecoration>,
    pub leader: LeaderConfig,
    pub plugins: Vec<crate::plugin::PluginConfig>,
    pub tab_picker_entries: Vec<TabPickerEntryConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            behavior: Behavior::default(),
            keys: KeyMap::from_defaults(),
            normal_keys: KeyMap::from_pairs(crate::default_keys::normal_defaults()),
            status_bar: StatusBarConfig::default(),
            decorations: PaneDecoration::defaults(),
            leader: LeaderConfig::default(),
            plugins: Vec::new(),
            tab_picker_entries: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = dirs::config_dir()
            .map(|d| d.join("pane").join("config.toml"))
            .unwrap_or_default();

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                let mut c = Self::default();
                c.adjust_for_terminal();
                return c;
            }
        };

        let raw: RawConfig = match toml::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pane: invalid config at {}: {}", path.display(), e);
                let mut c = Self::default();
                c.adjust_for_terminal();
                return c;
            }
        };

        Self::from_raw(raw)
    }

    /// Adjust theme defaults based on terminal light/dark detection.
    /// Only changes values that weren't explicitly set in the config.
    fn adjust_for_terminal(&mut self) {
        if is_light_terminal() {
            self.theme.border_inactive = Theme::BORDER_INACTIVE_LIGHT;
        }
    }

    pub fn decoration_for(&self, process: &str) -> Option<&PaneDecoration> {
        self.decorations.iter().find(|d| d.process == process)
    }

    /// Match a decoration by checking if any decoration's process name appears
    /// as a component in the executable path. This handles cases where the binary
    /// filename is not recognizable (e.g. a version string) but the install path
    /// contains the program name (e.g. `~/.local/share/claude/versions/2.1.74`).
    pub fn decoration_for_path(&self, path: &str) -> Option<&PaneDecoration> {
        let p = std::path::Path::new(path);
        self.decorations.iter().find(|d| {
            p.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .is_some_and(|s| s == d.process)
            })
        })
    }

    fn from_raw(raw: RawConfig) -> Self {
        let mut config = Self::default();

        // Theme
        let mut border_inactive_explicit = false;
        if let Some(t) = raw.theme {
            // Load preset as base if specified
            let has_preset = if let Some(ref preset_name) = t.preset {
                if let Some(preset_theme) = Theme::preset(preset_name) {
                    config.theme = preset_theme;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            // Apply per-field overrides on top of preset (or default)
            if let Some(s) = t.accent {
                if let Some(c) = parse_color(&s) {
                    config.theme.accent = c;
                }
            }
            if let Some(s) = t.border_inactive {
                if let Some(c) = parse_color(&s) {
                    config.theme.border_inactive = c;
                    border_inactive_explicit = true;
                }
            }
            // Auto-detect light terminal for default theme when not explicitly set
            if !border_inactive_explicit && !has_preset {
                config.adjust_for_terminal();
            }
            if let Some(s) = t.bg {
                if let Some(c) = parse_color(&s) {
                    config.theme.bg = c;
                }
            }
            if let Some(s) = t.fg {
                if let Some(c) = parse_color(&s) {
                    config.theme.fg = c;
                }
            }
            if let Some(s) = t.dim {
                if let Some(c) = parse_color(&s) {
                    config.theme.dim = c;
                }
            }
            if let Some(s) = t.tab_active {
                if let Some(c) = parse_color(&s) {
                    config.theme.tab_active = c;
                }
            }
            if let Some(s) = t.tab_inactive {
                if let Some(c) = parse_color(&s) {
                    config.theme.tab_inactive = c;
                }
            }
        } else {
            // No [theme] section — apply terminal detection on default theme
            config.adjust_for_terminal();
        }

        // Behavior
        if let Some(b) = raw.behavior {
            if let Some(v) = b.fold_bar_size {
                config.behavior.fold_bar_size = v;
            }
            if let Some(v) = b.vim_navigator {
                config.behavior.vim_navigator = v;
            }
            if let Some(v) = b.mouse {
                config.behavior.mouse = v;
            }
            if b.default_shell.is_some() {
                config.behavior.default_shell = b.default_shell;
            }
            if let Some(v) = b.auto_suspend_secs {
                config.behavior.auto_suspend_secs = v;
            }
            if b.terminal_title_format.is_some() {
                config.behavior.terminal_title_format = b.terminal_title_format;
            }
            if let Some(dirs) = b.projects_dirs {
                config.behavior.projects_dirs = dirs;
            }
            if let Some(v) = b.show_project_hub_on_start {
                config.behavior.show_project_hub_on_start = v;
            }
            if let Some(raw_layout) = b.hub_layout {
                let rows: Vec<Vec<HubWidget>> = raw_layout
                    .into_iter()
                    .filter_map(|row| {
                        let widgets: Vec<HubWidget> = row
                            .iter()
                            .filter_map(|s| HubWidget::from_str(s))
                            .collect();
                        if widgets.is_empty() {
                            None
                        } else {
                            Some(widgets)
                        }
                    })
                    .collect();
                if !rows.is_empty() {
                    config.behavior.hub_layout = HubLayout { rows };
                }
            }
            if let Some(v) = b.nerd_fonts {
                config.behavior.nerd_fonts = v.resolve();
            }
        }

        // Keys
        if let Some(keys) = raw.keys {
            config.keys.merge(&keys);
        }

        // Normal keys
        if let Some(normal_keys) = raw.normal_keys {
            config.normal_keys.merge(&normal_keys);
        }

        // Status bar
        if let Some(sb) = raw.status_bar {
            if let Some(v) = sb.show_cpu {
                config.status_bar.show_cpu = v;
            }
            if let Some(v) = sb.show_memory {
                config.status_bar.show_memory = v;
            }
            if let Some(v) = sb.show_load {
                config.status_bar.show_load = v;
            }
            if let Some(v) = sb.show_disk {
                config.status_bar.show_disk = v;
            }
            if let Some(v) = sb.update_interval_secs {
                config.status_bar.update_interval_secs = v;
            }
            if let Some(v) = sb.left {
                config.status_bar.left = v;
            }
            if let Some(v) = sb.right {
                config.status_bar.right = v;
            }
        }

        // Leader
        if let Some(ref leader) = raw.leader {
            if let Some(ref key_str) = leader.key {
                if let Some(k) = parse_key(key_str) {
                    config.leader.key = k;
                }
            }
            if let Some(ms) = leader.timeout_ms {
                config.leader.timeout_ms = ms;
            }
        }
        if let Some(ref leader_keys) = raw.leader_keys {
            config.leader.root = build_leader_tree(leader_keys, config.leader.root);
        }

        // Decorations
        if let Some(raw_decs) = raw.decorations {
            // User-provided decorations replace the defaults
            let mut decs = Vec::new();
            for rd in raw_decs {
                if let Some(color) = rd.border_color.and_then(|s| parse_color(&s)) {
                    decs.push(PaneDecoration {
                        process: rd.process,
                        border_color: color,
                    });
                }
            }
            if !decs.is_empty() {
                config.decorations = decs;
            }
        }

        // Tab picker entries
        if let Some(entries) = raw.tab_picker_entries {
            config.tab_picker_entries = entries
                .into_iter()
                .map(|e| TabPickerEntryConfig {
                    name: e.name,
                    command: e.command,
                    description: e.description,
                    shell: e.shell,
                    category: e.category,
                })
                .collect();
        }

        // Plugins
        if let Some(raw_plugins) = raw.plugins {
            config.plugins = raw_plugins
                .into_iter()
                .map(|rp| crate::plugin::PluginConfig {
                    command: rp.command,
                    events: if rp.events.is_empty() {
                        vec!["*".to_string()]
                    } else {
                        rp.events
                    },
                    refresh_interval_secs: rp.refresh_interval_secs,
                })
                .collect();
        }

        config
    }
}

// ---------------------------------------------------------------------------
// Raw TOML structs (all-optional for merge)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
struct RawConfig {
    theme: Option<RawTheme>,
    behavior: Option<RawBehavior>,
    keys: Option<HashMap<String, String>>,
    normal_keys: Option<HashMap<String, String>>,
    status_bar: Option<RawStatusBar>,
    decorations: Option<Vec<RawDecoration>>,
    leader: Option<RawLeader>,
    leader_keys: Option<HashMap<String, String>>,
    plugins: Option<Vec<RawPlugin>>,
    tab_picker_entries: Option<Vec<RawTabPickerEntry>>,
}

#[derive(Deserialize, Default)]
struct RawPlugin {
    command: String,
    #[serde(default)]
    events: Vec<String>,
    #[serde(default)]
    refresh_interval_secs: u64,
}

#[derive(Deserialize, Default)]
struct RawTabPickerEntry {
    name: String,
    command: String,
    description: Option<String>,
    shell: Option<String>,
    category: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawLeader {
    key: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
struct RawDecoration {
    process: String,
    border_color: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawTheme {
    preset: Option<String>,
    accent: Option<String>,
    border_inactive: Option<String>,
    bg: Option<String>,
    fg: Option<String>,
    dim: Option<String>,
    tab_active: Option<String>,
    tab_inactive: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawBehavior {
    #[allow(dead_code)]
    min_pane_width: Option<u16>,
    #[allow(dead_code)]
    min_pane_height: Option<u16>,
    fold_bar_size: Option<u16>,
    vim_navigator: Option<bool>,
    mouse: Option<bool>,
    default_shell: Option<String>,
    auto_suspend_secs: Option<u64>,
    terminal_title_format: Option<String>,
    projects_dirs: Option<Vec<String>>,
    show_project_hub_on_start: Option<bool>,
    /// Widget layout: array of arrays of widget names.
    /// e.g. `[["project_info"], ["recent_commits", "changed_files"]]`
    hub_layout: Option<Vec<Vec<String>>>,
    #[serde(default, deserialize_with = "deserialize_nerd_fonts")]
    nerd_fonts: Option<NerdFontsOption>,
}

#[derive(Deserialize, Default)]
struct RawStatusBar {
    show_cpu: Option<bool>,
    show_memory: Option<bool>,
    show_load: Option<bool>,
    show_disk: Option<bool>,
    update_interval_secs: Option<u64>,
    left: Option<String>,
    right: Option<String>,
}

// ---------------------------------------------------------------------------
// nerd_fonts deserializer: accepts true | false | "auto"
// ---------------------------------------------------------------------------

fn deserialize_nerd_fonts<'de, D>(deserializer: D) -> Result<Option<NerdFontsOption>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct NerdFontsVisitor;
    impl<'de> de::Visitor<'de> for NerdFontsVisitor {
        type Value = Option<NerdFontsOption>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("true, false, or \"auto\"")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
            Ok(Some(if v { NerdFontsOption::On } else { NerdFontsOption::Off }))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            match v.to_lowercase().as_str() {
                "auto" => Ok(Some(NerdFontsOption::Auto)),
                "true" | "yes" | "1" => Ok(Some(NerdFontsOption::On)),
                "false" | "no" | "0" => Ok(Some(NerdFontsOption::Off)),
                _ => Err(de::Error::custom(format!(
                    "invalid nerd_fonts value: {v:?} (expected true, false, or \"auto\")"
                ))),
            }
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(NerdFontsVisitor)
}

// ---------------------------------------------------------------------------
// parse_key: "ctrl+shift+d" → crossterm KeyEvent
// ---------------------------------------------------------------------------

pub fn parse_key(s: &str) -> Option<KeyEvent> {
    let s = s.trim().to_lowercase();
    let parts: Vec<&str> = s.split('+').collect();

    let mut mods = KeyModifiers::NONE;
    let mut key_part = "";

    for part in &parts {
        match *part {
            "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
            "alt" | "option" => mods |= KeyModifiers::ALT,
            "shift" => mods |= KeyModifiers::SHIFT,
            _ => key_part = part,
        }
    }

    let code = match key_part {
        "tab" if mods.contains(KeyModifiers::SHIFT) => {
            mods -= KeyModifiers::SHIFT;
            KeyCode::BackTab
        }
        "tab" => KeyCode::Tab,
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "space" => KeyCode::Char(' '),
        s if s.starts_with('f') && s.len() >= 2 => {
            if let Ok(n) = s[1..].parse::<u8>() {
                if (1..=12).contains(&n) {
                    KeyCode::F(n)
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        s if s.len() == 1 => {
            let ch = s.chars().next().unwrap();
            if mods.contains(KeyModifiers::SHIFT) && ch.is_ascii_alphabetic() {
                mods -= KeyModifiers::SHIFT;
                KeyCode::Char(ch.to_ascii_uppercase())
            } else {
                KeyCode::Char(ch)
            }
        }
        _ => return None,
    };

    Some(KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

// ---------------------------------------------------------------------------
// normalize_key: strip kind/state for consistent HashMap matching
// ---------------------------------------------------------------------------

pub fn normalize_key(key: KeyEvent) -> KeyEvent {
    KeyEvent {
        code: key.code,
        modifiers: key.modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

// ---------------------------------------------------------------------------
// parse_color: "cyan", "dark_gray", "#ff0000", "#f00", "reset"
// ---------------------------------------------------------------------------

pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim().to_lowercase();

    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::Rgb(r, g, b))
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Color::Rgb(r, g, b))
            }
            _ => None,
        };
    }

    match s.as_str() {
        "reset" => Some(Color::Reset),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "white" => Some(Color::White),
        "dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => Some(Color::DarkGray),
        // dark variants: use indexed ANSI colors (0-7 are the "dark" set)
        "dark_red" | "darkred" => Some(Color::Indexed(1)),
        "dark_green" | "darkgreen" => Some(Color::Indexed(2)),
        "dark_yellow" | "darkyellow" => Some(Color::Indexed(3)),
        "dark_blue" | "darkblue" => Some(Color::Indexed(4)),
        "dark_magenta" | "darkmagenta" => Some(Color::Indexed(5)),
        "dark_cyan" | "darkcyan" => Some(Color::Indexed(6)),
        "light_red" | "lightred" => Some(Color::LightRed),
        "light_green" | "lightgreen" => Some(Color::LightGreen),
        "light_yellow" | "lightyellow" => Some(Color::LightYellow),
        "light_blue" | "lightblue" => Some(Color::LightBlue),
        "light_magenta" | "lightmagenta" => Some(Color::LightMagenta),
        "light_cyan" | "lightcyan" => Some(Color::LightCyan),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    // --- parse_key ---

    #[test]
    fn test_parse_key_ctrl_q() {
        assert_eq!(
            parse_key("ctrl+q"),
            Some(make_key(KeyCode::Char('q'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn test_parse_key_ctrl_shift_d() {
        // ctrl+shift+d → uppercase D, no SHIFT modifier
        assert_eq!(
            parse_key("ctrl+shift+d"),
            Some(make_key(KeyCode::Char('D'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn test_parse_key_alt_bracket() {
        assert_eq!(
            parse_key("alt+]"),
            Some(make_key(KeyCode::Char(']'), KeyModifiers::ALT))
        );
    }

    #[test]
    fn test_parse_key_ctrl_tab() {
        assert_eq!(
            parse_key("ctrl+tab"),
            Some(make_key(KeyCode::Tab, KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn test_parse_key_ctrl_shift_tab_backtab() {
        // ctrl+shift+tab → BackTab with CONTROL
        assert_eq!(
            parse_key("ctrl+shift+tab"),
            Some(make_key(KeyCode::BackTab, KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn test_parse_key_shift_pageup() {
        assert_eq!(
            parse_key("shift+pageup"),
            Some(make_key(KeyCode::PageUp, KeyModifiers::SHIFT))
        );
    }

    #[test]
    fn test_parse_key_ctrl_alt_equals() {
        assert_eq!(
            parse_key("ctrl+alt+="),
            Some(make_key(
                KeyCode::Char('='),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            ))
        );
    }

    #[test]
    fn test_parse_key_case_insensitive() {
        assert_eq!(
            parse_key("Ctrl+Q"),
            Some(make_key(KeyCode::Char('q'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn test_parse_key_invalid() {
        assert_eq!(parse_key(""), None);
        assert_eq!(parse_key("ctrl+"), None);
    }

    #[test]
    fn test_parse_key_alt_shift_h() {
        assert_eq!(
            parse_key("alt+shift+h"),
            Some(make_key(KeyCode::Char('H'), KeyModifiers::ALT))
        );
    }

    #[test]
    fn test_parse_key_ctrl_alt_h() {
        assert_eq!(
            parse_key("ctrl+alt+h"),
            Some(make_key(
                KeyCode::Char('h'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            ))
        );
    }

    // --- parse_color ---

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("cyan"), Some(Color::Cyan));
        assert_eq!(parse_color("red"), Some(Color::Red));
        assert_eq!(parse_color("white"), Some(Color::White));
    }

    #[test]
    fn test_parse_color_dark_gray_variants() {
        assert_eq!(parse_color("dark_gray"), Some(Color::DarkGray));
        assert_eq!(parse_color("dark_grey"), Some(Color::DarkGray));
        assert_eq!(parse_color("darkgray"), Some(Color::DarkGray));
    }

    #[test]
    fn test_parse_color_hex_6() {
        assert_eq!(parse_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("#282828"), Some(Color::Rgb(40, 40, 40)));
    }

    #[test]
    fn test_parse_color_hex_3() {
        assert_eq!(parse_color("#f00"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_color("#0f0"), Some(Color::Rgb(0, 255, 0)));
    }

    #[test]
    fn test_parse_color_reset() {
        assert_eq!(parse_color("reset"), Some(Color::Reset));
    }

    #[test]
    fn test_parse_color_invalid() {
        assert_eq!(parse_color("nope"), None);
        assert_eq!(parse_color("#gggggg"), None);
    }

    // --- KeyMap ---

    #[test]
    fn test_keymap_defaults_quit_not_global() {
        let km = KeyMap::from_defaults();
        // Quit is leader-only now, not in the global keymap
        let key = make_key(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&key), None);
    }

    #[test]
    fn test_keymap_defaults_no_old_binds() {
        let km = KeyMap::from_defaults();
        // Old ctrl/alt combos removed — now handled via leader key
        let key = make_key(KeyCode::Char('3'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&key), None);
        let key = make_key(KeyCode::Char('5'), KeyModifiers::ALT);
        assert_eq!(km.lookup(&key), None);
    }

    #[test]
    fn test_keymap_merge_override() {
        let mut km = KeyMap::from_defaults();
        let mut overrides = HashMap::new();
        overrides.insert("scroll_mode".to_string(), "ctrl+x".to_string());
        km.merge(&overrides);

        // New binding present
        let new_key = make_key(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&new_key), Some(&Action::ScrollMode));
    }

    // --- Config::from_raw ---

    #[test]
    fn test_config_from_empty_raw() {
        let raw = RawConfig::default();
        let config = Config::from_raw(raw);
        // Should be identical to default
        assert_eq!(config.theme.accent, Color::Cyan);
        assert_eq!(config.behavior.fold_bar_size, 1);
        assert!(config.status_bar.show_cpu);
    }

    #[test]
    fn test_config_from_partial_toml() {
        let toml_str = r#"
[theme]
accent = "green"

[behavior]
min_pane_width = 80
"#;
        let raw: RawConfig = toml::from_str(toml_str).unwrap();
        let config = Config::from_raw(raw);
        assert_eq!(config.theme.accent, Color::Green);
        // Unchanged defaults — from_raw applies terminal detection, so compare
        // against what load() would produce rather than Theme::default()
        let expected = if is_light_terminal() {
            Theme::BORDER_INACTIVE_LIGHT
        } else {
            Color::Rgb(70, 70, 70)
        };
        assert_eq!(config.theme.border_inactive, expected);
        assert_eq!(config.behavior.fold_bar_size, 1);
    }

    // --- LeaderConfig ---

    #[test]
    fn test_leader_config_default_key_and_timeout() {
        let leader = LeaderConfig::default();
        assert_eq!(
            leader.key,
            make_key(KeyCode::Char(' '), KeyModifiers::NONE)
        );
        assert_eq!(leader.timeout_ms, 300);
    }

    #[test]
    fn test_leader_config_default_root_is_group() {
        let leader = LeaderConfig::default();
        match &leader.root {
            LeaderNode::Group { label, .. } => assert_eq!(label, "Leader"),
            _ => panic!("root should be a Group"),
        }
    }

    // --- default_leader_tree ---

    #[test]
    fn test_default_leader_tree_has_window_group() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let w_key = parse_key("w").unwrap();
        match children.get(&w_key) {
            Some(LeaderNode::Group { label, children }) => {
                assert_eq!(label, "Window");
                // Should have h/j/k/l, 1-9, d, D, c, =, r
                let h_key = parse_key("h").unwrap();
                assert!(children.contains_key(&h_key));
                let d_key = parse_key("d").unwrap();
                assert!(children.contains_key(&d_key));
                // FocusGroupN(5)
                let five_key = KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE);
                match children.get(&five_key) {
                    Some(LeaderNode::Leaf { action, .. }) => {
                        assert_eq!(*action, Action::FocusGroupN(5));
                    }
                    _ => panic!("expected FocusGroupN(5) leaf"),
                }
            }
            _ => panic!("expected Window group at 'w'"),
        }
    }

    #[test]
    fn test_default_leader_tree_has_tab_group() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let t_key = parse_key("t").unwrap();
        match children.get(&t_key) {
            Some(LeaderNode::Group { label, children }) => {
                assert_eq!(label, "Tab");
                let n_key = parse_key("n").unwrap();
                match children.get(&n_key) {
                    Some(LeaderNode::Leaf { action, .. }) => {
                        assert_eq!(*action, Action::NewTab);
                    }
                    _ => panic!("expected NewTab leaf"),
                }
            }
            _ => panic!("expected Tab group at 't'"),
        }
    }

    #[test]
    fn test_default_leader_tree_has_session_group() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let s_key = parse_key("s").unwrap();
        match children.get(&s_key) {
            Some(LeaderNode::Group { label, .. }) => assert_eq!(label, "Session"),
            _ => panic!("expected Session group at 's'"),
        }
    }

    #[test]
    fn test_default_leader_tree_has_workspace_group() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let w_key = parse_key("shift+w").unwrap();
        match children.get(&w_key) {
            Some(LeaderNode::Group { label, children }) => {
                assert_eq!(label, "Workspace");
                let n_key = parse_key("n").unwrap();
                assert!(children.contains_key(&n_key));
                // SwitchWorkspace(3)
                let three_key = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
                match children.get(&three_key) {
                    Some(LeaderNode::Leaf { action, .. }) => {
                        assert_eq!(*action, Action::SwitchWorkspace(3));
                    }
                    _ => panic!("expected SwitchWorkspace(3) leaf"),
                }
            }
            _ => panic!("expected Workspace group at 'W'"),
        }
    }

    #[test]
    fn test_default_leader_tree_has_resize_mode() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let r_key = parse_key("r").unwrap();
        match children.get(&r_key) {
            Some(LeaderNode::Leaf { action, .. }) => assert_eq!(*action, Action::ResizeMode),
            _ => panic!("expected ResizeMode leaf at 'r'"),
        }
    }

    #[test]
    fn test_default_leader_tree_has_paste_and_help() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let y_key = parse_key("y").unwrap();
        match children.get(&y_key) {
            Some(LeaderNode::Leaf { action, .. }) => assert_eq!(*action, Action::PasteClipboard),
            _ => panic!("expected Paste leaf at 'y'"),
        }
        let slash_key = parse_key("/").unwrap();
        match children.get(&slash_key) {
            Some(LeaderNode::Leaf { action, .. }) => assert_eq!(*action, Action::Help),
            _ => panic!("expected Help leaf at '/'"),
        }
    }

    #[test]
    fn test_default_leader_tree_has_command_palette() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let space_key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        match children.get(&space_key) {
            Some(LeaderNode::Leaf { action, .. }) => {
                assert_eq!(*action, Action::CommandPalette)
            }
            _ => panic!("expected CommandPalette leaf at space"),
        }
    }

    // --- build_leader_tree ---

    #[test]
    fn test_build_leader_tree_add_custom_leaf() {
        let defaults = default_leader_tree();
        let mut raw = HashMap::new();
        raw.insert("x".to_string(), "quit".to_string());
        let tree = build_leader_tree(&raw, defaults);
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let x_key = parse_key("x").unwrap();
        match children.get(&x_key) {
            Some(LeaderNode::Leaf { action, label }) => {
                assert_eq!(*action, Action::Quit);
                assert_eq!(label, "quit");
            }
            _ => panic!("expected Quit leaf at 'x'"),
        }
    }

    #[test]
    fn test_build_leader_tree_passthrough_value() {
        let defaults = default_leader_tree();
        let mut raw = HashMap::new();
        raw.insert("z".to_string(), "passthrough".to_string());
        let tree = build_leader_tree(&raw, defaults);
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let z_key = parse_key("z").unwrap();
        match children.get(&z_key) {
            Some(LeaderNode::PassThrough) => {}
            _ => panic!("expected PassThrough at 'z'"),
        }
    }

    #[test]
    fn test_build_leader_tree_group_label() {
        let defaults = default_leader_tree();
        let mut raw = HashMap::new();
        raw.insert("g".to_string(), "+Custom".to_string());
        let tree = build_leader_tree(&raw, defaults);
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let g_key = parse_key("g").unwrap();
        match children.get(&g_key) {
            Some(LeaderNode::Group { label, .. }) => assert_eq!(label, "Custom"),
            _ => panic!("expected Custom group at 'g'"),
        }
    }

    #[test]
    fn test_build_leader_tree_nested_path() {
        let defaults = default_leader_tree();
        let mut raw = HashMap::new();
        raw.insert("g q".to_string(), "quit".to_string());
        let tree = build_leader_tree(&raw, defaults);
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let g_key = parse_key("g").unwrap();
        match children.get(&g_key) {
            Some(LeaderNode::Group { children, .. }) => {
                let q_key = parse_key("q").unwrap();
                match children.get(&q_key) {
                    Some(LeaderNode::Leaf { action, .. }) => assert_eq!(*action, Action::Quit),
                    _ => panic!("expected Quit leaf at 'g q'"),
                }
            }
            _ => panic!("expected group at 'g'"),
        }
    }

    #[test]
    fn test_build_leader_tree_invalid_action_ignored() {
        let defaults = default_leader_tree();
        let mut raw = HashMap::new();
        raw.insert("x".to_string(), "nonexistent_action".to_string());
        let tree = build_leader_tree(&raw, defaults);
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let x_key = parse_key("x").unwrap();
        assert!(children.get(&x_key).is_none());
    }

    #[test]
    fn test_build_leader_tree_invalid_key_ignored() {
        let defaults = default_leader_tree();
        let mut raw = HashMap::new();
        raw.insert("".to_string(), "quit".to_string());
        let tree = build_leader_tree(&raw, defaults);
        // Should still be valid — just no new binding added
        match &tree {
            LeaderNode::Group { .. } => {}
            _ => panic!("root should be a Group"),
        }
    }

    // --- insert_into_tree ---

    #[test]
    fn test_insert_into_tree_single_key() {
        let mut tree = LeaderNode::Group {
            label: "root".into(),
            children: HashMap::new(),
        };
        let key = parse_key("a").unwrap();
        let node = LeaderNode::Leaf {
            action: Action::Quit,
            label: "Quit".into(),
        };
        insert_into_tree(&mut tree, &[key], node);
        match &tree {
            LeaderNode::Group { children, .. } => {
                assert!(children.contains_key(&key));
            }
            _ => panic!("root should be a Group"),
        }
    }

    #[test]
    fn test_insert_into_tree_nested_creates_intermediate_group() {
        let mut tree = LeaderNode::Group {
            label: "root".into(),
            children: HashMap::new(),
        };
        let key_a = parse_key("a").unwrap();
        let key_b = parse_key("b").unwrap();
        let node = LeaderNode::Leaf {
            action: Action::Help,
            label: "Help".into(),
        };
        insert_into_tree(&mut tree, &[key_a, key_b], node);
        match &tree {
            LeaderNode::Group { children, .. } => match children.get(&key_a) {
                Some(LeaderNode::Group {
                    children: inner, ..
                }) => match inner.get(&key_b) {
                    Some(LeaderNode::Leaf { action, .. }) => {
                        assert_eq!(*action, Action::Help);
                    }
                    _ => panic!("expected Help leaf"),
                },
                _ => panic!("expected intermediate group"),
            },
            _ => panic!("root should be a Group"),
        }
    }

    #[test]
    fn test_insert_into_tree_empty_keys_noop() {
        let mut tree = LeaderNode::Group {
            label: "root".into(),
            children: HashMap::new(),
        };
        insert_into_tree(&mut tree, &[], LeaderNode::PassThrough);
        match &tree {
            LeaderNode::Group { children, .. } => assert!(children.is_empty()),
            _ => panic!("root should be a Group"),
        }
    }

    // --- get_or_create_group ---

    #[test]
    fn test_get_or_create_group_new() {
        let mut tree = LeaderNode::Group {
            label: "root".into(),
            children: HashMap::new(),
        };
        let key = parse_key("g").unwrap();
        let group = get_or_create_group(&mut tree, &key, "Custom");
        assert!(group.is_empty());
        // Verify the group was actually created
        match &tree {
            LeaderNode::Group { children, .. } => match children.get(&key) {
                Some(LeaderNode::Group { label, .. }) => assert_eq!(label, "Custom"),
                _ => panic!("expected Custom group"),
            },
            _ => panic!("root should be a Group"),
        }
    }

    #[test]
    fn test_get_or_create_group_existing() {
        let mut inner = HashMap::new();
        let q_key = parse_key("q").unwrap();
        inner.insert(
            q_key,
            LeaderNode::Leaf {
                action: Action::Quit,
                label: "Quit".into(),
            },
        );
        let g_key = parse_key("g").unwrap();
        let mut root_children = HashMap::new();
        root_children.insert(
            g_key,
            LeaderNode::Group {
                label: "Existing".into(),
                children: inner,
            },
        );
        let mut tree = LeaderNode::Group {
            label: "root".into(),
            children: root_children,
        };
        let group = get_or_create_group(&mut tree, &g_key, "Ignored");
        // Should return existing group contents (with Quit inside)
        assert!(group.contains_key(&q_key));
        // Label should remain "Existing", not overwritten
        match &tree {
            LeaderNode::Group { children, .. } => match children.get(&g_key) {
                Some(LeaderNode::Group { label, .. }) => assert_eq!(label, "Existing"),
                _ => panic!("expected group"),
            },
            _ => panic!("root should be a Group"),
        }
    }

    // --- Dark ANSI color parsing ---

    #[test]
    fn test_parse_color_dark_red() {
        assert_eq!(parse_color("dark_red"), Some(Color::Indexed(1)));
        assert_eq!(parse_color("darkred"), Some(Color::Indexed(1)));
    }

    #[test]
    fn test_parse_color_dark_green() {
        assert_eq!(parse_color("dark_green"), Some(Color::Indexed(2)));
        assert_eq!(parse_color("darkgreen"), Some(Color::Indexed(2)));
    }

    #[test]
    fn test_parse_color_dark_yellow() {
        assert_eq!(parse_color("dark_yellow"), Some(Color::Indexed(3)));
        assert_eq!(parse_color("darkyellow"), Some(Color::Indexed(3)));
    }

    #[test]
    fn test_parse_color_dark_blue() {
        assert_eq!(parse_color("dark_blue"), Some(Color::Indexed(4)));
        assert_eq!(parse_color("darkblue"), Some(Color::Indexed(4)));
    }

    #[test]
    fn test_parse_color_dark_magenta() {
        assert_eq!(parse_color("dark_magenta"), Some(Color::Indexed(5)));
        assert_eq!(parse_color("darkmagenta"), Some(Color::Indexed(5)));
    }

    #[test]
    fn test_parse_color_dark_cyan() {
        assert_eq!(parse_color("dark_cyan"), Some(Color::Indexed(6)));
        assert_eq!(parse_color("darkcyan"), Some(Color::Indexed(6)));
    }

    // --- decoration_for ---

    #[test]
    fn test_decoration_for_matching_process() {
        let config = Config::default();
        let dec = config.decoration_for("claude");
        assert!(dec.is_some());
        assert_eq!(dec.unwrap().border_color, Color::Rgb(249, 115, 22));
    }

    #[test]
    fn test_decoration_for_no_match() {
        let config = Config::default();
        assert!(config.decoration_for("nano").is_none());
    }

    // --- decoration_for_path ---

    #[test]
    fn test_decoration_for_path_claude() {
        let config = Config::default();
        let dec = config.decoration_for_path("/Users/luca/.local/share/claude/versions/2.1.74");
        assert!(dec.is_some());
        assert_eq!(dec.unwrap().process, "claude");
    }

    #[test]
    fn test_decoration_for_path_direct_binary() {
        let config = Config::default();
        let dec = config.decoration_for_path("/opt/homebrew/bin/nvim");
        assert!(dec.is_some());
        assert_eq!(dec.unwrap().process, "nvim");
    }

    #[test]
    fn test_decoration_for_path_no_match() {
        let config = Config::default();
        assert!(config.decoration_for_path("/usr/bin/nano").is_none());
    }

    // --- Action name round-tripping ---

    #[test]
    fn test_action_name_map_focus_group_1_to_9() {
        let map = action_name_map();
        for n in 1..=9u8 {
            let name = format!("focus_group_{}", n);
            assert_eq!(map.get(name.as_str()), Some(&Action::FocusGroupN(n)));
        }
    }

    #[test]
    fn test_action_name_map_switch_workspace_1_to_9() {
        let map = action_name_map();
        for n in 1..=9u8 {
            let name = format!("switch_workspace_{}", n);
            assert_eq!(map.get(name.as_str()), Some(&Action::SwitchWorkspace(n)));
        }
    }

    #[test]
    fn test_action_name_map_contains_all_basic_actions() {
        let map = action_name_map();
        assert_eq!(map.get("quit"), Some(&Action::Quit));
        assert_eq!(map.get("new_workspace"), Some(&Action::NewWorkspace));
        assert_eq!(map.get("close_workspace"), Some(&Action::CloseWorkspace));
        assert_eq!(map.get("new_tab"), Some(&Action::NewTab));
        assert_eq!(map.get("next_tab"), Some(&Action::NextTab));
        assert_eq!(map.get("prev_tab"), Some(&Action::PrevTab));
        assert_eq!(map.get("close_tab"), Some(&Action::CloseTab));
        assert_eq!(map.get("split_horizontal"), Some(&Action::SplitHorizontal));
        assert_eq!(map.get("split_vertical"), Some(&Action::SplitVertical));
        assert_eq!(map.get("help"), Some(&Action::Help));
        assert_eq!(map.get("scroll_mode"), Some(&Action::ScrollMode));
        assert_eq!(map.get("detach"), Some(&Action::Detach));
    }

    // --- Theme presets ---

    #[test]
    fn test_theme_preset_default() {
        let theme = Theme::preset("default").unwrap();
        assert_eq!(theme.accent, Color::Cyan);
    }

    #[test]
    fn test_theme_preset_dracula() {
        let theme = Theme::preset("dracula").unwrap();
        assert_eq!(theme.accent, Color::Rgb(189, 147, 249));
        assert_eq!(theme.bg, Color::Rgb(40, 42, 54));
    }

    #[test]
    fn test_theme_preset_catppuccin() {
        let theme = Theme::preset("catppuccin").unwrap();
        assert_eq!(theme.accent, Color::Rgb(203, 166, 247));
        assert_eq!(theme.bg, Color::Rgb(30, 30, 46));
    }

    #[test]
    fn test_theme_preset_tokyo_night() {
        let theme = Theme::preset("tokyo-night").unwrap();
        assert_eq!(theme.accent, Color::Rgb(122, 162, 247));
        assert_eq!(theme.bg, Color::Rgb(26, 27, 38));
    }

    #[test]
    fn test_theme_preset_unknown_returns_none() {
        assert!(Theme::preset("nonexistent").is_none());
    }

    #[test]
    fn test_theme_preset_with_override() {
        let toml_str = r##"
[theme]
preset = "dracula"
accent = "#ff0000"
"##;
        let raw: RawConfig = toml::from_str(toml_str).unwrap();
        let config = Config::from_raw(raw);
        // Accent overridden to red
        assert_eq!(config.theme.accent, Color::Rgb(255, 0, 0));
        // Other fields from dracula preset
        assert_eq!(config.theme.bg, Color::Rgb(40, 42, 54));
    }

    // --- dim_color ---

    #[test]
    fn test_dim_color_rgb() {
        assert_eq!(
            Theme::dim_color(Color::Rgb(200, 100, 50), 0.65),
            Color::Rgb(130, 65, 32)
        );
    }

    #[test]
    fn test_dim_color_full_factor() {
        assert_eq!(
            Theme::dim_color(Color::Rgb(200, 100, 50), 1.0),
            Color::Rgb(200, 100, 50)
        );
    }

    #[test]
    fn test_dim_color_zero_factor() {
        assert_eq!(
            Theme::dim_color(Color::Rgb(200, 100, 50), 0.0),
            Color::Rgb(0, 0, 0)
        );
    }

    #[test]
    fn test_dim_color_reset_passthrough() {
        assert_eq!(Theme::dim_color(Color::Reset, 0.65), Color::Reset);
    }

    #[test]
    fn test_dim_color_named_to_rgb() {
        let dimmed = Theme::dim_color(Color::Green, 0.65);
        assert!(matches!(dimmed, Color::Rgb(_, _, _)));
    }
}
