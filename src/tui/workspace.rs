use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::dts;
use crate::west;
use crate::west::discovery::WorkspaceInfo;
use crate::west::fetch::HalDtsEntry;
use crate::{tui_log, tui_warn, tui_error};

// ---------------------------------------------------------------------------
// WorkspaceState — resolved workspace info held by App
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WorkspaceState {
    pub info: WorkspaceInfo,
    pub zephyr_version: String,
    /// Resolved HAL module DTS entries (local + cached + fetched).
    pub hal_entries: Vec<HalDtsEntry>,
}

// ---------------------------------------------------------------------------
// FileEntry — a single file in the file tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Dts,
    Dtsi,
    Overlay,
    Binding,
}

impl FileKind {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "dts" => Some(FileKind::Dts),
            "dtsi" => Some(FileKind::Dtsi),
            "overlay" | "dtso" => Some(FileKind::Overlay),
            "yaml" | "yml" => Some(FileKind::Binding),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Discover the Zephyr workspace, optionally from a user-supplied path.
pub async fn discover_workspace(
    override_path: Option<PathBuf>,
) -> Result<WorkspaceState, crate::dts::Error> {
    tokio::task::spawn_blocking(move || {
        let start = override_path.unwrap_or_else(|| {
            std::env::current_dir().expect("cannot read current directory")
        });

        tui_log!("Discovering workspace from: {}", start.display());
        let info = west::find_workspace(&start)?;
        tui_log!("Found workspace root: {}", info.workspace_root.display());
        tui_log!("Zephyr dir: {}", info.zephyr_dir.display());
        tui_log!("West manifest: {}", info.west_yml_path.display());

        let zephyr_version = west::get_zephyr_version(&info.zephyr_dir)?;
        tui_log!("Zephyr version: {zephyr_version}");

        Ok(WorkspaceState {
            info,
            zephyr_version,
            hal_entries: Vec::new(),
        })
    })
    .await
    .expect("workspace discovery task panicked")
}

/// Fetch HAL module DTS/bindings for all modules in the west manifest.
/// Returns the resolved entries (local, cached, or fetched).
pub async fn fetch_hal_modules(
    ws: WorkspaceState,
    progress_tx: tokio::sync::mpsc::Sender<String>,
) -> Vec<HalDtsEntry> {
    tokio::task::spawn_blocking(move || {
        tui_log!("Starting HAL module fetch...");
        match west::fetch::fetch_all_hal_dts(&ws.info, false, |idx, total, name| {
            tui_log!("HAL fetch ({}/{total}): {name}", idx + 1);
            let _ = progress_tx.try_send(format!(
                "Fetching HAL modules ({}/{total}): {name}...", idx + 1
            ));
        }) {
            Ok(entries) => {
                for e in &entries {
                    match &e.source {
                        crate::west::fetch::DtsSource::Local => {
                            tui_log!("  {} -> local (dts={:?})", e.project_name, e.dts_path);
                        }
                        crate::west::fetch::DtsSource::Cached => {
                            tui_log!("  {} -> cached", e.project_name);
                        }
                        crate::west::fetch::DtsSource::Fetched => {
                            tui_log!("  {} -> fetched", e.project_name);
                        }
                        crate::west::fetch::DtsSource::NoDts => {
                            tui_log!("  {} -> no dts", e.project_name);
                        }
                    }
                }
                let found = entries.iter().filter(|e| e.dts_path.is_some()).count();
                tui_log!("HAL fetch complete: {found}/{} modules have dts/", entries.len());
                entries
            }
            Err(e) => {
                tui_error!("HAL fetch error: {e}");
                let _ = progress_tx.try_send(format!("HAL fetch error: {e}"));
                Vec::new()
            }
        }
    })
    .await
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// File scanning
// ---------------------------------------------------------------------------

/// List board names from zephyr/boards/.
/// A board is a directory containing a `board.cmake` file.
pub async fn list_boards(zephyr_dir: &Path) -> Vec<String> {
    let dir = zephyr_dir.join("boards");
    tui_log!("Scanning boards in: {}", dir.display());
    tokio::task::spawn_blocking(move || {
        let mut boards = Vec::new();
        collect_boards_recursive(&dir, &mut boards);
        boards.sort();
        boards.dedup();
        tui_log!("Found {} boards", boards.len());
        boards
    })
    .await
    .unwrap_or_default()
}

fn collect_boards_recursive(dir: &Path, boards: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // A board directory contains board.cmake.
            if path.join("board.cmake").exists() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    boards.push(name.to_string());
                }
            }
            collect_boards_recursive(&path, boards);
        }
    }
}

