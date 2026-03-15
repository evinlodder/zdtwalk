use std::path::{Path, PathBuf};
use std::process::Command;

use super::cache;
use super::discovery::WorkspaceInfo;
use super::error::WestError;
use super::manifest::{self, ManifestContent, Project};
use super::version;

/// Describes where a HAL project's DTS files were sourced from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DtsSource {
    /// Found in the local workspace checkout.
    Local,
    /// Loaded from the zdtwalk cache.
    Cached,
    /// Freshly fetched from the remote git repository.
    Fetched,
    /// The repository does not contain a dts/ directory.
    NoDts,
}

/// A resolved HAL DTS source.
#[derive(Debug, Clone)]
pub struct HalDtsEntry {
    pub project_name: String,
    pub path: Option<PathBuf>,
    pub source: DtsSource,
}

/// Fetch DTS files for all HAL projects defined in the workspace manifest.
///
/// For each HAL project the resolution order is:
/// 1. Local workspace copy (`<workspace_root>/<project.path>/dts/`)
/// 2. zdtwalk cache (`~/.cache/zdtwalk/hal_dts/<version>/<name>/dts/`)
/// 3. Sparse git checkout from remote
pub fn fetch_all_hal_dts(
    workspace: &WorkspaceInfo,
    force: bool,
) -> Result<Vec<HalDtsEntry>, WestError> {
    let west = manifest::parse_west_manifest(&workspace.west_yml_path)?;
    let sdk_version = version::get_sdk_version(&workspace.zephyr_dir)?;
    let hal_projects = west.manifest.hal_projects();

    let mut results = Vec::with_capacity(hal_projects.len());

    for project in &hal_projects {
        let entry = resolve_hal_project(
            workspace,
            &west.manifest,
            project,
            &sdk_version,
            force,
        )?;
        results.push(entry);
    }

    Ok(results)
}

/// Resolve a single HAL project's DTS files.
fn resolve_hal_project(
    workspace: &WorkspaceInfo,
    manifest: &ManifestContent,
    project: &Project,
    sdk_version: &str,
    force: bool,
) -> Result<HalDtsEntry, WestError> {
    let name = &project.name;

    // 1. Check local workspace.
    let local_dts = workspace
        .workspace_root
        .join(project.local_path())
        .join("dts");
    if local_dts.is_dir() {
        return Ok(HalDtsEntry {
            project_name: name.clone(),
            path: Some(local_dts),
            source: DtsSource::Local,
        });
    }

    // 2. Check cache (unless --force).
    if !force {
        if cache::is_marked_no_dts(sdk_version, name)? {
            return Ok(HalDtsEntry {
                project_name: name.clone(),
                path: None,
                source: DtsSource::NoDts,
            });
        }
        if cache::is_cached(sdk_version, name)? {
            let cached_path = cache::dts_cache_path(sdk_version, name)?;
            return Ok(HalDtsEntry {
                project_name: name.clone(),
                path: Some(cached_path),
                source: DtsSource::Cached,
            });
        }
    }

    // 3. Fetch from remote via sparse checkout.
    let git_url = manifest.resolve_git_url(project).ok_or_else(|| {
        WestError::Git(format!("cannot resolve git URL for project '{name}'"))
    })?;

    fetch_dts_sparse(name, &git_url, &project.revision, sdk_version)
}

/// Perform a sparse git checkout to fetch only the `dts/` directory.
fn fetch_dts_sparse(
    project_name: &str,
    git_url: &str,
    revision: &str,
    sdk_version: &str,
) -> Result<HalDtsEntry, WestError> {
    let tmp_dir = std::env::temp_dir().join(format!("zdtwalk_fetch_{project_name}"));

    // Clean up any leftover temp dir from a previous interrupted run.
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }
    std::fs::create_dir_all(&tmp_dir)?;

    let result = do_sparse_checkout(&tmp_dir, git_url, revision);

    // Regardless of outcome, check for dts/ and cache appropriately.
    let entry = match result {
        Ok(()) => {
            let fetched_dts = tmp_dir.join("dts");
            if fetched_dts.is_dir() {
                let cache_dts = cache::dts_cache_path(sdk_version, project_name)?;
                std::fs::create_dir_all(cache_dts.parent().unwrap())?;
                copy_dir_recursive(&fetched_dts, &cache_dts)?;

                HalDtsEntry {
                    project_name: project_name.to_string(),
                    path: Some(cache_dts),
                    source: DtsSource::Fetched,
                }
            } else {
                // Repo exists but has no dts/ directory.
                cache::mark_no_dts(sdk_version, project_name)?;
                HalDtsEntry {
                    project_name: project_name.to_string(),
                    path: None,
                    source: DtsSource::NoDts,
                }
            }
        }
        Err(e) => {
            // Clean up and propagate error.
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }
    };

    // Clean up temp dir.
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(entry)
}

/// Execute the git sparse checkout sequence.
fn do_sparse_checkout(work_dir: &Path, git_url: &str, revision: &str) -> Result<(), WestError> {
    run_git(work_dir, &["init"])?;
    run_git(work_dir, &["remote", "add", "origin", git_url])?;
    run_git(work_dir, &["sparse-checkout", "init"])?;
    run_git(work_dir, &["sparse-checkout", "set", "dts/"])?;
    run_git(
        work_dir,
        &["fetch", "--depth", "1", "origin", revision],
    )?;
    run_git(work_dir, &["checkout", "FETCH_HEAD"])?;

    Ok(())
}

/// Run a git command in the given working directory.
fn run_git(work_dir: &Path, args: &[&str]) -> Result<(), WestError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(work_dir)
        .output()
        .map_err(|e| WestError::Git(format!("failed to run git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WestError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(())
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), WestError> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
