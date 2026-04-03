use std::collections::HashSet;
use std::time::Instant;

use ratatui::{layout::Rect, style::Style, Frame};

use pane_protocol::config::{TabPickerEntryConfig, Theme};

use super::dialog;

// Animated placeholder: typewriter cycling through example commands.
const PLACEHOLDER_EXAMPLES: &[&str] = &[
    "htop",
    "cargo build",
    "python3",
    "vim .",
    "npm run dev",
    "docker ps",
];
const CHAR_TYPE_MS: u128 = 80;
const CHAR_DELETE_MS: u128 = 40;
const HOLD_MS: u128 = 1800;
const PAUSE_MS: u128 = 300;

/// What the tab picker is being opened for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabPickerMode {
    NewTab,
    SplitHorizontal,
    SplitVertical,
}

/// State for the fuzzy tab picker overlay.
pub struct TabPickerState {
    pub input: String,
    pub selected: usize,
    pub entries: Vec<TabPickerEntry>,
    pub mode: TabPickerMode,
    filtered: Vec<usize>,
    created_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TabPickerSection {
    Shells,
    Editors,
    Agents,
    Repls,
    System,
    Cluster,
    Scripts,
    Other,
}

impl TabPickerSection {
    fn label(&self) -> &'static str {
        match self {
            Self::Shells => "Shells",
            Self::Editors => "Editors",
            Self::Agents => "Agents",
            Self::Repls => "REPLs",
            Self::System => "System Management",
            Self::Cluster => "Cluster Management",
            Self::Scripts => "Project Scripts",
            Self::Other => "Other",
        }
    }

    /// Parse a category string from config into a section.
    fn from_category(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "shells" | "shell" => Self::Shells,
            "editors" | "editor" => Self::Editors,
            "agents" | "agent" => Self::Agents,
            "repls" | "repl" | "languages" => Self::Repls,
            "system" | "system management" => Self::System,
            "cluster" | "cluster management" => Self::Cluster,
            "scripts" | "project scripts" => Self::Scripts,
            _ => Self::Other,
        }
    }
}

#[derive(Clone)]
pub struct TabPickerEntry {
    pub name: String,
    pub command: Option<String>,
    pub description: String,
    pub section: TabPickerSection,
    /// Shell to wrap the command in (e.g. "/bin/zsh"). When set, the command
    /// is executed as `shell -c "command"` rather than directly.
    pub shell: Option<String>,
    /// Whether this entry is starred (shown in Favorites at the top).
    pub favorite: bool,
}

impl TabPickerState {
    #[allow(dead_code)] // Used in tests
    pub fn new(
        system_programs: &[TabPickerEntry],
        custom_entries: &[TabPickerEntryConfig],
        favorites: &HashSet<String>,
    ) -> Self {
        Self::with_mode(system_programs, custom_entries, favorites, TabPickerMode::NewTab)
    }

    #[allow(dead_code)] // Used in tests
    pub fn with_mode(
        system_programs: &[TabPickerEntry],
        custom_entries: &[TabPickerEntryConfig],
        favorites: &HashSet<String>,
        mode: TabPickerMode,
    ) -> Self {
        Self::with_scripts(system_programs, custom_entries, favorites, mode, &[])
    }

    pub fn with_scripts(
        system_programs: &[TabPickerEntry],
        custom_entries: &[TabPickerEntryConfig],
        favorites: &HashSet<String>,
        mode: TabPickerMode,
        project_scripts: &[TabPickerEntry],
    ) -> Self {
        let entries = build_entries(system_programs, custom_entries, favorites, project_scripts);
        let mut filtered: Vec<usize> = (0..entries.len()).collect();
        // Sort favorites to top, preserving order within each group.
        filtered.sort_by(|&a, &b| entries[b].favorite.cmp(&entries[a].favorite));
        let selected = 0;
        Self {
            input: String::new(),
            selected,
            entries,
            mode,
            filtered,
            created_at: Instant::now(),
        }
    }

    /// Compute the animated placeholder text (typewriter effect).
    pub fn animated_placeholder(&self) -> String {
        let elapsed = self.created_at.elapsed().as_millis();

        let cycle_durations: Vec<u128> = PLACEHOLDER_EXAMPLES
            .iter()
            .map(|w| {
                let n = w.chars().count() as u128;
                n * CHAR_TYPE_MS + HOLD_MS + n * CHAR_DELETE_MS + PAUSE_MS
            })
            .collect();
        let total_cycle: u128 = cycle_durations.iter().sum();
        if total_cycle == 0 {
            return String::new();
        }

        let mut t = elapsed % total_cycle;
        for (i, &word) in PLACEHOLDER_EXAMPLES.iter().enumerate() {
            let dur = cycle_durations[i];
            if t >= dur {
                t -= dur;
                continue;
            }
            let n = word.chars().count();
            let type_time = n as u128 * CHAR_TYPE_MS;
            if t < type_time {
                let chars = (t / CHAR_TYPE_MS) as usize;
                return word.chars().take(chars).collect();
            }
            t -= type_time;
            if t < HOLD_MS {
                return word.to_string();
            }
            t -= HOLD_MS;
            let delete_time = n as u128 * CHAR_DELETE_MS;
            if t < delete_time {
                let remaining = n.saturating_sub((t / CHAR_DELETE_MS) as usize);
                return word.chars().take(remaining).collect();
            }
            return String::new();
        }
        String::new()
    }

    pub fn filtered_entries(&self) -> Vec<(usize, &TabPickerEntry)> {
        self.filtered
            .iter()
            .map(|&i| (i, &self.entries[i]))
            .collect()
    }

