use std::path::{Path, PathBuf};
use std::process::Command;

use super::cache;
use super::discovery::WorkspaceInfo;
use super::error::WestError;
use super::manifest::{self, ManifestContent, Project};
use super::version;
use crate::{tui_log, tui_warn, tui_error};

/// Describes where a HAL project's DTS files were sourced from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DtsSource {
    /// Found in the local workspace checkout.
    Local,
    /// Loaded from the zdtwalk cache.
    Cached,
    /// Freshly fetched from the remote git repository.
    Fetched,
    /// The repository does not contain a dts/ or bindings/ directory.
    NoDts,
}

/// A resolved HAL DTS source.
#[derive(Debug, Clone)]
pub struct HalDtsEntry {
    pub project_name: String,
    /// Path to the dts/ directory (local or cached).
    pub dts_path: Option<PathBuf>,
    /// Path to the bindings/ directory (local or cached).
    pub bindings_path: Option<PathBuf>,
    pub source: DtsSource,
}

/// Fetch DTS + bindings for all HAL projects defined in the workspace manifest.
///
/// Resolution order per project:
/// 1. Local workspace copy (`<workspace_root>/<project.path>/dts/`)
/// 2. zdtwalk cache (`~/.cache/zdtwalk/hal_dts/<zephyr_version>/<name>/`)
/// 3. Sparse git checkout from remote (only fetches `dts/` and `bindings/`)
///
/// `progress_cb` is called with `(project_index, total, project_name)` for UI feedback.
pub fn fetch_all_hal_dts(
    workspace: &WorkspaceInfo,
    force: bool,
    mut progress_cb: impl FnMut(usize, usize, &str),
) -> Result<Vec<HalDtsEntry>, WestError> {
    let zephyr_version = version::get_zephyr_version(&workspace.zephyr_dir)?;

    // Parse the primary manifest and collect HAL projects.  If the primary
    // manifest imports another (common with Zephyr's two-level manifests),
    // follow those imports to find the full set of HAL projects.
    let (hal_projects, merged_manifest) = collect_hal_projects(workspace)?;
    let total = hal_projects.len();

    let mut results = Vec::with_capacity(total);

    for (idx, project) in hal_projects.iter().enumerate() {
        progress_cb(idx, total, &project.name);
        let entry = resolve_hal_project(
            workspace,
            &merged_manifest,
            project,
            &zephyr_version,
            force,
        )?;
        results.push(entry);
    }

    Ok(results)
}

/// Collect HAL projects from the workspace manifest, following `import: true`
/// directives to discover projects from imported manifests (e.g. zephyr/west.yml).
fn collect_hal_projects(
    workspace: &WorkspaceInfo,
) -> Result<(Vec<Project>, ManifestContent), WestError> {
    tui_log!("Parsing west manifest: {}", workspace.west_yml_path.display());
    let west = manifest::parse_west_manifest(&workspace.west_yml_path)?;
    let mut all_projects = west.manifest.projects.clone();
    let mut remotes = west.manifest.remotes.clone();
    let defaults = west.manifest.defaults.clone();

    tui_log!("Primary manifest: {} projects, {} remotes",
        west.manifest.projects.len(), remotes.len());

    // Follow imports: for each project with `import`, parse its west.yml.
    for project in &west.manifest.projects {
        if project.has_import() {
            let project_dir = workspace.workspace_root.join(project.local_path());
            let imported_yml = project_dir.join("west.yml");
            tui_log!("Following import from project '{}' -> {}", project.name, imported_yml.display());
            if imported_yml.exists() {
                if let Ok(imported) = manifest::parse_west_manifest(&imported_yml) {
                    tui_log!("  Imported {} projects, {} remotes",
                        imported.manifest.projects.len(), imported.manifest.remotes.len());
                    // Merge remotes (avoid duplicates by name).
                    for r in imported.manifest.remotes {
                        if !remotes.iter().any(|existing| existing.name == r.name) {
                            remotes.push(r);
                        }
                    }
                    // Merge projects (avoid duplicates by name).
                    for p in imported.manifest.projects {
                        if !all_projects.iter().any(|existing| existing.name == p.name) {
                            all_projects.push(p);
                        }
                    }
                } else {
                    tui_warn!("  Failed to parse imported manifest: {}", imported_yml.display());
                }
            } else {
                tui_warn!("  Imported manifest does not exist: {}", imported_yml.display());
            }
        }
    }

    let merged = ManifestContent {
        defaults,
        remotes,
        projects: all_projects.clone(),
        group_filter: west.manifest.group_filter,
    };

    let hal_projects: Vec<Project> = merged
        .projects
        .iter()
        .filter(|p| p.groups.iter().any(|g| g == "hal"))
        .cloned()
        .collect();

    tui_log!("Merged manifest: {} total projects, {} HAL projects",
        merged.projects.len(), hal_projects.len());
    for p in &hal_projects {
        tui_log!("  HAL: {} (path={})", p.name, p.local_path());
    }

    Ok((hal_projects, merged))
}

