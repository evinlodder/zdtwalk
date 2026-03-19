use std::path::Path;

use super::error::WestError;

/// Read the Zephyr kernel version from `<zephyr_dir>/VERSION`.
///
/// The file has the format:
/// ```text
/// VERSION_MAJOR = 4
/// VERSION_MINOR = 3
/// PATCHLEVEL = 0
/// VERSION_TWEAK = 0
/// EXTRAVERSION = rc2
/// ```
///
/// Returns `"unknown"` if the file does not exist or cannot be parsed.
pub fn get_zephyr_version(zephyr_dir: &Path) -> Result<String, WestError> {
    let version_path = zephyr_dir.join("VERSION");
    match std::fs::read_to_string(&version_path) {
        Ok(content) => Ok(parse_version_file(&content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("unknown".to_string()),
        Err(e) => Err(WestError::Io(e)),
    }
}

fn parse_version_file(content: &str) -> String {
    let mut major = None;
    let mut minor = None;
    let mut patch = None;
    let mut extra = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "VERSION_MAJOR" => major = Some(val.to_string()),
                "VERSION_MINOR" => minor = Some(val.to_string()),
                "PATCHLEVEL" => patch = Some(val.to_string()),
                "EXTRAVERSION" if !val.is_empty() => extra = Some(val.to_string()),
                _ => {}
            }
        }
    }

    match (major, minor, patch) {
        (Some(ma), Some(mi), Some(pa)) => {
            let base = format!("{ma}.{mi}.{pa}");
            match extra {
                Some(ex) => format!("{base}-{ex}"),
                None => base,
            }
        }
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_get_zephyr_version() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_version");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("VERSION"),
            "VERSION_MAJOR = 4\nVERSION_MINOR = 3\nPATCHLEVEL = 0\nVERSION_TWEAK = 0\nEXTRAVERSION = rc2\n",
        )
        .unwrap();

        let ver = get_zephyr_version(&tmp).unwrap();
        assert_eq!(ver, "4.3.0-rc2");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_get_zephyr_version_no_extra() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_version_noextra");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("VERSION"),
            "VERSION_MAJOR = 3\nVERSION_MINOR = 7\nPATCHLEVEL = 1\n",
        )
        .unwrap();

        let ver = get_zephyr_version(&tmp).unwrap();
        assert_eq!(ver, "3.7.1");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_get_zephyr_version_missing_file() {
        let tmp = std::env::temp_dir().join("zdtwalk_test_version_missing");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let ver = get_zephyr_version(&tmp).unwrap();
        assert_eq!(ver, "unknown");

        let _ = fs::remove_dir_all(&tmp);
    }
}
