use std::path::Path;

use serde::Deserialize;

use super::error::WestError;

// ---------------------------------------------------------------------------
// Serde models for west.yml
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WestManifest {
    pub manifest: ManifestContent,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ManifestContent {
    #[serde(default)]
    pub defaults: ManifestDefaults,
    #[serde(default)]
    pub remotes: Vec<Remote>,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    #[allow(dead_code)]
    pub group_filter: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ManifestDefaults {
    pub remote: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Remote {
    pub name: String,
    pub url_base: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub revision: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default)]
    pub repo_path: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    /// West manifest import directive.  Can be `true`, a string path, or a map.
    /// We only need to detect its presence to follow imports.
    #[serde(default)]
    pub import: Option<serde_yaml::Value>,
}

// ---------------------------------------------------------------------------
// Resolution helpers
// ---------------------------------------------------------------------------

impl ManifestContent {
    /// Return only projects whose `groups` list contains `"hal"`.
    #[allow(dead_code)]
    pub fn hal_projects(&self) -> Vec<&Project> {
        self.projects
            .iter()
            .filter(|p| p.groups.iter().any(|g| g == "hal"))
            .collect()
    }

    /// Resolve the full git URL for a project.
    pub fn resolve_git_url(&self, project: &Project) -> Option<String> {
        let remote_name = project
            .remote
            .as_deref()
            .or(self.defaults.remote.as_deref())?;

        let remote = self.remotes.iter().find(|r| r.name == remote_name)?;
        let repo = project.repo_path.as_deref().unwrap_or(&project.name);

        Some(format!("{}/{}", remote.url_base, repo))
    }
}

impl Project {
    /// The local path for this project, relative to the workspace root.
    /// Falls back to the project name if `path` is not set.
    pub fn local_path(&self) -> &str {
        self.path.as_deref().unwrap_or(&self.name)
    }

    /// Whether this project has an `import` directive (i.e. its manifest
    /// should be recursively merged).
    pub fn has_import(&self) -> bool {
        self.import.is_some()
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a west manifest YAML file from disk.
pub fn parse_west_manifest(path: &Path) -> Result<WestManifest, WestError> {
    let content = std::fs::read_to_string(path)?;
    let manifest: WestManifest = serde_yaml::from_str(&content)?;
    Ok(manifest)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn example_manifest() -> WestManifest {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/examplewest.yml");
        parse_west_manifest(&path).expect("failed to parse examplewest.yml")
    }

    #[test]
    fn test_parse_remotes() {
        let m = example_manifest();
        assert_eq!(m.manifest.remotes.len(), 2);
        assert_eq!(m.manifest.remotes[0].name, "upstream");
        assert_eq!(
            m.manifest.remotes[0].url_base,
            "https://github.com/zephyrproject-rtos"
        );
    }

    #[test]
    fn test_hal_project_count() {
        let m = example_manifest();
        let hals = m.manifest.hal_projects();
        // examplewest.yml has many HAL projects; just check it's a reasonable number.
        assert!(hals.len() > 20, "expected >20 HAL projects, got {}", hals.len());
    }

    #[test]
    fn test_resolve_git_url_default_remote() {
        let m = example_manifest();
        let stm32 = m.manifest.projects.iter().find(|p| p.name == "hal_stm32").unwrap();
        let url = m.manifest.resolve_git_url(stm32).unwrap();
        assert_eq!(url, "https://github.com/zephyrproject-rtos/hal_stm32");
    }

    #[test]
    fn test_resolve_git_url_with_repo_path() {
        let m = example_manifest();
        let bsim = m.manifest.projects.iter().find(|p| p.name == "babblesim_base").unwrap();
        let url = m.manifest.resolve_git_url(bsim).unwrap();
        assert_eq!(url, "https://github.com/BabbleSim/base");
    }

    #[test]
    fn test_project_local_path() {
        let m = example_manifest();
        let stm32 = m.manifest.projects.iter().find(|p| p.name == "hal_stm32").unwrap();
        assert_eq!(stm32.local_path(), "modules/hal/stm32");
    }
}
