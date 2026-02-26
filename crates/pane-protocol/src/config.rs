use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::style::Color;
use serde::Deserialize;

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
    DevServerInput,
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
    SessionPicker,
    Help,
    ScrollMode,
    CopyMode,
    PasteClipboard,
    SelectLayout(String),
    ToggleSyncPanes,
    CommandPalette,
    RenameWindow,
    RenamePane,
    Detach,
    SelectMode,
    EnterInteract,
    EnterNormal,
    MaximizeFocused,
    ToggleZoom,
    ToggleFloat,
    NewFloat,
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Theme {
    pub accent: Color,
    pub border_active: Color,
    pub border_inactive: Color,
    pub border_normal: Color,
    pub border_interact: Color,
    pub border_scroll: Color,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub fold_bar_bg: Color,
    pub fold_bar_active_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            border_active: Color::Cyan,
            border_inactive: Color::DarkGray,
            border_normal: Color::Cyan,
            border_interact: Color::Green,
            border_scroll: Color::Yellow,
            bg: Color::Reset,
            fg: Color::Reset,
            dim: Color::DarkGray,
            tab_active: Color::Cyan,
            tab_inactive: Color::DarkGray,
            fold_bar_bg: Color::Rgb(40, 40, 40),
            fold_bar_active_bg: Color::DarkGray,
        }
    }
}

