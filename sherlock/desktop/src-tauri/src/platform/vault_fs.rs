//! OS-level primitives for encrypted vaults (gocryptfs + FUSE).
//!
//! All process spawning and mount-table inspection lives here so that the
//! rest of the app stays OS-agnostic. Linux and macOS share the gocryptfs
//! command line; Windows has no gocryptfs port, so every operation reports
//! `Unsupported` there.

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum VaultFsError {
    /// gocryptfs rejected the password (exit code 12).
    WrongPassword,
    /// The platform (or this machine) cannot run encrypted vaults.
    Unsupported(String),
    /// Any other failure, with the tool's stderr when available.
    Other(String),
}

impl std::fmt::Display for VaultFsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultFsError::WrongPassword => write!(f, "incorrect password"),
            VaultFsError::Unsupported(msg) => write!(f, "{msg}"),
            VaultFsError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

/// Exit status gocryptfs uses for a bad password.
const GOCRYPTFS_EXIT_WRONG_PASSWORD: i32 = 12;

/// Locate the gocryptfs binary, or explain why vaults are unavailable.
pub fn gocryptfs_binary() -> Result<PathBuf, VaultFsError> {
    #[cfg(target_os = "windows")]
    {
        Err(VaultFsError::Unsupported(
            "Encrypted vaults require gocryptfs, which is not available on Windows".to_string(),
        ))
    }
    #[cfg(not(target_os = "windows"))]
    {
        super::process::find_executable("gocryptfs").ok_or_else(|| {
            VaultFsError::Unsupported(
                "gocryptfs is not installed. Install it with your package manager \
                 (e.g. `sudo dnf install gocryptfs` or `sudo apt install gocryptfs`)"
                    .to_string(),
            )
        })
    }
}

/// Run gocryptfs with `args`, feeding `password` through stdin
/// (`-passfile /dev/stdin`) so it never touches the disk or the argv list.
#[cfg(not(target_os = "windows"))]
fn run_gocryptfs(args: &[&std::ffi::OsStr], password: &str) -> Result<(), VaultFsError> {
    use std::io::Write;
    use std::process::Stdio;

    let bin = gocryptfs_binary()?;
    let mut child = super::process::silent_command(bin)
        .args(["-q", "-passfile", "/dev/stdin"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| VaultFsError::Other(format!("failed to start gocryptfs: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        // gocryptfs reads a single line; a trailing newline is stripped.
        stdin
            .write_all(password.as_bytes())
            .map_err(|e| VaultFsError::Other(format!("failed to send password: {e}")))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| VaultFsError::Other(format!("gocryptfs did not finish: {e}")))?;

    if output.status.success() {
        return Ok(());
    }
    if output.status.code() == Some(GOCRYPTFS_EXIT_WRONG_PASSWORD) {
        return Err(VaultFsError::WrongPassword);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(VaultFsError::Other(if stderr.is_empty() {
        format!("gocryptfs exited with {}", output.status)
    } else {
        stderr
    }))
}

/// Create a brand-new cipher directory protected by `password`.
/// `cipher_dir` must already exist and be empty.
pub fn init_cipher(cipher_dir: &Path, password: &str) -> Result<(), VaultFsError> {
    #[cfg(target_os = "windows")]
    {
        let _ = (cipher_dir, password);
        Err(VaultFsError::Unsupported(
            "Encrypted vaults are not supported on Windows".to_string(),
        ))
    }
    #[cfg(not(target_os = "windows"))]
    {
        run_gocryptfs(
            &[std::ffi::OsStr::new("-init"), cipher_dir.as_os_str()],
            password,
        )
    }
}

/// Mount `cipher_dir` decrypted at `mount_point` (must exist and be empty).
pub fn mount(cipher_dir: &Path, mount_point: &Path, password: &str) -> Result<(), VaultFsError> {
    #[cfg(target_os = "windows")]
    {
        let _ = (cipher_dir, mount_point, password);
        Err(VaultFsError::Unsupported(
            "Encrypted vaults are not supported on Windows".to_string(),
        ))
    }
    #[cfg(not(target_os = "windows"))]
    {
        run_gocryptfs(&[cipher_dir.as_os_str(), mount_point.as_os_str()], password)
    }
}

/// Unmount a previously mounted vault.
pub fn unmount(mount_point: &Path) -> Result<(), VaultFsError> {
    #[cfg(target_os = "windows")]
    {
        let _ = mount_point;
        Err(VaultFsError::Unsupported(
            "Encrypted vaults are not supported on Windows".to_string(),
        ))
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Linux: fusermount3 / fusermount work without root for user mounts.
        // macOS (macFUSE): plain umount.
        #[cfg(target_os = "linux")]
        let candidates: &[(&str, &[&str])] = &[
            ("fusermount3", &["-u"]),
            ("fusermount", &["-u"]),
            ("umount", &[]),
        ];
        #[cfg(not(target_os = "linux"))]
        let candidates: &[(&str, &[&str])] = &[("umount", &[]), ("diskutil", &["unmount"])];

        let mut last_err = String::from("no unmount tool found");
        for (tool, flags) in candidates {
            let Some(bin) = super::process::find_executable(tool) else {
                continue;
            };
            let output = super::process::silent_command(bin)
                .args(*flags)
                .arg(mount_point)
                .output()
                .map_err(|e| VaultFsError::Other(format!("failed to run {tool}: {e}")))?;
            if output.status.success() {
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            last_err = if stderr.is_empty() {
                format!("{tool} exited with {}", output.status)
            } else {
                stderr
            };
            // "Device or resource busy": another program holds files open.
            if last_err.contains("busy") {
                return Err(VaultFsError::Other(format!(
                    "the folder is in use by another program (close it and try again): {last_err}"
                )));
            }
        }
        Err(VaultFsError::Other(last_err))
    }
}

/// Whether `mount_point` currently has a filesystem mounted on it.
pub fn is_mounted(mount_point: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
            return false;
        };
        let target = mount_point.to_string_lossy();
        mounts.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .map(unescape_proc_mounts)
                .is_some_and(|mp| mp == target)
        })
    }
    #[cfg(target_os = "macos")]
    {
        let Some(bin) = super::process::find_executable("mount") else {
            return false;
        };
        let Ok(output) = super::process::silent_command(bin).output() else {
            return false;
        };
        let needle = format!(" on {} (", mount_point.to_string_lossy());
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.contains(&needle))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = mount_point;
        false
    }
}

/// `/proc/mounts` escapes spaces, tabs, newlines and backslashes as octal.
#[cfg(target_os = "linux")]
fn unescape_proc_mounts(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 4 <= bytes.len() {
            if let Ok(code) = u8::from_str_radix(&raw[i + 1..i + 4], 8) {
                out.push(code as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_is_human_readable() {
        assert_eq!(VaultFsError::WrongPassword.to_string(), "incorrect password");
        assert_eq!(
            VaultFsError::Other("boom".to_string()).to_string(),
            "boom"
        );
        assert_eq!(
            VaultFsError::Unsupported("nope".to_string()).to_string(),
            "nope"
        );
    }

    #[test]
    fn is_mounted_false_for_random_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!is_mounted(dir.path()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unescape_handles_octal_space() {
        assert_eq!(unescape_proc_mounts("/mnt/my\\040vault"), "/mnt/my vault");
        assert_eq!(unescape_proc_mounts("/plain/path"), "/plain/path");
        assert_eq!(unescape_proc_mounts("trailing\\04"), "trailing\\04");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_reports_unsupported() {
        assert!(matches!(
            gocryptfs_binary(),
            Err(VaultFsError::Unsupported(_))
        ));
    }
}