    pub fn update_filter(&mut self) {
        let query = self.input.to_lowercase();
        if query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            self.filtered = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    e.name.to_lowercase().contains(&query)
                        || e.description.to_lowercase().contains(&query)
                        || e.command
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                })
                .map(|(i, _)| i)
                .collect();
        }
        // Favorites float to top, preserving order within each group.
        self.filtered
            .sort_by(|&a, &b| self.entries[b].favorite.cmp(&self.entries[a].favorite));
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    /// Returns the command to spawn for the selected entry, or None if nothing is selected.
    pub fn selected_command(&self) -> Option<String> {
        self.filtered.get(self.selected).map(|&i| {
            let entry = &self.entries[i];
            build_command_string(self.mode, entry.command.as_deref(), entry.shell.as_deref())
        })
    }

    /// Toggle the favorite star on the selected entry.
    /// Returns `Some((name, is_favorite))` for persistence, or `None` if nothing is selected.
    pub fn toggle_favorite(&mut self) -> Option<(String, bool)> {
        let &orig_idx = self.filtered.get(self.selected)?;
        let entry = &mut self.entries[orig_idx];
        entry.favorite = !entry.favorite;
        let result = (entry.name.clone(), entry.favorite);

        // Re-sort so the entry moves to/from the favorites group.
        self.filtered
            .sort_by(|&a, &b| self.entries[b].favorite.cmp(&self.entries[a].favorite));
        // Keep selection tracking the same entry.
        self.selected = self
            .filtered
            .iter()
            .position(|&i| i == orig_idx)
            .unwrap_or(0);

        Some(result)
    }
}

/// Build the tmux-style command string with optional shell wrapping.
fn build_command_string(mode: TabPickerMode, command: Option<&str>, shell: Option<&str>) -> String {
    let base = match mode {
        TabPickerMode::NewTab => "new-window",
        TabPickerMode::SplitHorizontal => "split-window -h",
        TabPickerMode::SplitVertical => "split-window -v",
    };
    let mut parts = base.to_string();
    if let Some(cmd) = command {
        let escaped = cmd.replace('\\', "\\\\").replace('"', "\\\"");
        parts.push_str(&format!(" -c \"{}\"", escaped));
    }
    if let Some(sh) = shell {
        let escaped = sh.replace('\\', "\\\\").replace('"', "\\\"");
        parts.push_str(&format!(" -s \"{}\"", escaped));
    }
    parts
}

/// Detect the user's default shell from $SHELL.
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

// ---------------------------------------------------------------------------
// Known programs registry
// ---------------------------------------------------------------------------

struct KnownProgram {
    name: &'static str,
    bins: &'static [&'static str],
    description: &'static str,
    section: TabPickerSection,
    /// Override the command sent to the daemon. When `None`, uses the matched binary name.
    /// Use this when the binary alone doesn't start the right mode (e.g. `bun` needs `bun repl`).
    command_override: Option<&'static str>,
}

const KNOWN_PROGRAMS: &[KnownProgram] = &[
    // -- Shells --
    KnownProgram { name: "Bash", bins: &["bash"], description: "Bourne Again Shell", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "Zsh", bins: &["zsh"], description: "Z Shell", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "Fish", bins: &["fish"], description: "Friendly Interactive Shell", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "Nushell", bins: &["nu"], description: "Modern shell written in Rust", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "Elvish", bins: &["elvish"], description: "Expressive programming shell", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "PowerShell", bins: &["pwsh"], description: "Cross-platform shell", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "Tcsh", bins: &["tcsh"], description: "C shell with enhancements", section: TabPickerSection::Shells, command_override: None },
    KnownProgram { name: "Ksh", bins: &["ksh"], description: "Korn Shell", section: TabPickerSection::Shells, command_override: None },
    // -- Editors --
    KnownProgram { name: "Neovim", bins: &["nvim"], description: "Hyperextensible Vim fork", section: TabPickerSection::Editors, command_override: None },
    KnownProgram { name: "Vim", bins: &["vim"], description: "Vi Improved", section: TabPickerSection::Editors, command_override: None },
    KnownProgram { name: "Helix", bins: &["hx", "helix"], description: "Post-modern text editor", section: TabPickerSection::Editors, command_override: None },
    KnownProgram { name: "Nano", bins: &["nano"], description: "Simple text editor", section: TabPickerSection::Editors, command_override: None },
    KnownProgram { name: "Micro", bins: &["micro"], description: "Modern terminal editor", section: TabPickerSection::Editors, command_override: None },
    KnownProgram { name: "Emacs", bins: &["emacs"], description: "Extensible text editor", section: TabPickerSection::Editors, command_override: None },
    KnownProgram { name: "Kakoune", bins: &["kak"], description: "Modal code editor", section: TabPickerSection::Editors, command_override: None },
    // -- Agents --
    KnownProgram { name: "Claude Code", bins: &["claude"], description: "Anthropic AI coding agent", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Aider", bins: &["aider"], description: "AI pair programming", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Codex", bins: &["codex"], description: "OpenAI coding agent", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Goose", bins: &["goose"], description: "AI developer agent", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Cline", bins: &["cline"], description: "AI coding assistant", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Mentat", bins: &["mentat"], description: "AI coding assistant", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "GPT Engineer", bins: &["gpt-engineer"], description: "AI code generation", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Amazon Q", bins: &["q"], description: "AWS AI assistant", section: TabPickerSection::Agents, command_override: None },
    KnownProgram { name: "Gemini CLI", bins: &["gemini"], description: "Google AI coding agent", section: TabPickerSection::Agents, command_override: None },
    // -- REPLs --
    KnownProgram { name: "Python", bins: &["python3", "python"], description: "Python interpreter", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Node.js", bins: &["node"], description: "JavaScript runtime", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Bun", bins: &["bun"], description: "JavaScript runtime", section: TabPickerSection::Repls, command_override: Some("bun repl") },
    KnownProgram { name: "Deno", bins: &["deno"], description: "JavaScript runtime", section: TabPickerSection::Repls, command_override: Some("deno repl") },
    KnownProgram { name: "Ruby", bins: &["irb"], description: "Ruby REPL", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Lua", bins: &["lua", "luajit"], description: "Lua interpreter", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "GHCi", bins: &["ghci"], description: "Haskell REPL", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Julia", bins: &["julia"], description: "Julia REPL", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "R", bins: &["R"], description: "R language", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Elixir", bins: &["iex"], description: "Elixir REPL", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Erlang", bins: &["erl"], description: "Erlang shell", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Scala", bins: &["scala", "amm"], description: "Scala REPL", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Swift", bins: &["swift"], description: "Swift REPL", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "SQLite", bins: &["sqlite3"], description: "SQLite shell", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "psql", bins: &["psql"], description: "PostgreSQL client", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "MySQL", bins: &["mysql"], description: "MySQL client", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Redis CLI", bins: &["redis-cli"], description: "Redis client", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "Mongosh", bins: &["mongosh"], description: "MongoDB shell", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "pgcli", bins: &["pgcli"], description: "PostgreSQL with autocomplete", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "mycli", bins: &["mycli"], description: "MySQL with autocomplete", section: TabPickerSection::Repls, command_override: None },
    KnownProgram { name: "litecli", bins: &["litecli"], description: "SQLite with autocomplete", section: TabPickerSection::Repls, command_override: None },
    // -- System Management --
    KnownProgram { name: "Htop", bins: &["htop"], description: "Interactive process viewer", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Btop", bins: &["btop"], description: "Resource monitor", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Top", bins: &["top"], description: "Process viewer", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Glances", bins: &["glances"], description: "System monitor", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Bottom", bins: &["btm"], description: "System monitor", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Zenith", bins: &["zenith"], description: "System monitor", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Bandwhich", bins: &["bandwhich"], description: "Network utilization monitor", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Yazi", bins: &["yazi"], description: "Terminal file manager", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Ranger", bins: &["ranger"], description: "Terminal file manager", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "lf", bins: &["lf"], description: "Terminal file manager", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "nnn", bins: &["nnn"], description: "Terminal file manager", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Midnight Commander", bins: &["mc"], description: "Visual file manager", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Broot", bins: &["broot"], description: "File manager & launcher", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Superfile", bins: &["spf"], description: "Terminal file manager", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Lazygit", bins: &["lazygit"], description: "Git terminal UI", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Tig", bins: &["tig"], description: "Git terminal UI", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "GitUI", bins: &["gitui"], description: "Git terminal UI", section: TabPickerSection::System, command_override: None },
    KnownProgram { name: "Lazydocker", bins: &["lazydocker"], description: "Docker terminal UI", section: TabPickerSection::System, command_override: None },
    // -- Cluster Management --
    KnownProgram { name: "K9s", bins: &["k9s"], description: "Kubernetes cluster TUI", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "kdash", bins: &["kdash"], description: "Kubernetes dashboard TUI", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "kubectl", bins: &["kubectl"], description: "Kubernetes CLI", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "Helm", bins: &["helm"], description: "Kubernetes package manager", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "Flux", bins: &["flux"], description: "GitOps toolkit for Kubernetes", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "ArgoCD", bins: &["argocd"], description: "GitOps continuous delivery", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "Terraform", bins: &["terraform", "tofu"], description: "Infrastructure as code", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "Pulumi", bins: &["pulumi"], description: "Infrastructure as code", section: TabPickerSection::Cluster, command_override: None },
    KnownProgram { name: "Nomad", bins: &["nomad"], description: "Workload orchestrator", section: TabPickerSection::Cluster, command_override: None },
];

// ---------------------------------------------------------------------------
// System scanning
// ---------------------------------------------------------------------------

/// Collect all binary names available on $PATH.
fn scan_path_binaries() -> HashSet<String> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    let mut bins = HashSet::new();
    for dir in path_var.split(':') {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                bins.insert(name.to_string());
            }
        }
    }
    bins
}