// ---------------------------------------------------------------------------
// Behavior
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Behavior {
    pub min_pane_width: u16,
    pub min_pane_height: u16,
    pub fold_bar_size: u16,
    pub vim_navigator: bool,
    pub mouse: bool,
    pub default_shell: Option<String>,
    /// Seconds of no connected clients before auto-saving and exiting (default: 86400 = 24h).
    pub auto_suspend_secs: u64,
    /// Format string for outer terminal title (e.g., "{session} - {workspace}").
    pub terminal_title_format: Option<String>,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            min_pane_width: 80,
            min_pane_height: 20,
            fold_bar_size: 1,
            vim_navigator: false,
            mouse: true,
            default_shell: None,
            auto_suspend_secs: 86400,
            terminal_title_format: Some("{session} - {workspace}".to_string()),
        }
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
        vec![PaneDecoration {
            process: "claude".to_string(),
            border_color: Color::Rgb(249, 115, 22), // orange
        }]
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
            right: "#{cpu} #{mem} #{load}  \\ leader ".to_string(),
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

        let defaults: Vec<(&str, Action)> = vec![
            ("ctrl+q", Action::Quit),
            ("shift+pageup", Action::ScrollMode),
        ];

        for (key_str, action) in defaults {
            if let Some(key) = parse_key(key_str) {
                map.insert(key, action);
            }
        }

        Self { map }
    }

    pub fn select_defaults() -> Self {
        let mut map = HashMap::new();

        let defaults: Vec<(&str, Action)> = vec![
            ("h", Action::FocusLeft),
            ("j", Action::FocusDown),
            ("k", Action::FocusUp),
            ("l", Action::FocusRight),
            ("left", Action::FocusLeft),
            ("down", Action::FocusDown),
            ("up", Action::FocusUp),
            ("right", Action::FocusRight),
            ("n", Action::NewTab),
            ("w", Action::CloseTab),
            ("[", Action::PrevTab),
            ("]", Action::NextTab),
            ("d", Action::SplitHorizontal),
            ("shift+d", Action::SplitVertical),
            ("t", Action::NewWorkspace),
            ("shift+w", Action::CloseWorkspace),
            ("shift+h", Action::ResizeShrinkH),
            ("shift+l", Action::ResizeGrowH),
            ("shift+j", Action::ResizeGrowV),
            ("shift+k", Action::ResizeShrinkV),
            ("=", Action::Equalize),
            ("alt+h", Action::MoveTabLeft),
            ("alt+j", Action::MoveTabDown),
            ("alt+k", Action::MoveTabUp),
            ("alt+l", Action::MoveTabRight),
            ("/", Action::CommandPalette),
            ("?", Action::Help),
            ("s", Action::SessionPicker),
            ("c", Action::CopyMode),
            ("p", Action::PasteClipboard),
            ("r", Action::RestartPane),
            ("esc", Action::SelectMode),
        ];

        for (key_str, action) in defaults {
            if let Some(key) = parse_key(key_str) {
                map.insert(key, action);
            }
        }

        // 1..9 → FocusGroupN
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            map.insert(key, Action::FocusGroupN(n));
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
    let mut m = HashMap::new();
    m.insert("quit", Action::Quit);
    m.insert("new_workspace", Action::NewWorkspace);
    m.insert("close_workspace", Action::CloseWorkspace);
    m.insert("new_tab", Action::NewTab);
    m.insert("dev_server_input", Action::DevServerInput);
    m.insert("next_tab", Action::NextTab);
    m.insert("next_tab_alt", Action::NextTab);
    m.insert("prev_tab", Action::PrevTab);
    m.insert("prev_tab_alt", Action::PrevTab);
    m.insert("close_tab", Action::CloseTab);
    m.insert("split_horizontal", Action::SplitHorizontal);
    m.insert("split_vertical", Action::SplitVertical);
    m.insert("restart_pane", Action::RestartPane);
    m.insert("focus_left", Action::FocusLeft);
    m.insert("focus_down", Action::FocusDown);
    m.insert("focus_up", Action::FocusUp);
    m.insert("focus_right", Action::FocusRight);
    m.insert("move_tab_left", Action::MoveTabLeft);
    m.insert("move_tab_down", Action::MoveTabDown);
    m.insert("move_tab_up", Action::MoveTabUp);
    m.insert("move_tab_right", Action::MoveTabRight);
    m.insert("resize_shrink_h", Action::ResizeShrinkH);
    m.insert("resize_grow_h", Action::ResizeGrowH);
    m.insert("resize_grow_v", Action::ResizeGrowV);
    m.insert("resize_shrink_v", Action::ResizeShrinkV);
    m.insert("equalize", Action::Equalize);
    m.insert("session_picker", Action::SessionPicker);
    m.insert("help", Action::Help);
    m.insert("scroll_mode", Action::ScrollMode);
    m.insert("copy_mode", Action::CopyMode);
    m.insert("paste_clipboard", Action::PasteClipboard);
    m.insert("toggle_sync_panes", Action::ToggleSyncPanes);
    m.insert("command_palette", Action::CommandPalette);
    m.insert("rename_window", Action::RenameWindow);
    m.insert("rename_pane", Action::RenamePane);
    m.insert("detach", Action::Detach);
    m.insert("select_mode", Action::SelectMode);
    m.insert("enter_interact", Action::EnterInteract);
    m.insert("enter_normal", Action::EnterNormal);
    m.insert("maximize_focused", Action::MaximizeFocused);
    m.insert("toggle_zoom", Action::ToggleZoom);
    m.insert("toggle_float", Action::ToggleFloat);
    m.insert("new_float", Action::NewFloat);
    for n in 1..=9u8 {
        // Leak is fine — these are static strings created once at startup
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
            key: KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE),
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
        insert_leaf(&mut children, "s", Action::SessionPicker, "Sessions");
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

    // \r → +Resize
    {
        let mut children = HashMap::new();
        insert_leaf(&mut children, "h", Action::ResizeShrinkH, "Shrink H");
        insert_leaf(&mut children, "l", Action::ResizeGrowH, "Grow H");
        insert_leaf(&mut children, "j", Action::ResizeGrowV, "Grow V");
        insert_leaf(&mut children, "k", Action::ResizeShrinkV, "Shrink V");
        insert_leaf(&mut children, "=", Action::Equalize, "Equalize");
        let key = parse_key("r").unwrap();
        root.insert(
            key,
            LeaderNode::Group {
                label: "Resize".into(),
                children,
            },
        );
    }

    // Quick splits at root level (2-keystroke access)
    insert_leaf(&mut root, "d", Action::SplitHorizontal, "Split H");
    insert_leaf(&mut root, "shift+d", Action::SplitVertical, "Split V");

    // \y → Paste
    insert_leaf(&mut root, "y", Action::PasteClipboard, "Paste");
    // \/ → Help
    insert_leaf(&mut root, "/", Action::Help, "Help");

    // \\ → PassThrough (literal backslash)
    let bs_key = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE);
    root.insert(bs_key, LeaderNode::PassThrough);

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

