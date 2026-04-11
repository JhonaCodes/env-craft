use std::path::Path;

use anyhow::Result;

/// Write `content` to `path` and restrict the file to owner-only access (0o600).
pub fn write_secret_file(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::write(path, content)?;
    restrict_file(path)?;
    Ok(())
}

/// Restrict an existing file to owner read/write only.
pub fn restrict_file(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Create a directory (and parents) with owner-only access (0o700).
pub fn create_restricted_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn write_secret_file_sets_owner_only_permissions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("secret.txt");
        write_secret_file(&path, b"top-secret").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "top-secret");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn create_restricted_dir_sets_owner_only_permissions() {
        let dir = tempdir().unwrap();
        let restricted = dir.path().join("private");
        create_restricted_dir(&restricted).unwrap();
        assert!(restricted.is_dir());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&restricted).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn restrict_file_changes_existing_file_permissions() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "data").unwrap();
        restrict_file(&path).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}