/// Scan the system for known interactive programs.
///
/// Call once on startup and cache the result — this walks $PATH.
pub fn scan_system_programs() -> Vec<TabPickerEntry> {
    let available = scan_path_binaries();
    let user_shell = default_shell();
    let default_shell_bin = std::path::Path::new(&user_shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let mut entries = Vec::new();
    for prog in KNOWN_PROGRAMS {
        let found_bin = prog.bins.iter().find(|b| available.contains(**b));
        let Some(&bin) = found_bin else { continue };

        // Skip the user's default shell (already shown as the "Shell" entry).
        if prog.section == TabPickerSection::Shells && bin == default_shell_bin {
            continue;
        }

        let cmd = prog.command_override.map(|s| s.to_string()).unwrap_or_else(|| bin.to_string());
        entries.push(TabPickerEntry {
            name: prog.name.to_string(),
            command: Some(cmd),
            description: prog.description.to_string(),
            section: prog.section.clone(),
            shell: if prog.section == TabPickerSection::Shells {
                None
            } else {
                Some(user_shell.clone())
            },
            favorite: false,
        });
    }
    entries
}

/// Detect project scripts/tasks from common config files in the given directory.
///
/// Supports: package.json (npm/bun/yarn), Cargo.toml (cargo), Makefile, Justfile,
/// Taskfile.yml, Pipfile, pyproject.toml, and composer.json.
pub fn scan_project_scripts(project_dir: &std::path::Path) -> Vec<TabPickerEntry> {
    let mut entries = Vec::new();
    let user_shell = default_shell();

    // --- package.json scripts (npm/bun/yarn) ---
    let pkg_json = project_dir.join("package.json");
    if pkg_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                // Detect runner: bun > pnpm > yarn > npm
                let runner = detect_js_runner(project_dir);
                if let Some(scripts) = parsed["scripts"].as_object() {
                    for (name, val) in scripts {
                        let cmd_text = val.as_str().unwrap_or("");
                        entries.push(TabPickerEntry {
                            name: format!("{} run {}", runner, name),
                            command: Some(format!("{} run {}", runner, name)),
                            description: truncate_script_desc(cmd_text, 50),
                            section: TabPickerSection::Scripts,
                            shell: Some(user_shell.clone()),
                            favorite: false,
                        });
                    }
                }
            }
        }
    }

    // --- Cargo.toml binary targets + common cargo commands ---
    let cargo_toml = project_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        // Standard cargo commands
        for (name, desc) in [
            ("cargo build", "Build the project"),
            ("cargo run", "Run the default binary"),
            ("cargo test", "Run tests"),
            ("cargo clippy", "Run linter"),
            ("cargo check", "Type-check without building"),
        ] {
            entries.push(TabPickerEntry {
                name: name.to_string(),
                command: Some(name.to_string()),
                description: desc.to_string(),
                section: TabPickerSection::Scripts,
                shell: Some(user_shell.clone()),
                favorite: false,
            });
        }

        // Parse [[bin]] targets
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("name") {
                    if let Some(name_val) = trimmed
                        .split('=')
                        .nth(1)
                        .map(|s| s.trim().trim_matches('"'))
                    {
                        if !name_val.is_empty()
                            && name_val != "name"
                            && !entries.iter().any(|e| e.name == format!("cargo run --bin {}", name_val))
                        {
                            entries.push(TabPickerEntry {
                                name: format!("cargo run --bin {}", name_val),
                                command: Some(format!("cargo run --bin {}", name_val)),
                                description: format!("Run {}", name_val),
                                section: TabPickerSection::Scripts,
                                shell: Some(user_shell.clone()),
                                favorite: false,
                            });
                        }
                    }
                }
            }
        }
    }

    // --- Makefile targets ---
    let makefile = if project_dir.join("Makefile").exists() {
        Some(project_dir.join("Makefile"))
    } else if project_dir.join("makefile").exists() {
        Some(project_dir.join("makefile"))
    } else if project_dir.join("GNUmakefile").exists() {
        Some(project_dir.join("GNUmakefile"))
    } else {
        None
    };
    if let Some(mf) = makefile {
        if let Ok(content) = std::fs::read_to_string(&mf) {
            for line in content.lines() {
                // Match lines like "target:" but not variable assignments or comments
                if let Some(target) = line.split(':').next() {
                    let target = target.trim();
                    if !target.is_empty()
                        && !target.starts_with('#')
                        && !target.starts_with('.')
                        && !target.starts_with('\t')
                        && !target.contains('=')
                        && !target.contains('$')
                        && !target.contains(' ')
                    {
                        entries.push(TabPickerEntry {
                            name: format!("make {}", target),
                            command: Some(format!("make {}", target)),
                            description: String::new(),
                            section: TabPickerSection::Scripts,
                            shell: Some(user_shell.clone()),
                            favorite: false,
                        });
                    }
                }
            }
        }
    }

    // --- Justfile recipes ---
    let justfile = if project_dir.join("justfile").exists() {
        Some(project_dir.join("justfile"))
    } else if project_dir.join("Justfile").exists() {
        Some(project_dir.join("Justfile"))
    } else {
        None
    };
    if let Some(jf) = justfile {
        if let Ok(content) = std::fs::read_to_string(&jf) {
            for line in content.lines() {
                let trimmed = line.trim();
                // Recipe lines: "name:" or "name arg:" (not indented, not comments, not settings)
                if !trimmed.is_empty()
                    && !trimmed.starts_with('#')
                    && !trimmed.starts_with(' ')
                    && !trimmed.starts_with('\t')
                    && !trimmed.starts_with("set ")
                    && !trimmed.starts_with("export ")
                    && !trimmed.starts_with("alias ")
                    && !trimmed.starts_with("import ")
                    && !trimmed.starts_with("mod ")
                {
                    if let Some(name) = trimmed.split(':').next() {
                        let name = name.split_whitespace().next().unwrap_or("").trim();
                        if !name.is_empty() && !name.contains('=') {
                            entries.push(TabPickerEntry {
                                name: format!("just {}", name),
                                command: Some(format!("just {}", name)),
                                description: String::new(),
                                section: TabPickerSection::Scripts,
                                shell: Some(user_shell.clone()),
                                favorite: false,
                            });
                        }
                    }
                }
            }
        }
    }

    // --- pyproject.toml scripts ---
    let pyproject = project_dir.join("pyproject.toml");
    if pyproject.exists() {
        if let Ok(content) = std::fs::read_to_string(&pyproject) {
            let mut in_scripts = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed == "[project.scripts]" || trimmed == "[tool.poetry.scripts]" {
                    in_scripts = true;
                    continue;
                }
                if trimmed.starts_with('[') {
                    in_scripts = false;
                    continue;
                }
                if in_scripts {
                    if let Some(name) = trimmed.split('=').next() {
                        let name = name.trim().trim_matches('"');
                        if !name.is_empty() {
                            entries.push(TabPickerEntry {
                                name: name.to_string(),
                                command: Some(name.to_string()),
                                description: "Python script".to_string(),
                                section: TabPickerSection::Scripts,
                                shell: Some(user_shell.clone()),
                                favorite: false,
                            });
                        }
                    }
                }
            }
        }
    }

    // --- composer.json scripts (PHP) ---
    let composer_json = project_dir.join("composer.json");
    if composer_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&composer_json) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(scripts) = parsed["scripts"].as_object() {
                    for (name, _) in scripts {
                        // Skip lifecycle hooks
                        if name.starts_with("pre-") || name.starts_with("post-") {
                            continue;
                        }
                        entries.push(TabPickerEntry {
                            name: format!("composer {}", name),
                            command: Some(format!("composer run-script {}", name)),
                            description: String::new(),
                            section: TabPickerSection::Scripts,
                            shell: Some(user_shell.clone()),
                            favorite: false,
                        });
                    }
                }
            }
        }
    }

    // Deduplicate by name
    let mut seen = std::collections::HashSet::new();
    entries.retain(|e| seen.insert(e.name.clone()));

    entries
}