#[derive(Clone, Debug)]
pub struct Config {
    pub theme: Theme,
    pub behavior: Behavior,
    pub keys: KeyMap,
    pub select_keys: KeyMap,
    pub status_bar: StatusBarConfig,
    pub decorations: Vec<PaneDecoration>,
    pub leader: LeaderConfig,
    pub plugins: Vec<crate::plugin::PluginConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            behavior: Behavior::default(),
            keys: KeyMap::from_defaults(),
            select_keys: KeyMap::select_defaults(),
            status_bar: StatusBarConfig::default(),
            decorations: PaneDecoration::defaults(),
            leader: LeaderConfig::default(),
            plugins: Vec::new(),
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
            Err(_) => return Self::default(),
        };

        let raw: RawConfig = match toml::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("pane: invalid config at {}: {}", path.display(), e);
                return Self::default();
            }
        };

        Self::from_raw(raw)
    }

    pub fn decoration_for(&self, process: &str) -> Option<&PaneDecoration> {
        self.decorations.iter().find(|d| d.process == process)
    }

    fn from_raw(raw: RawConfig) -> Self {
        let mut config = Self::default();

        // Theme
        if let Some(t) = raw.theme {
            if let Some(s) = t.accent {
                if let Some(c) = parse_color(&s) {
                    config.theme.accent = c;
                }
            }
            if let Some(s) = t.border_active {
                if let Some(c) = parse_color(&s) {
                    config.theme.border_active = c;
                }
            }
            if let Some(s) = t.border_inactive {
                if let Some(c) = parse_color(&s) {
                    config.theme.border_inactive = c;
                }
            }
            if let Some(s) = t.border_normal {
                if let Some(c) = parse_color(&s) {
                    config.theme.border_normal = c;
                }
            }
            if let Some(s) = t.border_interact {
                if let Some(c) = parse_color(&s) {
                    config.theme.border_interact = c;
                }
            }
            if let Some(s) = t.border_scroll {
                if let Some(c) = parse_color(&s) {
                    config.theme.border_scroll = c;
                }
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
            if let Some(s) = t.fold_bar_bg {
                if let Some(c) = parse_color(&s) {
                    config.theme.fold_bar_bg = c;
                }
            }
            if let Some(s) = t.fold_bar_active_bg {
                if let Some(c) = parse_color(&s) {
                    config.theme.fold_bar_active_bg = c;
                }
            }
        }

        // Behavior
        if let Some(b) = raw.behavior {
            if let Some(v) = b.min_pane_width {
                config.behavior.min_pane_width = v;
            }
            if let Some(v) = b.min_pane_height {
                config.behavior.min_pane_height = v;
            }
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
        }

        // Keys
        if let Some(keys) = raw.keys {
            config.keys.merge(&keys);
        }

        // Select keys
        if let Some(select_keys) = raw.select_keys {
            config.select_keys.merge(&select_keys);
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
    select_keys: Option<HashMap<String, String>>,
    status_bar: Option<RawStatusBar>,
    decorations: Option<Vec<RawDecoration>>,
    leader: Option<RawLeader>,
    leader_keys: Option<HashMap<String, String>>,
    plugins: Option<Vec<RawPlugin>>,
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
    accent: Option<String>,
    border_active: Option<String>,
    border_inactive: Option<String>,
    border_normal: Option<String>,
    border_interact: Option<String>,
    border_scroll: Option<String>,
    bg: Option<String>,
    fg: Option<String>,
    dim: Option<String>,
    tab_active: Option<String>,
    tab_inactive: Option<String>,
    fold_bar_bg: Option<String>,
    fold_bar_active_bg: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawBehavior {
    min_pane_width: Option<u16>,
    min_pane_height: Option<u16>,
    fold_bar_size: Option<u16>,
    vim_navigator: Option<bool>,
    mouse: Option<bool>,
    default_shell: Option<String>,
    auto_suspend_secs: Option<u64>,
    terminal_title_format: Option<String>,
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

    if s.starts_with('#') {
        let hex = &s[1..];
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
    fn test_keymap_defaults_quit() {
        let km = KeyMap::from_defaults();
        let key = make_key(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&key), Some(&Action::Quit));
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
        overrides.insert("quit".to_string(), "ctrl+x".to_string());
        km.merge(&overrides);

        // Old binding should be gone
        let old_key = make_key(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&old_key), None);

        // New binding present
        let new_key = make_key(KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&new_key), Some(&Action::Quit));
    }

    // --- Config::from_raw ---

    #[test]
    fn test_config_from_empty_raw() {
        let raw = RawConfig::default();
        let config = Config::from_raw(raw);
        // Should be identical to default
        assert_eq!(config.theme.accent, Color::Cyan);
        assert_eq!(config.behavior.min_pane_width, 80);
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
        assert_eq!(config.behavior.min_pane_width, 80);
        // Unchanged defaults
        assert_eq!(config.theme.border_active, Color::Cyan);
        assert_eq!(config.behavior.min_pane_height, 20);
    }

    // --- LeaderConfig ---

    #[test]
    fn test_leader_config_default_key_and_timeout() {
        let leader = LeaderConfig::default();
        assert_eq!(
            leader.key,
            make_key(KeyCode::Char('\\'), KeyModifiers::NONE)
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
    fn test_default_leader_tree_has_resize_group() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let r_key = parse_key("r").unwrap();
        match children.get(&r_key) {
            Some(LeaderNode::Group { label, .. }) => assert_eq!(label, "Resize"),
            _ => panic!("expected Resize group at 'r'"),
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
    fn test_default_leader_tree_has_passthrough() {
        let tree = default_leader_tree();
        let children = match &tree {
            LeaderNode::Group { children, .. } => children,
            _ => panic!("root should be a Group"),
        };
        let bs_key = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::NONE);
        match children.get(&bs_key) {
            Some(LeaderNode::PassThrough) => {}
            _ => panic!("expected PassThrough at '\\\\'"),
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
        assert!(config.decoration_for("vim").is_none());
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
        assert_eq!(map.get("select_mode"), Some(&Action::SelectMode));
    }
}
