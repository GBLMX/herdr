# herdr-gui-poc

A native window for Herdr panes, built with **Ferriso** and **iced_term**.

This is a working prototype, deliberately narrower than the TUI:

- the core actor asks the installed `herdr` CLI for `pane list`;
- the sidebar shows each pane, its workspace, and its agent state;
- selecting a pane starts `herdr terminal attach --takeover <terminal_id>`;
- `iced_term` owns the PTY, parses the ANSI stream with `alacritty_terminal`,
  and renders/inputs into that pane.

One pane is shown at a time. Split layout, Herdr's own sidebar, tabs, images, and
session creation are not ported. The pane processes stay owned by Herdr; closing
the GUI only closes the attach processes.

## Requirements

- Herdr installed and a server/session available (`herdr` or `herdr server`).
- Linux with a working Wayland or X11 session.
- A monospace font. `iced_term` uses the default monospace family unless the
  font settings in `src/gui.rs` are changed.
- Rust 1.88+.

The package has its own `rust-toolchain.toml` pinned to stable. The enclosing
Herdr repository pins 1.96.1; that pin still applies when building Herdr itself.

## Run

From this directory:

```bash
cargo run --release
```

If `herdr` is not on `PATH`:

```bash
HERDR_BIN=/usr/bin/herdr cargo run --release
```

The pane list refreshes every two seconds. Select a pane in the sidebar, then
click the terminal once to give it keyboard focus. If the attach process exits,
the banner offers **Reconnect**.

`terminal attach --takeover` intentionally replaces an existing writable owner
for that pane. Close the TUI first if you want to keep both clients independent;
without takeover Herdr only permits one writer.

## Layout

```text
gui/src/app.rs   Ferriso App: `herdr pane list`, pane snapshot, selection
gui/src/gui.rs   Ferriso Renderer: iced sidebar + iced_term widget
gui/src/main.rs  window entry point
```

The terminal lives in the renderer, not in the actor: it owns a PTY and a
terminal grid, which must not be recreated by an actor snapshot.