/// Detect the JS package runner for a project directory.
fn detect_js_runner(dir: &std::path::Path) -> &'static str {
    if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
        "bun"
    } else if dir.join("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if dir.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

fn truncate_script_desc(s: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if s.width() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let target = max - 1;
    let mut w = 0;
    let mut end = 0;
    for (i, ch) in s.char_indices() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > target {
            break;
        }
        w += cw;
        end = i + ch.len_utf8();
    }
    format!("{}…", &s[..end])
}

// ---------------------------------------------------------------------------
// Entry builder
// ---------------------------------------------------------------------------

fn build_entries(
    system_programs: &[TabPickerEntry],
    custom_entries: &[TabPickerEntryConfig],
    favorites: &HashSet<String>,
    project_scripts: &[TabPickerEntry],
) -> Vec<TabPickerEntry> {
    let mut entries = Vec::new();
    let user_shell = default_shell();

    // -- Default shell (always present) --
    entries.push(TabPickerEntry {
        name: "Shell".into(),
        command: None,
        description: format!(
            "Default ({})",
            std::path::Path::new(&user_shell)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("shell")
        ),
        section: TabPickerSection::Shells,
        shell: None,
        favorite: false,
    });

    // -- System-detected programs --
    entries.extend(system_programs.iter().cloned());

    // -- Project scripts (detected from project config files) --
    entries.extend(project_scripts.iter().cloned());

    // -- Custom entries from config (placed in their configured category) --
    for ce in custom_entries {
        let section = ce
            .category
            .as_deref()
            .map(TabPickerSection::from_category)
            .unwrap_or(TabPickerSection::Other);
        entries.push(TabPickerEntry {
            name: ce.name.clone(),
            command: Some(ce.command.clone()),
            description: ce.description.clone().unwrap_or_default(),
            section,
            shell: Some(ce.shell.clone().unwrap_or_else(|| user_shell.clone())),
            favorite: false,
        });
    }

    // -- Apply favorites --
    for entry in &mut entries {
        if favorites.contains(&entry.name) {
            entry.favorite = true;
        }
    }

    entries
}

