use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use super::error::WestError;

/// Information about a discovered Zephyr workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    /// The root of the west workspace (directory containing `.west/`).
    pub workspace_root: PathBuf,
    /// The zephyr repository directory (e.g. `<root>/zephyr`).
    pub zephyr_dir: PathBuf,
    /// Path to the west manifest YAML file.
    pub west_yml_path: PathBuf,
}

/// Maximum number of parent directories to walk upward.
const MAX_UP_LEVELS: usize = 10;
/// Maximum depth for downward BFS search.
const MAX_DOWN_DEPTH: usize = 4;

/// Discover a Zephyr workspace from the given starting directory.
///
/// Strategy:
/// 1. Walk upward from `start` up to [`MAX_UP_LEVELS`] looking for `.west/`.
/// 2. If not found, BFS downward from `start` up to [`MAX_DOWN_DEPTH`].
pub fn find_workspace(start: &Path) -> Result<WorkspaceInfo, WestError> {
    // --- upward search ---
    let mut current = start.to_path_buf();
    for _ in 0..MAX_UP_LEVELS {
        if current.join(".west").is_dir() {
            return workspace_from_root(&current);
        }
        if !current.pop() {
            break;
        }
    }

    // --- downward BFS ---
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((start.to_path_buf(), 0));

    while let Some((dir, depth)) = queue.pop_front() {
        if depth > MAX_DOWN_DEPTH {
            continue;
        }
        if dir.join(".west").is_dir() {
            return workspace_from_root(&dir);
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && !is_hidden(&path) {
                    queue.push_back((path, depth + 1));
                }
            }
        }
    }

    Err(WestError::WorkspaceNotFound(start.to_path_buf()))
}

/// Build [`WorkspaceInfo`] from the workspace root by reading `.west/config`.
fn workspace_from_root(root: &Path) -> Result<WorkspaceInfo, WestError> {
    let manifest_path = read_manifest_path(root)?;
    let zephyr_dir = root.join(&manifest_path);
    let west_yml = zephyr_dir.join("west.yml");

    Ok(WorkspaceInfo {
        workspace_root: root.to_path_buf(),
        zephyr_dir,
        west_yml_path: west_yml,
    })
}

/// Parse `.west/config` to extract the manifest path.
///
/// The file is a simple INI-like format:
/// ```ini
/// [manifest]
/// path = zephyr
/// ```
fn read_manifest_path(root: &Path) -> Result<String, WestError> {
    let config_path = root.join(".west").join("config");
    let content = std::fs::read_to_string(&config_path)?;

    let mut in_manifest_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_manifest_section = trimmed == "[manifest]";
            continue;
        }
        if in_manifest_section {
            if let Some(rest) = trimmed.strip_prefix("path") {
                let rest = rest.trim_start();
                if let Some(value) = rest.strip_prefix('=') {
                    return Ok(value.trim().to_string());
                }
            }
        }
    }

    // Fallback: assume "zephyr" if no explicit path found.
    Ok("zephyr".to_string())
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map_or(false, |n| n.starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_find_workspace_upward() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_discovery_up");
        let _ = fs::remove_dir_all(&tmp);

        // Create workspace structure.
        let west_dir = tmp.join(".west");
        let zephyr_dir = tmp.join("zephyr");
        let nested = tmp.join("zephyr").join("boards").join("arm");
        fs::create_dir_all(&west_dir).unwrap();
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            west_dir.join("config"),
            "[manifest]\npath = zephyr\n",
        )
        .unwrap();
        // Create a west.yml so the path exists.
        fs::write(zephyr_dir.join("west.yml"), "").unwrap();

        let info = find_workspace(&nested).unwrap();
        assert_eq!(info.workspace_root, tmp);
        assert_eq!(info.zephyr_dir, zephyr_dir);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_find_workspace_downward() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_discovery_down");
        let _ = fs::remove_dir_all(&tmp);

        // Create workspace structure nested inside tmp.
        let ws = tmp.join("myproject");
        let west_dir = ws.join(".west");
        let zephyr_dir = ws.join("zephyr");
        fs::create_dir_all(&west_dir).unwrap();
        fs::create_dir_all(&zephyr_dir).unwrap();
        fs::write(
            west_dir.join("config"),
            "[manifest]\npath = zephyr\n",
        )
        .unwrap();
        fs::write(zephyr_dir.join("west.yml"), "").unwrap();

        let info = find_workspace(&tmp).unwrap();
        assert_eq!(info.workspace_root, ws);

        let _ = fs::remove_dir_all(&tmp);
    }
}