/// Scan board-specific DTS/DTSI files by finding the board's main .dts/.dtsi
/// and recursively collecting all of its includes.
pub async fn scan_board_files(ws: &WorkspaceState, board: &str) -> Vec<FileEntry> {
    let boards_dir = ws.info.zephyr_dir.join("boards");
    let ws_clone = ws.clone();
    let board_name = board.to_string();
    tui_log!("Scanning DTS files for board: {board}");
    tokio::task::spawn_blocking(move || {
        // Find the board directory.
        let Some(board_dir) = find_board_dir(&boards_dir, &board_name) else {
            return vec![];
        };

        // Look for the main DTS entry point: {board_name}.dts or {board_name}.dtsi.
        let main_dts = board_dir.join(format!("{board_name}.dts"));
        let main_dtsi = board_dir.join(format!("{board_name}.dtsi"));
        let entry = if main_dts.exists() {
            main_dts
        } else if main_dtsi.exists() {
            main_dtsi
        } else {
            // Fallback: just list all DTS/DTSI in the board dir.
            let mut entries = Vec::new();
            collect_dts_entries(&board_dir, &mut entries);
            return entries;
        };

        // Build the standard Zephyr DTS include search paths (including HAL DTS dirs).
        let search_paths = build_dts_search_paths(&ws_clone, &board_dir);

        // Recursively collect all included files starting from the entry point.
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        collect_includes_recursive(&entry, &search_paths, &mut seen, &mut files);

        files
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                let kind = FileKind::from_extension(ext).unwrap_or(FileKind::Dtsi);
                FileEntry { path, name, kind }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Find a board directory by name (recursive search).
fn find_board_dir(dir: &Path, board: &str) -> Option<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return None;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some(board)
                && path.join("board.cmake").exists()
            {
                return Some(path);
            }
            if let Some(found) = find_board_dir(&path, board) {
                return Some(found);
            }
        }
    }
    None
}

/// Build the standard Zephyr DTS include search paths.
///
/// Zephyr's CMake adds these include dirs for DTS compilation:
/// - The board directory itself
/// - zephyr/dts/common/
/// - zephyr/dts/<arch>/ for each architecture dir
/// - HAL module dts/ directories (from local checkout, cache, or fetched)
fn build_dts_search_paths(ws: &WorkspaceState, board_dir: &Path) -> Vec<PathBuf> {
    let zephyr_dir = &ws.info.zephyr_dir;
    let mut paths = vec![board_dir.to_path_buf()];

    let dts_dir = zephyr_dir.join("dts");
    // Add dts/common/ if it exists.
    let common = dts_dir.join("common");
    if common.is_dir() {
        paths.push(common);
    }

    // Add each architecture subdirectory of dts/.
    if let Ok(rd) = std::fs::read_dir(&dts_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Skip non-arch directories.
                if name == "bindings" || name == "common" || name == "vendor" {
                    continue;
                }
                paths.push(p);
            }
        }
    }

    // Also add the zephyr root and dts/ itself as last-resort paths.
    paths.push(dts_dir);
    paths.push(zephyr_dir.to_path_buf());

    // Add HAL module DTS directories from resolved entries.
    for entry in &ws.hal_entries {
        if let Some(dts_path) = &entry.dts_path {
            if dts_path.is_dir() {
                paths.push(dts_path.clone());
            }
        }
    }

    paths
}

/// Starting from a DTS/DTSI file, parse it for #include / /include/ directives
/// and recursively collect all referenced files.
fn collect_includes_recursive(
    file: &Path,
    search_paths: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
    result: &mut Vec<PathBuf>,
) {
    let canonical = match file.canonicalize() {
        Ok(c) => c,
        Err(_) => return,
    };
    if seen.contains(&canonical) {
        return;
    }
    seen.insert(canonical.clone());
    result.push(canonical.clone());

    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Try parsing to extract includes. If parse fails, fall back to regex extraction.
    let includes = match dts::parse_dts(&content) {
        Ok(tree) => tree.includes.iter().map(|i| i.path.clone()).collect::<Vec<_>>(),
        Err(_) => extract_includes_regex(&content),
    };

    // Build search paths: file's own directory + provided paths.
    let mut paths = search_paths.to_vec();
    if let Some(parent) = canonical.parent() {
        let p = parent.to_path_buf();
        if !paths.contains(&p) {
            paths.insert(0, p);
        }
    }

    for inc_str in &includes {
        let inc_path = Path::new(inc_str);

        // Skip .h includes (C headers like input-event-codes.h).
        if let Some(ext) = inc_path.extension().and_then(|e| e.to_str()) {
            if ext == "h" {
                continue;
            }
        }

        let resolved = if inc_path.is_absolute() && inc_path.exists() {
            Some(inc_path.to_path_buf())
        } else {
            // Try each search path.
            paths.iter().map(|d| d.join(inc_str)).find(|c| c.exists())
        };

        if let Some(resolved) = resolved {
            collect_includes_recursive(&resolved, &paths, seen, result);
        }
    }
}