// ---------------------------------------------------------------------------
// Favorites persistence
// ---------------------------------------------------------------------------

fn favorites_path() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        std::path::PathBuf::from(h)
            .join(".config")
            .join("pane")
            .join("favorites")
    })
}

/// Load the set of favorited entry names from disk.
pub fn load_favorites() -> HashSet<String> {
    let Some(path) = favorites_path() else {
        return HashSet::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashSet::new();
    };
    content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Persist the set of favorited entry names to disk.
pub fn save_favorites(favorites: &HashSet<String>) {
    let Some(path) = favorites_path() else { return };
    let mut lines: Vec<&str> = favorites.iter().map(|s| s.as_str()).collect();
    lines.sort();
    let content = lines.join("\n") + "\n";
    let _ = std::fs::write(path, content);
}

/// Mouse hit-test result for the tab picker.
pub enum TabPickerClick {
    /// Clicked on a list item at the given visible index.
    Item(usize),
}

/// Compute the popup area for the tab picker.
fn compute_popup_area(area: Rect) -> Rect {
    dialog::popup_rect(
        dialog::PopupSize::FixedClamped { width: 60, height: 22, pad: 4 },
        dialog::PopupAnchor::Center,
        area,
    )
}

/// Hit-test a mouse click against the tab picker.
///
/// `area` must be the same area passed to `render()`.
///
/// Returns `Some(TabPickerClick::Item(idx))` if the click landed on a list
/// item, where `idx` is the filtered item index (suitable for assigning to
/// `state.selected`). Returns `None` if the click is outside the list area.
pub fn hit_test(state: &TabPickerState, area: Rect, x: u16, y: u16) -> Option<TabPickerClick> {
    let popup_area = compute_popup_area(area);
    let inner = dialog::inner_rect(popup_area);

    if inner.height < 3 || inner.width < 10 {
        return None;
    }

    // List starts after input row + separator row
    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );

    if x < list_area.x
        || x >= list_area.x + list_area.width
        || y < list_area.y
        || y >= list_area.y + list_area.height
    {
        return None;
    }

    let row = (y - list_area.y) as usize;
    let filtered = state.filtered_entries();
    let show_sections = state.input.is_empty();

    // Compute scroll offset to match render_select_list logic
    let visible_count = list_area.height as usize;
    let scroll_offset = if state.selected >= visible_count {
        state.selected - visible_count + 1
    } else {
        0
    };

    // Walk through items accounting for section headers and scroll to map row → item index
    let mut visual_row = 0usize;
    let mut item_idx_visual = 0usize; // counts headers + items for scroll comparison
    let mut last_section: Option<&str> = None;

    for (item_idx, (_, entry)) in filtered.iter().enumerate() {
        if show_sections {
            let section = if entry.favorite { "★ Favorites" } else { entry.section.label() };
            let need_header = match last_section {
                None => true,
                Some(prev) => prev != section,
            };
            if need_header {
                last_section = Some(section);
                if item_idx_visual >= scroll_offset {
                    if visual_row == row {
                        // Clicked on a section header — ignore
                        return None;
                    }
                    visual_row += 1;
                }
                item_idx_visual += 1;
            }
        }
        if item_idx_visual < scroll_offset {
            item_idx_visual += 1;
            continue;
        }
        if visual_row == row {
            return Some(TabPickerClick::Item(item_idx));
        }
        visual_row += 1;
        item_idx_visual += 1;
    }

    None
}

/// Check whether a click is inside the popup area.
pub fn is_inside_popup(area: Rect, x: u16, y: u16) -> bool {
    let popup = compute_popup_area(area);
    x >= popup.x && x < popup.x + popup.width && y >= popup.y && y < popup.y + popup.height
}

