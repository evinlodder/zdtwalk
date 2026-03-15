use std::path::Path;

use super::error::WestError;

/// Read the Zephyr SDK version from `<zephyr_dir>/SDK_VERSION`.
///
/// Returns `"unknown"` if the file does not exist.
pub fn get_sdk_version(zephyr_dir: &Path) -> Result<String, WestError> {
    let version_path = zephyr_dir.join("SDK_VERSION");
    match std::fs::read_to_string(&version_path) {
        Ok(content) => {
            let version = content.trim().to_string();
            if version.is_empty() {
                Ok("unknown".to_string())
            } else {
                Ok(version)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("unknown".to_string()),
        Err(e) => Err(WestError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_get_sdk_version() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_version");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("SDK_VERSION"), "  0.17.0  \n").unwrap();

        let ver = get_sdk_version(&tmp).unwrap();
        assert_eq!(ver, "0.17.0");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_get_sdk_version_missing_file() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_version_missing");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let ver = get_sdk_version(&tmp).unwrap();
        assert_eq!(ver, "unknown");

        let _ = fs::remove_dir_all(&tmp);
    }
}
