use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::error::Error;
use super::model::*;
use super::parser::{self, merge_nodes};

/// Recursively resolves `#include` / `/include/` directives by reading the
/// referenced files from disk and merging their contents into a single
/// [`DeviceTree`].
pub struct Resolver {
    search_paths: Vec<PathBuf>,
    resolved: HashSet<PathBuf>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            search_paths: Vec::new(),
            resolved: HashSet::new(),
        }
    }

    /// Add a directory that will be searched when resolving include paths.
    pub fn add_search_path(&mut self, path: impl Into<PathBuf>) {
        self.search_paths.push(path.into());
    }

    /// Parse the file at `path` and recursively resolve all includes.
    pub fn resolve_file(&mut self, path: &Path) -> Result<DeviceTree, Error> {
        let canonical = path.canonicalize().map_err(|e| {
            Error::Include(format!("cannot canonicalize {}: {}", path.display(), e))
        })?;

        // Prevent infinite include loops.
        if self.resolved.contains(&canonical) {
            return Ok(DeviceTree::new());
        }
        self.resolved.insert(canonical.clone());

        let content = std::fs::read_to_string(&canonical)?;
        let mut tree = parser::parse_dts(&content)?;

        // Add the file's parent directory as a search path.
        if let Some(parent) = canonical.parent() {
            let parent_buf = parent.to_path_buf();
            if !self.search_paths.contains(&parent_buf) {
                self.search_paths.push(parent_buf);
            }
        }

        // Resolve every include we found.
        let includes = std::mem::take(&mut tree.includes);
        for inc in &includes {
            if let Some(resolved_path) = self.find_include(&inc.path) {
                let included = self.resolve_file(&resolved_path)?;
                merge_trees(&mut tree, included);
            }
        }
        tree.includes = includes;

        Ok(tree)
    }

    fn find_include(&self, path: &str) -> Option<PathBuf> {
        let p = Path::new(path);

        if p.is_absolute() && p.exists() {
            return Some(p.to_path_buf());
        }

        for dir in &self.search_paths {
            let candidate = dir.join(path);
            if candidate.exists() {
                return Some(candidate);
            }
        }

        None
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tree merging
// ---------------------------------------------------------------------------

fn merge_trees(target: &mut DeviceTree, source: DeviceTree) {
    if target.version.is_none() {
        target.version = source.version;
    }
    target.is_plugin = target.is_plugin || source.is_plugin;
    target.memory_reservations.extend(source.memory_reservations);

    if let Some(src_root) = source.root {
        if let Some(ref mut tgt_root) = target.root {
            merge_nodes(tgt_root, src_root);
        } else {
            target.root = Some(src_root);
        }
    }

    target.reference_nodes.extend(source.reference_nodes);
}
