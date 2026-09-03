//! Mounted volume discovery for the "Add folder" flow.
//!
//! Native folder dialogs only bookmark a handful of well-known places, so a
//! second disk mounted at `/mnt/...` (Linux), `/Volumes/...` (macOS) or another
//! drive letter (Windows) is often invisible in their sidebar. We enumerate the
//! real mounted filesystems ourselves and hand the chosen mount point to the
//! dialog as its starting directory.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// A place the user can start browsing from in the Add Folder picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    /// Display label ("Home", "arquivos", "C:").
    pub name: String,
    /// Absolute path to browse from.
    pub path: String,
    /// "home" | "root" | "drive" — the frontend picks an icon from this.
    pub kind: String,
}

impl VolumeInfo {
    fn new(name: impl Into<String>, path: impl Into<String>, kind: &str) -> Self {
        Self { name: name.into(), path: path.into(), kind: kind.to_string() }
    }
}

/// The user's home directory, if it can be resolved.
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let var = std::env::var_os("HOME");
    var.map(PathBuf::from).filter(|p| p.is_dir())
}

/// Label a mount point by its last path component, falling back to the path.
/// Windows names its volumes by drive letter and never needs this.
#[cfg(not(windows))]
fn label_for(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// All places worth offering as a browse start: home, extra disks, then root.
pub fn list_volumes() -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    let home = home_dir();
    if let Some(ref h) = home {
        volumes.push(VolumeInfo::new("Home", h.to_string_lossy(), "home"));
    }

    let mut drives = mounted_drives();
    drives.sort_by_key(|(name, _)| name.to_lowercase());
    for (name, path) in drives {
        // Home is already listed; so is every folder containing it (e.g. a
        // separate /home mount), which would read as a duplicate of Home.
        if home.as_ref().is_some_and(|h| h.starts_with(&path)) {
            continue;
        }
        if volumes.iter().any(|v| v.path == path) {
            continue;
        }
        volumes.push(VolumeInfo::new(name, path, "drive"));
    }

    let root = root_path();
    if !volumes.iter().any(|v| v.path == root) {
        volumes.push(VolumeInfo::new(root_label(), root, "root"));
    }
    volumes
}

/// Expand `~`, canonicalize, and confirm the path is an existing directory.
pub fn resolve_folder_path(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a folder path".to_string());
    }
    let expanded = expand_tilde(trimmed);
    let canonical = dunce_canonicalize(&expanded)
        .map_err(|_| format!("Path not found: {}", expanded.display()))?;
    if !canonical.is_dir() {
        return Err(format!("Not a folder: {}", canonical.display()));
    }
    Ok(canonical.to_string_lossy().into_owned())
}

/// Replace a leading `~` with the home directory.
fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(input));
    }
    if let Some(rest) = input.strip_prefix("~/").or_else(|| input.strip_prefix("~\\")) {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(input)
}

/// `canonicalize` without the Windows `\\?\` verbatim prefix, which no dialog
/// or shell understands.
fn dunce_canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let text = canonical.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            if !stripped.starts_with("UNC\\") {
                return Ok(PathBuf::from(stripped));
            }
        }
    }
    Ok(canonical)
}

fn root_path() -> String {
    #[cfg(windows)]
    {
        home_dir()
            .and_then(|h| h.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()))
            .map(|d| format!("{d}\\"))
            .unwrap_or_else(|| "C:\\".to_string())
    }
    #[cfg(not(windows))]
    {
        "/".to_string()
    }
}

fn root_label() -> String {
    #[cfg(windows)]
    {
        "System drive".to_string()
    }
    #[cfg(not(windows))]
    {
        "Filesystem root".to_string()
    }
}

// ── Linux ────────────────────────────────────────────────────

/// Pseudo/virtual filesystems that never hold user photos.
#[cfg(target_os = "linux")]
const PSEUDO_FS: &[&str] = &[
    "autofs", "binfmt_misc", "bpf", "cgroup", "cgroup2", "configfs", "debugfs", "devpts",
    "devtmpfs", "efivarfs", "fuse.gvfsd-fuse", "fuse.portal", "fusectl", "hugetlbfs", "mqueue",
    "nsfs", "overlay", "proc", "pstore", "ramfs", "rpc_pipefs", "securityfs", "selinuxfs",
    "squashfs", "sysfs", "tmpfs", "tracefs",
];

/// Mount points under these prefixes are system plumbing, not user storage.
/// `/run/media` is the exception: that is where removable disks land.
#[cfg(target_os = "linux")]
const HIDDEN_PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/boot", "/tmp", "/var/lib", "/snap"];

