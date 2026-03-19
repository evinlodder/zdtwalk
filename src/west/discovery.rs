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
    let config = parse_west_config(root)?;

    // The manifest path is where west.yml lives.
    let manifest_dir = root.join(&config.manifest_path);
    let west_yml = manifest_dir.join(config.manifest_file.as_deref().unwrap_or("west.yml"));

    // Resolve the actual Zephyr directory:
    // 1. Use [zephyr] base if present and valid
    // 2. Fall back to root/zephyr/ if it contains VERSION
    // 3. Fall back to the manifest path
    let zephyr_dir = if let Some(base) = &config.zephyr_base {
        let candidate = root.join(base);
        if candidate.join("VERSION").exists() {
            candidate
        } else {
            manifest_dir.clone()
        }
    } else {
        // No [zephyr] base — try root/zephyr/ first.
        let candidate = root.join("zephyr");
        if candidate.join("VERSION").exists() {
            candidate
        } else {
            manifest_dir.clone()
        }
    };

    Ok(WorkspaceInfo {
        workspace_root: root.to_path_buf(),
        zephyr_dir,
        west_yml_path: west_yml,
    })
}

/// Parsed values from `.west/config`.
struct WestConfig {
    manifest_path: String,
    manifest_file: Option<String>,
    zephyr_base: Option<String>,
}

/// Parse `.west/config` to extract relevant paths.
///
/// The file is a simple INI-like format:
/// ```ini
/// [manifest]
/// path = zephyr
/// file = west.yml
///
/// [zephyr]
/// base = zephyr
/// ```
fn parse_west_config(root: &Path) -> Result<WestConfig, WestError> {
    let config_path = root.join(".west").join("config");
    let content = std::fs::read_to_string(&config_path)?;

    let mut manifest_path = None;
    let mut manifest_file = None;
    let mut zephyr_base = None;
    let mut current_section = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_string();
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match (current_section.as_str(), key) {
                ("manifest", "path") => manifest_path = Some(val.to_string()),
                ("manifest", "file") => manifest_file = Some(val.to_string()),
                ("zephyr", "base") => zephyr_base = Some(val.to_string()),
                _ => {}
            }
        }
    }

    Ok(WestConfig {
        manifest_path: manifest_path.unwrap_or_else(|| "zephyr".to_string()),
        manifest_file,
        zephyr_base,
    })
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
        // Create a west.yml and VERSION so the path resolves correctly.
        fs::write(zephyr_dir.join("west.yml"), "").unwrap();
        fs::write(zephyr_dir.join("VERSION"), "VERSION_MAJOR = 3\nVERSION_MINOR = 0\nPATCHLEVEL = 0\n").unwrap();

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
        fs::write(zephyr_dir.join("VERSION"), "VERSION_MAJOR = 3\nVERSION_MINOR = 0\nPATCHLEVEL = 0\n").unwrap();

        let info = find_workspace(&tmp).unwrap();
        assert_eq!(info.workspace_root, ws);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_find_workspace_zephyr_base() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_discovery_base");
        let _ = fs::remove_dir_all(&tmp);

        // Simulate a workspace where manifest path != zephyr base.
        let west_dir = tmp.join(".west");
        let zephyr_dir = tmp.join("zephyr");
        fs::create_dir_all(&west_dir).unwrap();
        fs::create_dir_all(&zephyr_dir).unwrap();
        fs::write(
            west_dir.join("config"),
            "[manifest]\npath = .west\nfile = west.yml\n\n[zephyr]\nbase = zephyr\n",
        )
        .unwrap();
        fs::write(west_dir.join("west.yml"), "").unwrap();
        fs::write(zephyr_dir.join("VERSION"), "VERSION_MAJOR = 4\nVERSION_MINOR = 3\nPATCHLEVEL = 0\n").unwrap();

        let info = find_workspace(&tmp).unwrap();
        assert_eq!(info.workspace_root, tmp);
        // zephyr_dir should resolve to zephyr/ via [zephyr] base, NOT .west/.
        assert_eq!(info.zephyr_dir, zephyr_dir);
        // west.yml should be in .west/.
        assert_eq!(info.west_yml_path, west_dir.join("west.yml"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