/// Extract #include paths from DTS content using regex-like line parsing.
/// Used as fallback when the parser fails on a file.
fn extract_includes_regex(content: &str) -> Vec<String> {
    let mut includes = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("#include") {
            let rest = rest.trim();
            if let Some(path) = rest.strip_prefix('<').and_then(|r| r.strip_suffix('>')) {
                includes.push(path.to_string());
            } else if let Some(path) = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
                includes.push(path.to_string());
            }
        }
    }
    includes
}

/// Scan user-defined overlay/DTS files outside .west/.
pub async fn scan_user_overlays(workspace_root: &Path) -> Vec<FileEntry> {
    let root = workspace_root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        collect_user_overlays(&root, &root, &mut entries);
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    })
    .await
    .unwrap_or_default()
}

fn collect_user_overlays(dir: &Path, root: &Path, entries: &mut Vec<FileEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip .west, .git, build, modules, bootloader, zephyr directories.
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.')
                || name == "build"
                || name == "target"
                || name == "modules"
                || name == "bootloader"
                || name == "zephyr"
            {
                continue;
            }
            collect_user_overlays(&path, root, entries);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "overlay" | "dtso" | "dts" | "dtsi" => {
                    let display = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .display()
                        .to_string();
                    if let Some(kind) = FileKind::from_extension(ext) {
                        entries.push(FileEntry {
                            path: path.clone(),
                            name: display,
                            kind,
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

/// Scan binding YAML files from zephyr/dts/bindings/.
pub async fn scan_bindings(zephyr_dir: &Path) -> Vec<FileEntry> {
    let bindings_dir = zephyr_dir.join("dts").join("bindings");
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        collect_binding_entries(&bindings_dir, &bindings_dir, &mut entries);
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    })
    .await
    .unwrap_or_default()
}

fn collect_binding_entries(dir: &Path, root: &Path, entries: &mut Vec<FileEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_binding_entries(&path, root, entries);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "yaml" || ext == "yml" {
                let display = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                entries.push(FileEntry {
                    path: path.clone(),
                    name: display,
                    kind: FileKind::Binding,
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_dts_entries(dir: &Path, entries: &mut Vec<FileEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dts_entries(&path, entries);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if let Some(kind) = FileKind::from_extension(ext) {
                if matches!(kind, FileKind::Dts | FileKind::Dtsi | FileKind::Overlay) {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?")
                        .to_string();
                    entries.push(FileEntry { path, name, kind });
                }
            }
        }
    }
}

/// Resolve an include path to an absolute file path using the workspace search
/// paths.  `origin` is the directory of the file that contains the include.
/// For binding YAML includes, also searches the `bindings/` subdirectories.
pub fn resolve_include(
    ws: &WorkspaceState,
    origin: &Path,
    include_name: &str,
) -> Option<PathBuf> {
    let inc_path = std::path::Path::new(include_name);

    // If already absolute and exists, return it.
    if inc_path.is_absolute() && inc_path.exists() {
        return Some(inc_path.to_path_buf());
    }

    // Build candidate dirs: origin directory first, then standard search paths.
    let mut dirs = vec![origin.to_path_buf()];

    let zephyr_dir = &ws.info.zephyr_dir;
    let dts_dir = zephyr_dir.join("dts");

    // Binding YAML dirs.
    let bindings_dir = dts_dir.join("bindings");
    if bindings_dir.is_dir() {
        dirs.push(bindings_dir.clone());
        // Also add all subdirectories of bindings/ (one level deep).
        if let Ok(rd) = std::fs::read_dir(&bindings_dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    dirs.push(p);
                }
            }
        }
    }

    // DTS search dirs.
    let common = dts_dir.join("common");
    if common.is_dir() {
        dirs.push(common);
    }
    if let Ok(rd) = std::fs::read_dir(&dts_dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "bindings" && name != "common" && name != "vendor" {
                    dirs.push(p);
                }
            }
        }
    }
    dirs.push(dts_dir);
    dirs.push(zephyr_dir.to_path_buf());

    // HAL module DTS and bindings directories.
    for entry in &ws.hal_entries {
        if let Some(dts_path) = &entry.dts_path {
            if dts_path.is_dir() {
                dirs.push(dts_path.clone());
            }
        }
        if let Some(bp) = &entry.bindings_path {
            if bp.is_dir() {
                dirs.push(bp.clone());
                // Also search one level of subdirectories.
                if let Ok(rd) = std::fs::read_dir(bp) {
                    for e in rd.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            dirs.push(p);
                        }
                    }
                }
            }
        }
    }

    let result = dirs.iter().map(|d| d.join(include_name)).find(|c| c.exists());
    match &result {
        Some(p) => tui_log!("Resolved include '{}' -> {}", include_name, p.display()),
        None => tui_warn!("Could not resolve include: '{}'", include_name),
    }
    result
}