/// Decode the octal escapes (`\040` for space) used in `/proc/mounts` fields.
#[cfg(target_os = "linux")]
fn unescape_mount_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = String::with_capacity(field.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            if let Ok(code) = u8::from_str_radix(&field[i + 1..i + 4], 8) {
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

/// Extract user-visible `(label, mount point)` pairs from `/proc/mounts` text.
#[cfg(target_os = "linux")]
fn parse_mounts(contents: &str) -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in contents.lines() {
        let mut fields = line.split_whitespace();
        let (Some(_device), Some(target), Some(fstype)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if PSEUDO_FS.contains(&fstype) {
            continue;
        }
        let target = unescape_mount_field(target);
        if target == "/" {
            continue;
        }
        // Removable disks land under /run/media; the rest of /run is plumbing.
        let removable = target.starts_with("/run/media/");
        let hidden = HIDDEN_PREFIXES
            .iter()
            .any(|p| target == *p || target.starts_with(&format!("{p}/")))
            || target.starts_with("/run/");
        if hidden && !removable {
            continue;
        }
        if !seen.insert(target.clone()) {
            continue;
        }
        out.push((label_for(Path::new(&target)), target));
    }
    out
}

#[cfg(target_os = "linux")]
fn mounted_drives() -> Vec<(String, String)> {
    let contents = std::fs::read_to_string("/proc/mounts")
        .or_else(|_| std::fs::read_to_string("/etc/mtab"))
        .unwrap_or_default();
    parse_mounts(&contents)
        .into_iter()
        .filter(|(_, path)| Path::new(path).is_dir())
        .collect()
}

// ── macOS ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn mounted_drives() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir("/Volumes") else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `/Volumes/Macintosh HD` is a symlink back to `/`; skip those.
        if entry.file_type().is_ok_and(|t| t.is_symlink()) || !path.is_dir() {
            continue;
        }
        out.push((label_for(&path), path.to_string_lossy().into_owned()));
    }
    out
}

// ── Windows ──────────────────────────────────────────────────

#[cfg(windows)]
fn mounted_drives() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for letter in b'A'..=b'Z' {
        let root = format!("{}:\\", letter as char);
        if Path::new(&root).is_dir() {
            out.push((format!("{}:", letter as char), root));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_volumes_includes_a_root_and_no_duplicates() {
        let volumes = list_volumes();
        assert!(!volumes.is_empty());
        assert!(volumes.iter().any(|v| v.kind == "root" || v.kind == "home"));
        let mut paths: Vec<&str> = volumes.iter().map(|v| v.path.as_str()).collect();
        let count = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), count, "volume list must not repeat a path");
    }

    #[test]
    fn resolve_folder_path_accepts_an_existing_dir() {
        let dir = std::env::temp_dir();
        let resolved = resolve_folder_path(dir.to_str().expect("temp dir is utf-8"))
            .expect("temp dir resolves");
        assert!(Path::new(&resolved).is_dir());
    }

    #[test]
    fn resolve_folder_path_rejects_blank_and_missing() {
        assert!(resolve_folder_path("   ").is_err());
        let missing = std::env::temp_dir().join("frank_sherlock_no_such_dir_zzz");
        assert!(resolve_folder_path(&missing.to_string_lossy()).is_err());
    }

    #[test]
    fn resolve_folder_path_rejects_a_file() {
        let file = std::env::temp_dir().join("frank_sherlock_volumes_test.txt");
        std::fs::write(&file, b"x").expect("write temp file");
        let result = resolve_folder_path(&file.to_string_lossy());
        let _ = std::fs::remove_file(&file);
        assert!(result.is_err());
    }

    #[test]
    fn expand_tilde_expands_only_a_leading_tilde() {
        if let Some(home) = home_dir() {
            assert_eq!(expand_tilde("~"), home);
            assert_eq!(expand_tilde("~/Pictures"), home.join("Pictures"));
        }
        assert_eq!(expand_tilde("/mnt/a~b"), PathBuf::from("/mnt/a~b"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_mounts_keeps_real_disks_and_drops_pseudo_filesystems() {
        let sample = "\
proc /proc proc rw,nosuid 0 0
sysfs /sys sysfs rw 0 0
/dev/nvme0n1p3 / btrfs rw,subvol=/root 0 0
/dev/nvme0n1p3 /home btrfs rw,subvol=/home 0 0
/dev/sda1 /mnt/arquivos btrfs rw 0 0
/dev/nvme1n1p1 /mnt/trabalho ext4 rw 0 0
/dev/nvme0n1p2 /boot ext4 rw 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/sdb1 /run/media/user/My\\040Backup vfat rw 0 0
";
        let mounts = parse_mounts(sample);
        let paths: Vec<&str> = mounts.iter().map(|(_, p)| p.as_str()).collect();
        assert_eq!(
            paths,
            vec!["/home", "/mnt/arquivos", "/mnt/trabalho", "/run/media/user/My Backup"]
        );
        assert_eq!(mounts[3].0, "My Backup");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_mounts_deduplicates_repeated_mount_points() {
        let sample = "\
/dev/sda1 /mnt/arquivos btrfs rw 0 0
/dev/sda1 /mnt/arquivos btrfs rw,subvol=/snapshots 0 0
";
        assert_eq!(parse_mounts(sample).len(), 1);
    }
}
