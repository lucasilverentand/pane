use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::style::Color;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Action enum — all bindable actions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    NewWorkspace,
    CloseWorkspace,
    SwitchWorkspace(u8),  // 1-indexed
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
    FocusGroupN(u8),  // 1-indexed
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
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Theme {
    pub accent: Color,
    pub border_active: Color,
    pub border_inactive: Color,
    pub bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub tab_active: Color,
    pub tab_inactive: Color,
    pub fold_bar_bg: Color,
    pub fold_bar_active_bg: Color,
    pub workspace_tab_active_bg: Color,
    pub workspace_tab_inactive_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            border_active: Color::Cyan,
            border_inactive: Color::DarkGray,
            bg: Color::Reset,
            fg: Color::Reset,
            dim: Color::DarkGray,
            tab_active: Color::Cyan,
            tab_inactive: Color::DarkGray,
            fold_bar_bg: Color::Rgb(40, 40, 40),
            fold_bar_active_bg: Color::DarkGray,
            workspace_tab_active_bg: Color::Cyan,
            workspace_tab_inactive_bg: Color::Rgb(50, 50, 50),
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
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            min_pane_width: 100,
            min_pane_height: 4,
            fold_bar_size: 1,
        }
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
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            show_cpu: true,
            show_memory: true,
            show_load: true,
            show_disk: false,
            update_interval_secs: 3,
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
            ("ctrl+t", Action::NewWorkspace),
            ("ctrl+shift+w", Action::CloseWorkspace),
            ("ctrl+n", Action::NewTab),
            ("ctrl+shift+n", Action::DevServerInput),
            ("ctrl+tab", Action::NextTab),
            ("alt+]", Action::NextTab),
            ("ctrl+shift+tab", Action::PrevTab),
            ("alt+[", Action::PrevTab),
            ("ctrl+w", Action::CloseTab),
            ("ctrl+d", Action::SplitHorizontal),
            ("ctrl+shift+d", Action::SplitVertical),
            ("ctrl+r", Action::RestartPane),
            ("alt+h", Action::FocusLeft),
            ("alt+j", Action::FocusDown),
            ("alt+k", Action::FocusUp),
            ("alt+l", Action::FocusRight),
            ("alt+shift+h", Action::MoveTabLeft),
            ("alt+shift+j", Action::MoveTabDown),
            ("alt+shift+k", Action::MoveTabUp),
            ("alt+shift+l", Action::MoveTabRight),
            ("ctrl+alt+h", Action::ResizeShrinkH),
            ("ctrl+alt+l", Action::ResizeGrowH),
            ("ctrl+alt+j", Action::ResizeGrowV),
            ("ctrl+alt+k", Action::ResizeShrinkV),
            ("ctrl+alt+=", Action::Equalize),
            ("ctrl+s", Action::SessionPicker),
            ("ctrl+h", Action::Help),
            ("ctrl+/", Action::Help),
            ("ctrl+?", Action::Help),
            ("shift+pageup", Action::ScrollMode),
        ];

        for (key_str, action) in defaults {
            if let Some(key) = parse_key(key_str) {
                map.insert(key, action);
            }
        }

        // ctrl+1..9 → SwitchWorkspace
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL);
            map.insert(key, Action::SwitchWorkspace(n));
        }

        // alt+1..9 → FocusGroupN
        for n in 1..=9u8 {
            let ch = (b'0' + n) as char;
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT);
            map.insert(key, Action::FocusGroupN(n));
        }

        Self { map }
    }

    pub fn lookup(&self, key: &KeyEvent) -> Option<&Action> {
        self.map.get(key)
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
    m
}