pub fn render(state: &TabPickerState, theme: &Theme, hover: Option<(u16, u16)>, frame: &mut Frame, area: Rect) {
    let popup_area = compute_popup_area(area);

    let title = match state.mode {
        TabPickerMode::NewTab => "New Tab",
        TabPickerMode::SplitHorizontal => "Split Right",
        TabPickerMode::SplitVertical => "Split Down",
    };

    let inner = dialog::render_popup(frame, popup_area, title, theme);

    if inner.height < 3 || inner.width < 10 {
        return;
    }

    // Filter input with animated placeholder
    let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let placeholder = state.animated_placeholder();
    let ph = if placeholder.is_empty() { None } else { Some(placeholder.as_str()) };
    dialog::render_filter_input_placeholder(frame, input_area, &state.input, ph, theme);

    // Junction separator: ├───┤ connecting to the left/right borders
    let sep_y = inner.y + 1;
    let border_style = Style::default().fg(theme.accent);
    let inner_width = popup_area.width.saturating_sub(2) as usize;
    let sep_str = format!("├{}┤", "─".repeat(inner_width));
    let buf = frame.buffer_mut();
    buf.set_string(popup_area.x, sep_y, &sep_str, border_style);

    // Build list items with section info
    let filtered = state.filtered_entries();
    let show_sections = state.input.is_empty();
    let has_input = !state.input.trim().is_empty();
    let no_matches = filtered.is_empty();

    // Custom command label shown when input doesn't match or as first option
    let run_label = format!("Run '{}'", state.input.trim());
    let run_desc = "Custom command".to_string();
    let user_shell = default_shell();
    let shell_hint = std::path::Path::new(&user_shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("shell")
        .to_string();

    let mut items: Vec<dialog::ListItem> = Vec::new();

    // Show "Run '<input>'" at the top when there are no matches
    if has_input && no_matches {
        items.push(dialog::ListItem {
            label: &run_label,
            description: &run_desc,
            section: None,
            hint: Some(&shell_hint),
        });
    }

    // Build hint strings for each entry (needs to live long enough)
    let hints: Vec<Option<String>> = filtered
        .iter()
        .map(|(_, entry)| {
            entry.shell.as_ref().map(|s| {
                std::path::Path::new(s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("shell")
                    .to_string()
            })
        })
        .collect();

    for (idx, (_, entry)) in filtered.iter().enumerate() {
        let section_label = if show_sections {
            if entry.favorite {
                Some("★ Favorites")
            } else {
                Some(entry.section.label())
            }
        } else {
            None
        };
        items.push(dialog::ListItem {
            label: &entry.name,
            description: &entry.description,
            section: section_label,
            hint: hints[idx].as_deref(),
        });
    }

    let list_area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(2),
    );
    dialog::render_select_list(frame, list_area, &items, state.selected, show_sections, hover, theme);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_favorites() -> HashSet<String> {
        HashSet::new()
    }

    fn make_custom_entries() -> Vec<TabPickerEntryConfig> {
        vec![
            TabPickerEntryConfig {
                name: "My Tool".into(),
                command: "mytool".into(),
                description: Some("A custom tool".into()),
                shell: None,
                category: None,
            },
            TabPickerEntryConfig {
                name: "Editor".into(),
                command: "vim".into(),
                description: None,
                shell: Some("/bin/zsh".into()),
                category: Some("editors".into()),
            },
        ]
    }

    #[test]
    fn test_new_creates_state_with_entries() {
        let state = TabPickerState::new(&[], &[], &empty_favorites());
        assert_eq!(state.mode, TabPickerMode::NewTab);
        assert!(state.input.is_empty());
        assert_eq!(state.selected, 0);
        assert!(
            state.entries.iter().any(|e| e.name == "Shell"),
            "should contain the default Shell entry"
        );
    }

    #[test]
    fn test_with_mode_split_horizontal() {
        let state = TabPickerState::with_mode(&[], &[], &empty_favorites(), TabPickerMode::SplitHorizontal);
        assert_eq!(state.mode, TabPickerMode::SplitHorizontal);
    }

    #[test]
    fn test_with_mode_split_vertical() {
        let state = TabPickerState::with_mode(&[], &[], &empty_favorites(), TabPickerMode::SplitVertical);
        assert_eq!(state.mode, TabPickerMode::SplitVertical);
    }

    #[test]
    fn test_custom_entries_included() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&[], &custom, &empty_favorites());
        assert!(
            state.entries.iter().any(|e| e.name == "My Tool"),
            "custom entries should be included"
        );
        assert!(
            state.entries.iter().any(|e| e.name == "Editor"),
            "custom entries should be included"
        );
    }

    #[test]
    fn test_custom_entries_in_other_section() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&[], &custom, &empty_favorites());
        let my_tool = state.entries.iter().find(|e| e.name == "My Tool").unwrap();
        assert_eq!(my_tool.section, TabPickerSection::Other);
        assert_eq!(my_tool.description, "A custom tool");
        assert_eq!(my_tool.command, Some("mytool".into()));
    }

    #[test]
    fn test_custom_entry_with_category() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&[], &custom, &empty_favorites());
        let editor = state.entries.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(editor.section, TabPickerSection::Editors);
    }

    #[test]
    fn test_custom_entry_empty_description() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&[], &custom, &empty_favorites());
        let editor = state.entries.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(editor.description, "");
    }

    #[test]
    fn test_custom_entry_with_explicit_shell() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&[], &custom, &empty_favorites());
        let editor = state.entries.iter().find(|e| e.name == "Editor").unwrap();
        assert_eq!(editor.shell, Some("/bin/zsh".into()));
    }

    #[test]
    fn test_custom_entry_inherits_default_shell() {
        let custom = make_custom_entries();
        let state = TabPickerState::new(&[], &custom, &empty_favorites());
        let my_tool = state.entries.iter().find(|e| e.name == "My Tool").unwrap();
        assert!(my_tool.shell.is_some(), "should have a shell set");
    }

    #[test]
    fn test_favorites_float_to_top() {
        let favs: HashSet<String> = ["Shell".to_string()].into_iter().collect();
        let state = TabPickerState::new(&[], &make_custom_entries(), &favs);
        // The first entry in the filtered list should be the favorited one
        let first_filtered = &state.entries[state.filtered[0]];
        assert!(first_filtered.favorite, "first filtered entry should be a favorite");
        assert_eq!(first_filtered.name, "Shell");
    }

    #[test]
    fn test_toggle_favorite() {
        let mut state = TabPickerState::new(&[], &make_custom_entries(), &empty_favorites());
        state.selected = 0;
        let result = state.toggle_favorite();
        assert!(result.is_some());
        let (name, is_fav) = result.unwrap();
        assert!(is_fav);
        // Toggle back
        // Find the entry again (it may have moved)
        let idx = state.filtered.iter().position(|&i| state.entries[i].name == name).unwrap();
        state.selected = idx;
        let result = state.toggle_favorite();
        assert!(result.is_some());
        let (_, is_fav) = result.unwrap();
        assert!(!is_fav);
    }

    #[test]
    fn test_filtered_entries_all_initially() {
        let state = TabPickerState::new(&[], &[], &empty_favorites());
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), state.entries.len());
    }

    #[test]
    fn test_move_down_increments() {
        let state_entries = make_custom_entries();
        let mut state = TabPickerState::new(&[], &state_entries, &empty_favorites());
        state.selected = 0;
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_move_down_clamps_at_end() {
        let mut state = TabPickerState::new(&[], &[], &empty_favorites());
        let max = state.filtered_entries().len() - 1;
        for _ in 0..max + 5 {
            state.move_down();
        }
        assert_eq!(state.selected, max);
    }

    #[test]
    fn test_move_up_decrements() {
        let mut state = TabPickerState::new(&[], &[], &empty_favorites());
        state.selected = 2;
        state.move_up();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_move_up_clamps_at_zero() {
        let mut state = TabPickerState::new(&[], &[], &empty_favorites());
        state.selected = 0;
        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_filter_narrows_results() {
        let custom = vec![TabPickerEntryConfig {
            name: "UniqueXYZ".into(),
            command: "xyz_cmd".into(),
            description: Some("special tool".into()),
            shell: None,
            category: None,
        }];
        let mut state = TabPickerState::new(&[], &custom, &empty_favorites());
        state.input = "UniqueXYZ".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1.name, "UniqueXYZ");
    }

    #[test]
    fn test_filter_case_insensitive() {
        let custom = vec![TabPickerEntryConfig {
            name: "MySpecialTool".into(),
            command: "mst".into(),
            description: None,
            shell: None,
            category: None,
        }];
        let mut state = TabPickerState::new(&[], &custom, &empty_favorites());
        state.input = "myspecialtool".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(
            filtered.iter().any(|(_, e)| e.name == "MySpecialTool"),
            "filter should be case-insensitive"
        );
    }

    #[test]
    fn test_filter_matches_description() {
        let custom = vec![TabPickerEntryConfig {
            name: "Foo".into(),
            command: "foo".into(),
            description: Some("Unique Description Here".into()),
            shell: None,
            category: None,
        }];
        let mut state = TabPickerState::new(&[], &custom, &empty_favorites());
        state.input = "unique description".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(
            filtered.iter().any(|(_, e)| e.name == "Foo"),
            "should match on description"
        );
    }

    #[test]
    fn test_filter_matches_command() {
        let custom = vec![TabPickerEntryConfig {
            name: "Foo".into(),
            command: "my_unique_command_xyz".into(),
            description: None,
            shell: None,
            category: None,
        }];
        let mut state = TabPickerState::new(&[], &custom, &empty_favorites());
        state.input = "unique_command".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(
            filtered.iter().any(|(_, e)| e.name == "Foo"),
            "should match on command"
        );
    }

    #[test]
    fn test_filter_no_match() {
        let mut state = TabPickerState::new(&[], &[], &empty_favorites());
        state.input = "zzzzz_no_match_ever".to_string();
        state.update_filter();
        let filtered = state.filtered_entries();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_clears_restores_all() {
        let mut state = TabPickerState::new(&[], &[], &empty_favorites());
        let total = state.entries.len();
        state.input = "zzzzz".to_string();
        state.update_filter();
        assert_eq!(state.filtered_entries().len(), 0);
        state.input.clear();
        state.update_filter();
        assert_eq!(state.filtered_entries().len(), total);
    }

    #[test]
    fn test_filter_resets_selection_when_out_of_bounds() {
        let custom = vec![
            TabPickerEntryConfig { name: "A".into(), command: "a".into(), description: None, shell: None, category: None },
            TabPickerEntryConfig { name: "B".into(), command: "b".into(), description: None, shell: None, category: None },
        ];
        let mut state = TabPickerState::new(&[], &custom, &empty_favorites());
        for _ in 0..state.entries.len() {
            state.move_down();
        }
        let prev_selected = state.selected;
        state.input = "zzzzz_no_match".to_string();
        state.update_filter();
        assert_eq!(state.selected, 0);
        assert!(prev_selected > 0, "test setup: should have moved down");
    }

    #[test]
    fn test_selected_command_shell() {
        let mut state = TabPickerState::new(&[], &[], &empty_favorites());
        let shell_idx = state.entries.iter().position(|e| e.name == "Shell").unwrap();
        state.selected = state.filtered.iter().position(|&i| i == shell_idx).unwrap();
        let cmd = state.selected_command().unwrap();
        assert_eq!(cmd, "new-window");
    }

    #[test]
    fn test_selected_command_with_shell_wrapping() {
        let custom = vec![TabPickerEntryConfig {
            name: "Htop".into(),
            command: "htop".into(),
            description: None,
            shell: Some("/bin/zsh".into()),
            category: None,
        }];
        let mut state = TabPickerState::new(&[], &custom, &empty_favorites());
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Htop" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert!(cmd.contains("-c \"htop\""), "should include -c flag: {}", cmd);
        assert!(cmd.contains("-s \"/bin/zsh\""), "should include -s flag: {}", cmd);
    }

    #[test]
    fn test_selected_command_split_horizontal() {
        let custom = vec![TabPickerEntryConfig {
            name: "Vim".into(),
            command: "vim".into(),
            description: None,
            shell: None,
            category: None,
        }];
        let mut state = TabPickerState::with_mode(&[], &custom, &empty_favorites(), TabPickerMode::SplitHorizontal);
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Vim" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert!(cmd.starts_with("split-window -h"), "cmd: {}", cmd);
    }

    #[test]
    fn test_selected_command_split_vertical() {
        let custom = vec![TabPickerEntryConfig {
            name: "Vim".into(),
            command: "vim".into(),
            description: None,
            shell: None,
            category: None,
        }];
        let mut state = TabPickerState::with_mode(&[], &custom, &empty_favorites(), TabPickerMode::SplitVertical);
        for (i, entry) in state.entries.iter().enumerate() {
            if entry.name == "Vim" {
                state.selected = state.filtered.iter().position(|&idx| idx == i).unwrap();
                break;
            }
        }
        let cmd = state.selected_command().unwrap();
        assert!(cmd.starts_with("split-window -v"), "cmd: {}", cmd);
    }

    #[test]
    fn test_section_labels() {
        assert_eq!(TabPickerSection::Shells.label(), "Shells");
        assert_eq!(TabPickerSection::Editors.label(), "Editors");
        assert_eq!(TabPickerSection::Agents.label(), "Agents");
        assert_eq!(TabPickerSection::Repls.label(), "REPLs");
        assert_eq!(TabPickerSection::System.label(), "System Management");
        assert_eq!(TabPickerSection::Cluster.label(), "Cluster Management");
        assert_eq!(TabPickerSection::Other.label(), "Other");
    }

    #[test]
    fn test_build_command_string_no_shell() {
        let cmd = build_command_string(TabPickerMode::NewTab, Some("vim"), None);
        assert_eq!(cmd, "new-window -c \"vim\"");
    }

    #[test]
    fn test_build_command_string_with_shell() {
        let cmd = build_command_string(TabPickerMode::NewTab, Some("htop"), Some("/bin/zsh"));
        assert_eq!(cmd, "new-window -c \"htop\" -s \"/bin/zsh\"");
    }

    #[test]
    fn test_build_command_string_no_command() {
        let cmd = build_command_string(TabPickerMode::NewTab, None, None);
        assert_eq!(cmd, "new-window");
    }

    #[test]
    fn test_build_command_string_escapes_quotes() {
        let cmd = build_command_string(
            TabPickerMode::NewTab,
            Some("git commit -m \"fix bug\""),
            Some("/bin/zsh"),
        );
        assert_eq!(
            cmd,
            "new-window -c \"git commit -m \\\"fix bug\\\"\" -s \"/bin/zsh\""
        );
    }

    #[test]
    fn test_build_command_string_escapes_backslashes() {
        let cmd = build_command_string(
            TabPickerMode::NewTab,
            Some("echo \\n"),
            None,
        );
        assert_eq!(cmd, "new-window -c \"echo \\\\n\"");
    }

    #[test]
    fn test_shell_entries_have_no_shell_wrapping() {
        let state = TabPickerState::new(&[], &[], &empty_favorites());
        let shell_entry = state.entries.iter().find(|e| e.name == "Shell").unwrap();
        assert!(shell_entry.shell.is_none(), "default shell entry should not have shell wrapping");
    }

    #[test]
    fn test_favorites_in_filtered_order() {
        let entries = vec![
            TabPickerEntry {
                name: "Shell".into(),
                command: None,
                description: "Default".into(),
                section: TabPickerSection::Shells,
                shell: None,
                favorite: false,
            },
            TabPickerEntry {
                name: "htop".into(),
                command: Some("htop".into()),
                description: "Process viewer".into(),
                section: TabPickerSection::System,
                shell: Some("/bin/zsh".into()),
                favorite: true,
            },
        ];
        let mut filtered: Vec<usize> = (0..entries.len()).collect();
        filtered.sort_by(|&a, &b| entries[b].favorite.cmp(&entries[a].favorite));
        let state = TabPickerState {
            input: String::new(),
            selected: 0,
            entries,
            mode: TabPickerMode::NewTab,
            filtered,
            created_at: Instant::now(),
        };
        // htop (favorite) should be first in filtered order
        assert_eq!(state.entries[state.filtered[0]].name, "htop");
        assert!(state.entries[state.filtered[0]].favorite);
    }

    #[test]
    fn test_initial_selection_default() {
        let entries = vec![
            TabPickerEntry {
                name: "Shell".into(),
                command: None,
                description: "Default".into(),
                section: TabPickerSection::Shells,
                shell: None,
                favorite: false,
            },
            TabPickerEntry {
                name: "htop".into(),
                command: Some("htop".into()),
                description: "Process viewer".into(),
                section: TabPickerSection::System,
                shell: Some("/bin/zsh".into()),
                favorite: false,
            },
        ];
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let state = TabPickerState {
            input: String::new(),
            selected: 0,
            entries,
            mode: TabPickerMode::NewTab,
            filtered,
            created_at: Instant::now(),
        };
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_non_shell_entries_have_shell_wrapping() {
        let state = TabPickerState::new(&[], &[], &empty_favorites());
        for entry in &state.entries {
            if entry.section != TabPickerSection::Shells && entry.command.is_some() {
                assert!(entry.shell.is_some(), "'{}' should have shell wrapping", entry.name);
            }
        }
    }

    #[test]
    fn test_hit_test_across_sections() {
        let entries = vec![
            TabPickerEntry {
                name: "Shell".into(),
                command: None,
                description: "Default".into(),
                section: TabPickerSection::Shells,
                shell: None,
                favorite: false,
            },
            TabPickerEntry {
                name: "htop".into(),
                command: Some("htop".into()),
                description: "Process viewer".into(),
                section: TabPickerSection::System,
                shell: Some("/bin/zsh".into()),
                favorite: false,
            },
            TabPickerEntry {
                name: "k9s".into(),
                command: Some("k9s".into()),
                description: "Kubernetes TUI".into(),
                section: TabPickerSection::Cluster,
                shell: Some("/bin/zsh".into()),
                favorite: false,
            },
        ];
        let filtered: Vec<usize> = (0..entries.len()).collect();
        let state = TabPickerState {
            input: String::new(),
            selected: 0,
            entries,
            mode: TabPickerMode::NewTab,
            filtered,
            created_at: Instant::now(),
        };

        let area = Rect::new(0, 0, 80, 30);
        let popup = compute_popup_area(area);
        let inner = crate::ui::dialog::inner_rect(popup);
        let list_y = inner.y + 2;

        // With sections visible and no scroll, layout is:
        // row 0: Shells header
        // row 1: Shell
        // row 2: System Management header
        // row 3: htop
        // row 4: Cluster Management header
        // row 5: k9s (item_idx=2)

        let click = hit_test(&state, area, inner.x + 5, list_y + 5);
        assert_eq!(
            click.as_ref().map(|c| match c { TabPickerClick::Item(i) => *i }),
            Some(2),
            "clicking on 'k9s' should return item index 2"
        );

        // Verify the command for item 2
        let state_with_selection = TabPickerState {
            input: String::new(),
            selected: 2,
            entries: state.entries.clone(),
            mode: TabPickerMode::NewTab,
            filtered: state.filtered.clone(),
            created_at: Instant::now(),
        };
        let cmd = state_with_selection.selected_command();
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert!(cmd.contains("k9s"), "command should contain k9s, got: {}", cmd);
    }

    #[test]
    fn test_from_category_parser() {
        assert_eq!(TabPickerSection::from_category("shells"), TabPickerSection::Shells);
        assert_eq!(TabPickerSection::from_category("Editors"), TabPickerSection::Editors);
        assert_eq!(TabPickerSection::from_category("AGENTS"), TabPickerSection::Agents);
        assert_eq!(TabPickerSection::from_category("repls"), TabPickerSection::Repls);
        assert_eq!(TabPickerSection::from_category("system"), TabPickerSection::System);
        assert_eq!(TabPickerSection::from_category("cluster"), TabPickerSection::Cluster);
        assert_eq!(TabPickerSection::from_category("something"), TabPickerSection::Other);
    }

    #[test]
    fn truncate_script_desc_short() {
        assert_eq!(truncate_script_desc("hello", 10), "hello");
    }

    #[test]
    fn truncate_script_desc_exact() {
        assert_eq!(truncate_script_desc("hello", 5), "hello");
    }

    #[test]
    fn truncate_script_desc_long() {
        assert_eq!(truncate_script_desc("hello world", 6), "hello…");
    }

    #[test]
    fn truncate_script_desc_max_one() {
        assert_eq!(truncate_script_desc("hello", 1), "…");
    }

    #[test]
    fn truncate_script_desc_multibyte() {
        // Ensure no panic on multi-byte input
        assert_eq!(truncate_script_desc("café au lait", 5), "café…");
    }

    #[test]
    fn truncate_script_desc_cjk() {
        // Each CJK char is 2 display columns
        assert_eq!(truncate_script_desc("你好世界", 5), "你好…");
    }
}
