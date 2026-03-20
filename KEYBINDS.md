# zdtwalk — Keybindings Reference

Full list of all keyboard shortcuts in zdtwalk, organized by context.

Navigation is vim-style: `j`/`k` to move, `h`/`l` to collapse/expand, `/` to search.

---

## Global

These work regardless of which panel is focused.

| Key | Action |
|-----|--------|
| `q` | Quit (or close debug panel if active) |
| `Ctrl-C` | Quit |
| `Tab` | Focus next panel |
| `Shift-Tab` | Focus previous panel |
| `?` | Toggle help overlay |
| `g` | Toggle generator (right panel) |
| `Ctrl-D` | Toggle debug log panel |
| `[` | Decrease left panel width (−3%, min 10%) |
| `]` | Increase left panel width (+3%, max 50%) |

---

## Left Panel — File Tree

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Open selected file |

### Mode Switching

| Key | Action |
|-----|--------|
| `m` | Cycle mode: Board DTS → User Overlays → Bindings |
| `1` | Switch to Board Files |
| `2` | Switch to User Overlays |
| `3` | Switch to Bindings |

### Board Picker (Board Files mode)

| Key | Action |
|-----|--------|
| `b` | Toggle board picker dropdown |
| `Enter` | Select board from dropdown |

### Search

| Key | Action |
|-----|--------|
| `/` | Start fuzzy search |
| _typing_ | Filter results |
| `Enter` | Confirm search |
| `Esc` | Cancel search and clear results |
| `Backspace` | Delete last character |

---

## Center Panel — Viewer

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Scroll down |
| `k` / `↑` | Scroll up |

### View Mode

| Key | Action |
|-----|--------|
| `v` | Toggle between raw text and structured tree view |

### Node Navigation (Tree View)

| Key | Action |
|-----|--------|
| `Enter` / `Space` | Toggle expand/collapse node |
| `h` / `←` | Collapse current node |
| `l` / `→` | Expand current node |

### Include Navigation

| Key | Action |
|-----|--------|
| `Enter` / `Space` | Follow `#include` directive (opens referenced file) |

### Search

| Key | Action |
|-----|--------|
| `/` | Start search |
| `n` | Jump to next match |
| `N` | Jump to previous match |
| `Esc` | Clear search |

When typing a search query:

| Key | Action |
|-----|--------|
| `Enter` | Confirm query, jump to first match |
| `Esc` | Cancel search |
| `Backspace` | Delete last character |

### Visual Selection

| Key | Action |
|-----|--------|
| `V` | Enter/exit visual line selection mode |
| `j` / `k` | Extend selection |
| `y` | Yank (copy) selected lines to clipboard |
| `Esc` | Exit visual mode |

### Tabs

| Key | Action |
|-----|--------|
| `}` | Next tab |
| `{` | Previous tab |
| `Ctrl-W` | Close current tab |

### Generator Integration

| Key | Action |
|-----|--------|
| `a` | Add node at cursor to the generator overlay |

---

## Right Panel — Generator

The generator is a step-by-step wizard for building device tree overlays.

### Step 1: Select Board

| Key | Action |
|-----|--------|
| `→` / `Enter` | Continue to Edit Nodes step |

### Step 2: Edit Nodes

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` / `Space` | Toggle expand/collapse node |
| `n` | Add new reference node |
| `c` | Add child node to selected node |
| `p` | Add property to selected node |
| `e` | Edit property value |
| `d` | Delete selected node or property |
| `s` | Quick save (if path already set) |
| `→` | Proceed to Save File step |
| `←` / `Esc` | Go back to Select Board |

### Step 3: Save File

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down in file browser |
| `k` / `↑` | Move up in file browser |
| `Enter` | Select file/directory or confirm save |
| `Backspace` | Go up one directory |
| `n` | Create new file |
| `←` / `Esc` | Go back to Edit Nodes |

### Filename Input

| Key | Action |
|-----|--------|
| _typing_ | Enter filename |
| `Enter` | Confirm filename |
| `Esc` | Cancel |
| `Backspace` | Delete last character |

### Overwrite Confirmation

| Key | Action |
|-----|--------|
| `Enter` | Confirm overwrite |
| `Esc` | Cancel |

### After Save

| Key | Action |
|-----|--------|
| `y` | Continue editing the same overlay |
| `n` | Start a new overlay |
| `g` | Close generator panel |

---

## Debug Panel

Toggle with `Ctrl-D`. Shows workspace discovery, parsing, and fetch logs.

| Key | Action |
|-----|--------|
| `j` / `↓` | Scroll down |
| `k` / `↑` | Scroll up |
| `G` | Jump to bottom (follow mode) |
| `g` | Jump to top |

---

## Notes

- The left panel is **locked** while the generator is open with a resolved board tree. Close the generator with `g` to unlock it.
- Clipboard yanking uses the **OSC 52** escape sequence, which is supported by most modern terminals (kitty, alacritty, iTerm2, Windows Terminal, tmux with `set -g set-clipboard on`).
- All input fields (search, filename, node name, property value) share the same input behavior: type to enter text, `Enter` to confirm, `Esc` to cancel, `Backspace` to delete.