// ---------------------------------------------------------------------------
// Config (top-level)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Config {
    pub theme: Theme,
    pub behavior: Behavior,
    pub keys: KeyMap,
    pub status_bar: StatusBarConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            behavior: Behavior::default(),
            keys: KeyMap::from_defaults(),
            status_bar: StatusBarConfig::default(),
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

    fn from_raw(raw: RawConfig) -> Self {
        let mut config = Self::default();

        // Theme
        if let Some(t) = raw.theme {
            if let Some(s) = t.accent { if let Some(c) = parse_color(&s) { config.theme.accent = c; } }
            if let Some(s) = t.border_active { if let Some(c) = parse_color(&s) { config.theme.border_active = c; } }
            if let Some(s) = t.border_inactive { if let Some(c) = parse_color(&s) { config.theme.border_inactive = c; } }
            if let Some(s) = t.bg { if let Some(c) = parse_color(&s) { config.theme.bg = c; } }
            if let Some(s) = t.fg { if let Some(c) = parse_color(&s) { config.theme.fg = c; } }
            if let Some(s) = t.dim { if let Some(c) = parse_color(&s) { config.theme.dim = c; } }
            if let Some(s) = t.tab_active { if let Some(c) = parse_color(&s) { config.theme.tab_active = c; } }
            if let Some(s) = t.tab_inactive { if let Some(c) = parse_color(&s) { config.theme.tab_inactive = c; } }
            if let Some(s) = t.fold_bar_bg { if let Some(c) = parse_color(&s) { config.theme.fold_bar_bg = c; } }
            if let Some(s) = t.fold_bar_active_bg { if let Some(c) = parse_color(&s) { config.theme.fold_bar_active_bg = c; } }
            if let Some(s) = t.workspace_tab_active_bg { if let Some(c) = parse_color(&s) { config.theme.workspace_tab_active_bg = c; } }
            if let Some(s) = t.workspace_tab_inactive_bg { if let Some(c) = parse_color(&s) { config.theme.workspace_tab_inactive_bg = c; } }
        }

        // Behavior
        if let Some(b) = raw.behavior {
            if let Some(v) = b.min_pane_width { config.behavior.min_pane_width = v; }
            if let Some(v) = b.min_pane_height { config.behavior.min_pane_height = v; }
            if let Some(v) = b.fold_bar_size { config.behavior.fold_bar_size = v; }
        }

        // Keys
        if let Some(keys) = raw.keys {
            config.keys.merge(&keys);
        }

        // Status bar
        if let Some(sb) = raw.status_bar {
            if let Some(v) = sb.show_cpu { config.status_bar.show_cpu = v; }
            if let Some(v) = sb.show_memory { config.status_bar.show_memory = v; }
            if let Some(v) = sb.show_load { config.status_bar.show_load = v; }
            if let Some(v) = sb.show_disk { config.status_bar.show_disk = v; }
            if let Some(v) = sb.update_interval_secs { config.status_bar.update_interval_secs = v; }
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
    status_bar: Option<RawStatusBar>,
}

#[derive(Deserialize, Default)]
struct RawTheme {
    accent: Option<String>,
    border_active: Option<String>,
    border_inactive: Option<String>,
    bg: Option<String>,
    fg: Option<String>,
    dim: Option<String>,
    tab_active: Option<String>,
    tab_inactive: Option<String>,
    fold_bar_bg: Option<String>,
    fold_bar_active_bg: Option<String>,
    workspace_tab_active_bg: Option<String>,
    workspace_tab_inactive_bg: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawBehavior {
    min_pane_width: Option<u16>,
    min_pane_height: Option<u16>,
    fold_bar_size: Option<u16>,
}

#[derive(Deserialize, Default)]
struct RawStatusBar {
    show_cpu: Option<bool>,
    show_memory: Option<bool>,
    show_load: Option<bool>,
    show_disk: Option<bool>,
    update_interval_secs: Option<u64>,
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
    fn test_keymap_defaults_switch_workspace_3() {
        let km = KeyMap::from_defaults();
        let key = make_key(KeyCode::Char('3'), KeyModifiers::CONTROL);
        assert_eq!(km.lookup(&key), Some(&Action::SwitchWorkspace(3)));
    }

    #[test]
    fn test_keymap_defaults_focus_group_5() {
        let km = KeyMap::from_defaults();
        let key = make_key(KeyCode::Char('5'), KeyModifiers::ALT);
        assert_eq!(km.lookup(&key), Some(&Action::FocusGroupN(5)));
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
        assert_eq!(config.behavior.min_pane_width, 100);
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
        assert_eq!(config.behavior.min_pane_height, 4);
    }
}
