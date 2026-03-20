# zdtwalk

[![Crates.io](https://img.shields.io/crates/v/zdtwalk.svg)](https://crates.io/crates/zdtwalk)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A terminal UI for exploring and editing Zephyr device trees.

---

## Overview

**zdtwalk** is a TUI tool built for Zephyr RTOS developers who work with device tree source (DTS) files. It auto-discovers your west workspace, indexes board DTS files, user overlays, and YAML bindings, and lets you explore everything from a single terminal interface.

You can browse parsed device trees as expandable node hierarchies, follow `#include` chains across files, view Zephyr binding definitions, and interactively build overlay files — all without leaving your terminal.

zdtwalk fetches HAL module DTS sources on first run (using sparse git checkouts), caches them locally, and resolves full board device trees with all includes merged.

## Features

- **Workspace auto-discovery** — finds your `.west/` root and Zephyr version automatically
- **Three-mode file browser** — switch between Board DTS files, User Overlays, and Bindings with `1`/`2`/`3`
- **Full DTS parser** — nodes, properties, cell arrays, phandle references, C macros, labeled references, `/delete-node/`, `/delete-property/`, and more
- **Structured tree view** — expand and collapse device tree nodes alongside a raw text view; toggle with `v`
- **Include navigation** — press `Enter` on an `#include` line to jump into the referenced file
- **YAML binding viewer** — browse and search `dts/bindings/` with parsed property types, descriptions, and constraints
- **Interactive overlay generator** — a step-by-step wizard to create device tree overlays: select a board, add/edit nodes and properties, and save to a `.overlay` file
- **Multi-tab viewer** — open multiple files side-by-side, switch with `{`/`}`
- **Fuzzy search** — filter the file tree or search within open files with `/`
- **Vim-style navigation** — `hjkl`, `/` search, `n`/`N` next/prev match, `V` visual select, `y` yank
- **HAL fetching with caching** — sparse-checks out HAL DTS directories, cached per Zephyr version in `~/.cache/zdtwalk/`
- **Visual selection + clipboard** — select lines with `V`, yank with `y` (uses OSC 52 for terminal clipboard)
- **Debug log panel** — toggle with `Ctrl-D` to see workspace discovery, parsing, and fetch activity

To see a full list of keybinds, either press ? in the program or reference [KEYBINDS.md](KEYBINDS.md)

## Installation

```sh
cargo install zdtwalk
```

Or install from source:

```sh
cargo install --git https://github.com/evinlodder/zdtwalk
```

## Quick Start

Run `zdtwalk` inside your Zephyr workspace — it will auto-discover the `.west/` directory:

```sh
cd ~/zephyrproject
zdtwalk
```

Or specify the workspace path explicitly:

```sh
zdtwalk --workspace /path/to/zephyrproject
```

On first run, zdtwalk will fetch HAL module DTS sources in the background. Subsequent runs use the cache.

## Usage

zdtwalk has a three-panel layout:

```
┌─────────────┬────────────────────────┬───────────────┐
│  File Tree  │        Viewer          │   Generator   │
│  (Left)     │       (Center)         │   (Right)     │
│             │                        │               │
│  Board DTS  │  Tree / Raw view of    │  Overlay      │
│  Overlays   │  selected file         │  builder      │
│  Bindings   │                        │  wizard       │
└─────────────┴────────────────────────┴───────────────┘
```

### Panels

- **Left — File Tree**: Browse board DTS files, user overlays, or YAML bindings. Press `m` to cycle modes or `1`/`2`/`3` to jump directly. Use `b` to pick a board in Board mode.
- **Center — Viewer**: View the selected file in a structured tree or raw text mode. Navigate includes by pressing `Enter` on `#include` lines. Search with `/`, toggle view mode with `v`.
- **Right — Generator**: Toggle with `g`. Walk through a wizard to build a device tree overlay: select a board → edit nodes and properties → save the file.

Switch panels with `Tab` / `Shift-Tab`. Press `?` for a help overlay.

### Overlay Generator Workflow

1. Press `g` to open the generator
2. Select a target board (the board's full device tree is resolved automatically)
3. Add nodes — press `a` on any node in the viewer to add it to the overlay, or `n` to create a new reference node manually
4. Edit properties with `p` (add), `e` (edit), `d` (delete)
5. Press `→` to move to the save step, pick a location, and write the file

## Keybindings

Here are the most common keybindings. See [KEYBINDS.md](KEYBINDS.md) for the full reference.

| Key | Action |
|-----|--------|
| `Tab` / `Shift-Tab` | Switch panels |
| `j` / `k` | Move down / up |
| `h` / `l` | Collapse / expand node |
| `/` | Search |
| `v` | Toggle tree / raw view |
| `g` | Toggle generator panel |
| `Enter` | Open file / follow include / expand node |
| `?` | Help overlay |
| `q` / `Ctrl-C` | Quit |

## Requirements

- A **Zephyr workspace** initialized with `west init` and `west update`
- **git** on your `PATH` (used for sparse HAL checkouts on first run)
- A terminal with **256-color support** (most modern terminals)

## Long-Term Possibilities
- Overlay file error checker
- Analysis for build.dts

## License

[MIT](LICENSE)
