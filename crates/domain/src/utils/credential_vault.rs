//! Repo-local durable home for database admin passwords (Ubuntu keyring style).
//!
//! Passwords live under `{repo}/.gfs/secrets/` with mode `0600` — same trust
//! boundary as the GFS repository on the client machine. Kubernetes also keeps
//! a runtime Secret for pod injection; this file store is the portable SoT for
//! reveal / CLI / docker (where env read-back can be lost).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SECRETS_DIR: &str = "secrets";
const ADMIN_PASSWORD_FILE: &str = "admin_password";

/// File-backed credential vault rooted at a GFS repository's `.gfs` directory.
#[derive(Debug, Clone)]
pub struct RepoCredentialVault {
    secrets_dir: PathBuf,
}

impl RepoCredentialVault {
    /// `repo_path` is the repository root (the directory that contains `.gfs/`).
    pub fn open(repo_path: &Path) -> Self {
        Self {
            secrets_dir: repo_path.join(".gfs").join(SECRETS_DIR),
        }
    }

    pub fn admin_password_path(&self) -> PathBuf {
        self.secrets_dir.join(ADMIN_PASSWORD_FILE)
    }

    pub fn put_admin_password(&self, password: &str) -> io::Result<()> {
        fs::create_dir_all(&self.secrets_dir)?;
        let path = self.admin_password_path();
        fs::write(&path, password.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            fs::set_permissions(&self.secrets_dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    pub fn get_admin_password(&self) -> io::Result<Option<String>> {
        let path = self.admin_password_path();
        match fs::read_to_string(&path) {
            Ok(s) => {
                let trimmed = s.trim_end_matches(['\n', '\r']).to_string();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed))
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn delete_admin_password(&self) -> io::Result<()> {
        let path = self.admin_password_path();
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Copy admin password from another repo's vault (seeded clone inherit).
    pub fn adopt_from(&self, source: &RepoCredentialVault) -> io::Result<()> {
        match source.get_admin_password()? {
            Some(pw) => self.put_admin_password(&pw),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_put_get_delete() {
        let dir = tempdir().unwrap();
        let vault = RepoCredentialVault::open(dir.path());
        assert!(vault.get_admin_password().unwrap().is_none());
        vault.put_admin_password("sekret").unwrap();
        assert_eq!(
            vault.get_admin_password().unwrap().as_deref(),
            Some("sekret")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(vault.admin_password_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        vault.delete_admin_password().unwrap();
        assert!(vault.get_admin_password().unwrap().is_none());
    }

    #[test]
    fn adopt_copies_password() {
        let dir = tempdir().unwrap();
        let parent = dir.path().join("parent");
        let child = dir.path().join("child");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(&child).unwrap();
        let src = RepoCredentialVault::open(&parent);
        let dst = RepoCredentialVault::open(&child);
        src.put_admin_password("parent-pw").unwrap();
        dst.adopt_from(&src).unwrap();
        assert_eq!(
            dst.get_admin_password().unwrap().as_deref(),
            Some("parent-pw")
        );
    }
}
