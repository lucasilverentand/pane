# Configuration

`pane` is usable without any configuration. The defaults are intentionally
opinionated: mouse support is enabled, the status bar is populated, common tools
get pane decorations automatically, and default keymaps cover the core workflow.

If you want to customize behavior, `pane` reads a TOML file from:

```text
~/.config/pane/config.toml
```

If the file does not exist, `pane` runs with built-in defaults.

## Configuration Shape

Configuration is grouped into sections:

- `[theme]`
- `[behavior]`
- `[keys]`
- `[normal_keys]`
- `[status_bar]`
- `[leader]`
- `[leader_keys]`
- `[[decorations]]`
- `[[plugins]]`
- `[[tab_picker_entries]]`

Example:

```toml
[theme]
preset = "catppuccin"

[behavior]
mouse = true
nerd_fonts = true
default_shell = "/bin/zsh"
terminal_title_format = "{session} - {workspace}"

[keys]
"ctrl+space" = "enter_normal"

[normal_keys]
"x" = "close_tab"
"space" = "command_palette"

[leader]
key = "space"
timeout_ms = 300

[leader_keys]
"w" = "+Window"
"w d" = "split_horizontal"
"w shift+d" = "split_vertical"
"w z" = "toggle_zoom"
"s" = "+Session"
"s p" = "command_palette"

[status_bar]
show_cpu = true
show_memory = true
show_load = true
show_disk = false
right = "#{cpu} #{mem} #{load}  ^⎵ normal  ⎵ leader "

[[decorations]]
process = "claude"
border_color = "#f97316"

[[plugins]]
command = "pane-git-status"
events = ["workspace_changed", "tick"]
refresh_interval_secs = 5

[[tab_picker_entries]]
name = "API server"
command = "npm run dev"
description = "Start the local dev server"
shell = "/bin/zsh"
category = "project"
```

## Theme

`[theme]` controls colors and presets.

Built-in presets:

- `default`
- `dracula`
- `catppuccin`
- `tokyo-night`

Supported keys:

- `preset`
- `accent`
- `border_inactive`
- `bg`
- `fg`
- `dim`
- `tab_active`
- `tab_inactive`

Colors are accepted as hex strings such as `"#cba6f7"`.

## Behavior

`[behavior]` controls runtime defaults.

Supported keys:

- `fold_bar_size`
- `vim_navigator`
- `mouse`
- `default_shell`
- `auto_suspend_secs`
- `terminal_title_format`
- `nerd_fonts`

Notes:

- `mouse = true` is the default
- `auto_suspend_secs` defaults to `86400`
- `terminal_title_format` defaults to `"{session} - {workspace}"`

## Key Bindings

There are three places to customize input:

- `[keys]` for bindings active in all modes
- `[normal_keys]` for Normal mode bindings
- `[leader_keys]` for multi-key leader sequences

Key syntax examples:

- `"ctrl+space"`
- `"shift+tab"`
- `"alt+h"`
- `"pageup"`
- `"f12"`

Action names come from the action registry used by the command palette and help
screen. Common actions include:

- `enter_interact`
- `enter_normal`
- `new_tab`
- `close_tab`
- `split_horizontal`
- `split_vertical`
- `toggle_zoom`
- `toggle_fold`
- `new_workspace`
- `toggle_overview`
- `command_palette`
- `copy_mode`
- `paste_clipboard`
- `reload_config`

Parameterized workspace actions are also available:

- `switch_workspace_1` through `switch_workspace_9`
- `focus_group_1` through `focus_group_9`

## Leader Key

The default leader key is `space` with a `300ms` timeout.

`[leader]` supports:

- `key`
- `timeout_ms`

`[leader_keys]` maps key paths to either:

- an action name such as `"toggle_zoom"`
- a group label prefixed with `+`, such as `"+Window"`
- `"passthrough"` to stop leader interception at that node

Example:

```toml
[leader]
key = "space"

[leader_keys]
"w" = "+Window"
"w d" = "split_horizontal"
"w h" = "focus_left"
"w l" = "focus_right"
"space" = "command_palette"
```

## Status Bar

`[status_bar]` supports:

- `show_cpu`
- `show_memory`
- `show_load`
- `show_disk`
- `update_interval_secs`
- `left`
- `right`

The default right-hand segment is:

```text
#{cpu} #{mem} #{load}  ^⎵ normal  ⎵ leader
```

## Decorations

`[[decorations]]` lets you override pane border color by detected process name.

Fields:

- `process`
- `border_color`

`pane` already ships with defaults for tools such as `claude`, `codex`, `nvim`,
`k9s`, `python`, `node`, and `ssh`.

## Plugins

`[[plugins]]` adds external commands that can contribute status-bar segments.

Fields:

- `command`
- `events`
- `refresh_interval_secs`

Each plugin returns structured text segments; this is the current extension
point for lightweight status integrations.

## Tab Picker Entries

`[[tab_picker_entries]]` adds custom entries to the tab picker.

Fields:

- `name`
- `command`
- `description`
- `shell`
- `category`

This is useful for project-specific shortcuts such as local servers, test
commands, editors, or agent entry points.

## Reloading Configuration

`pane` supports a `reload_config` action, but there is no hardcoded default key
binding for it. The simplest way to use it is to invoke it from the command
palette or bind it yourself:

```toml
[normal_keys]
"R" = "reload_config"
```
