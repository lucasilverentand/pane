# Development

This document covers the practical workflow for building and contributing to
`pane`.

## Repository Layout

`pane` is a Rust workspace with four crates:

```text
pane-tui (binary)  -->  pane-daemon  -->  pane-protocol
                        \-> vt100-patched
```

- `crates/pane-tui`: TUI client binary and rendering
- `crates/pane-daemon`: daemon, PTY lifecycle, session state, and socket handling
- `crates/pane-protocol`: shared config, actions, protocol types, and layout model
- `crates/vt100-patched`: local fork of `vt100` with project-specific terminal support

For the conceptual model, see [Architecture](ARCHITECTURE.md).

## Build

```sh
cargo build
cargo build --release
```

Run the client:

```sh
cargo run -p pane-tui -- --help
cargo run -p pane-tui
```

Run the daemon in the foreground:

```sh
cargo run -p pane-tui -- daemon
```

## Test

```sh
cargo test
cargo test -p pane-tui -- snapshot_tests
```

Snapshot workflow:

```sh
cargo insta review
```

## Documentation Map

The docs are intentionally split by audience:

- [README](../README.md) for the project landing page
- [Install and Usage](INSTALL.md) for end-user setup
- [Configuration](CONFIGURATION.md) for runtime customization
- [Architecture](ARCHITECTURE.md) for design and crate structure
- [Design Notes](../DESIGN.md) for UI direction and experiments

## Notes for Contributors

- Keep CLI docs aligned with `crates/pane-tui/src/main.rs`
- Keep configuration docs aligned with `crates/pane-protocol/src/config.rs`
- Keep bindable action names aligned with `crates/pane-protocol/src/registry.rs`
- If you change UI output, expect snapshot test updates in `pane-tui`

## Useful Commands

```sh
git status --short
cargo fmt
cargo clippy --all-targets --all-features
cargo test
```