/// Resolve a single HAL project's DTS + bindings files.
fn resolve_hal_project(
    workspace: &WorkspaceInfo,
    manifest: &ManifestContent,
    project: &Project,
    zephyr_version: &str,
    force: bool,
) -> Result<HalDtsEntry, WestError> {
    let name = &project.name;
    let local_root = workspace.workspace_root.join(project.local_path());

    // 1. Check local workspace.
    let local_dts = local_root.join("dts");
    let local_bindings = local_root.join("bindings");
    let has_local_dts = local_dts.is_dir();
    let has_local_bindings = local_bindings.is_dir();

    if has_local_dts || has_local_bindings {
        return Ok(HalDtsEntry {
            project_name: name.clone(),
            dts_path: if has_local_dts { Some(local_dts) } else { None },
            bindings_path: if has_local_bindings { Some(local_bindings) } else { None },
            source: DtsSource::Local,
        });
    }

    // 2. Check cache (unless --force).
    if !force {
        if cache::is_marked_no_dts(zephyr_version, name)? {
            return Ok(HalDtsEntry {
                project_name: name.clone(),
                dts_path: None,
                bindings_path: None,
                source: DtsSource::NoDts,
            });
        }
        let cached_dts = cache::dts_cache_path(zephyr_version, name)?;
        let cached_bindings = cache::bindings_cache_path(zephyr_version, name)?;
        let has_cached_dts = cached_dts.is_dir();
        let has_cached_bindings = cached_bindings.is_dir();

        if has_cached_dts || has_cached_bindings {
            return Ok(HalDtsEntry {
                project_name: name.clone(),
                dts_path: if has_cached_dts { Some(cached_dts) } else { None },
                bindings_path: if has_cached_bindings { Some(cached_bindings) } else { None },
                source: DtsSource::Cached,
            });
        }
    }

    // 3. Fetch from remote via sparse checkout.
    let git_url = manifest.resolve_git_url(project).ok_or_else(|| {
        WestError::Git(format!("cannot resolve git URL for project '{name}'"))
    })?;

    fetch_hal_sparse(name, &git_url, &project.revision, zephyr_version)
}

/// Perform a sparse git checkout to fetch `dts/` and `bindings/` directories.
fn fetch_hal_sparse(
    project_name: &str,
    git_url: &str,
    revision: &str,
    zephyr_version: &str,
) -> Result<HalDtsEntry, WestError> {
    let tmp_dir = std::env::temp_dir().join(format!("zdtwalk_fetch_{project_name}"));

    // Clean up any leftover temp dir from a previous interrupted run.
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }
    std::fs::create_dir_all(&tmp_dir)?;

    let result = do_sparse_checkout(&tmp_dir, git_url, revision);

    let entry = match result {
        Ok(()) => {
            let fetched_dts = tmp_dir.join("dts");
            let fetched_bindings = tmp_dir.join("bindings");
            let has_dts = fetched_dts.is_dir();
            let has_bindings = fetched_bindings.is_dir();

            if !has_dts && !has_bindings {
                // Repo has neither directory — mark as no-dts.
                cache::mark_no_dts(zephyr_version, project_name)?;
                HalDtsEntry {
                    project_name: project_name.to_string(),
                    dts_path: None,
                    bindings_path: None,
                    source: DtsSource::NoDts,
                }
            } else {
                let mut dts_path = None;
                let mut bindings_path = None;

                if has_dts {
                    let cache_dts = cache::dts_cache_path(zephyr_version, project_name)?;
                    std::fs::create_dir_all(cache_dts.parent().unwrap())?;
                    copy_dir_recursive(&fetched_dts, &cache_dts)?;
                    dts_path = Some(cache_dts);
                }
                if has_bindings {
                    let cache_bindings = cache::bindings_cache_path(zephyr_version, project_name)?;
                    std::fs::create_dir_all(cache_bindings.parent().unwrap())?;
                    copy_dir_recursive(&fetched_bindings, &cache_bindings)?;
                    bindings_path = Some(cache_bindings);
                }

                HalDtsEntry {
                    project_name: project_name.to_string(),
                    dts_path,
                    bindings_path,
                    source: DtsSource::Fetched,
                }
            }
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }
    };

    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(entry)
}

/// Execute the git sparse checkout sequence for dts/ and bindings/.
fn do_sparse_checkout(work_dir: &Path, git_url: &str, revision: &str) -> Result<(), WestError> {
    run_git(work_dir, &["init"])?;
    run_git(work_dir, &["remote", "add", "origin", git_url])?;
    run_git(work_dir, &["sparse-checkout", "init"])?;
    run_git(work_dir, &["sparse-checkout", "set", "dts/", "bindings/"])?;
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
