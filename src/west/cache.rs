use std::path::PathBuf;

use super::error::WestError;

const APP_NAME: &str = "zdtwalk";
const HAL_DTS_DIR: &str = "hal_dts";

/// Return the base cache directory for zdtwalk: `<cache_dir>/zdtwalk/`.
fn base_cache_dir() -> Result<PathBuf, WestError> {
    let cache = dirs::cache_dir().ok_or(WestError::NoCacheDir)?;
    Ok(cache.join(APP_NAME))
}

/// Return the cache path for a specific HAL project's DTS files:
/// `<cache_dir>/zdtwalk/hal_dts/<sdk_version>/<project_name>/`
pub fn cache_path_for(sdk_version: &str, project_name: &str) -> Result<PathBuf, WestError> {
    Ok(base_cache_dir()?.join(HAL_DTS_DIR).join(sdk_version).join(project_name))
}

/// Return the cache path specifically for the dts/ sub-directory of a HAL project.
pub fn dts_cache_path(sdk_version: &str, project_name: &str) -> Result<PathBuf, WestError> {
    Ok(cache_path_for(sdk_version, project_name)?.join("dts"))
}

/// Return the cache path specifically for the bindings/ sub-directory of a HAL project.
pub fn bindings_cache_path(sdk_version: &str, project_name: &str) -> Result<PathBuf, WestError> {
    Ok(cache_path_for(sdk_version, project_name)?.join("bindings"))
}

/// Check whether DTS files are already cached for this project+version.
#[allow(dead_code)]
pub fn is_cached(sdk_version: &str, project_name: &str) -> Result<bool, WestError> {
    let dts_dir = dts_cache_path(sdk_version, project_name)?;
    if dts_dir.is_dir() {
        // Check it has at least one entry.
        if let Ok(mut entries) = std::fs::read_dir(&dts_dir) {
            return Ok(entries.next().is_some());
        }
    }
    // Also consider it "cached" if we have a no-dts marker.
    let marker = cache_path_for(sdk_version, project_name)?.join(".no_dts");
    Ok(marker.exists())
}

/// Check whether a project has been marked as having no dts/ directory.
pub fn is_marked_no_dts(sdk_version: &str, project_name: &str) -> Result<bool, WestError> {
    let marker = cache_path_for(sdk_version, project_name)?.join(".no_dts");
    Ok(marker.exists())
}

/// Write the `.no_dts` marker for a project that has no dts/ directory.
pub fn mark_no_dts(sdk_version: &str, project_name: &str) -> Result<(), WestError> {
    let project_dir = cache_path_for(sdk_version, project_name)?;
    std::fs::create_dir_all(&project_dir)?;
    std::fs::write(project_dir.join(".no_dts"), "")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_path_structure() {
        let path = cache_path_for("0.17.0", "hal_stm32").unwrap();
        assert!(path.ends_with("zdtwalk/hal_dts/0.17.0/hal_stm32"));
    }

    #[test]
    fn test_dts_cache_path_structure() {
        let path = dts_cache_path("0.17.0", "hal_stm32").unwrap();
        assert!(path.ends_with("zdtwalk/hal_dts/0.17.0/hal_stm32/dts"));
    }

    #[test]
    fn test_is_cached_empty() {
        // For a project that doesn't exist yet, should return false.
        let result = is_cached("99.99.99", "nonexistent_hal_project").unwrap();
        assert!(!result);
    }
}
