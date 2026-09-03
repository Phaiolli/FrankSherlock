//! Encrypted vault folders.
//!
//! A vault is a scanned root whose contents live in a gocryptfs cipher
//! directory (`<parent>/.<name>.vault`). While the vault is *unlocked* the
//! decrypted view is mounted at the original folder path, so the scanner and
//! every other program see plain files. When it is *locked* the mount is
//! removed, the (now empty) mount point directory is deleted, and every DB
//! query hides the vault's files. Vaults are always locked at startup and on
//! exit, so a closed app never leaves a decrypted view behind.
//!
//! Thumbnails and face crops for a vault are written inside the mount
//! (`<root>/.frank_sherlock/`), so they are encrypted at rest as well.
//!
//! The DB index of a vault (file names, AI descriptions, OCR text, FTS) is
//! *sealed* on lock: it is written to `<root>/.frank_sherlock/index.json`
//! inside the vault, then blanked in SQLite with `secure_delete`, and put back
//! on unlock. If a vault is found unmounted but unsealed at startup (e.g. the
//! machine rebooted while it was open), the index is parked in
//! `<db dir>/vault_pending/<root id>.json` and moved into the vault at the
//! next unlock, so nothing is lost.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::config::canonical_root_path;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::{
    CreateVaultResult, VaultIndexSnapshot, VaultProbe, VaultProgress, VaultRoot, VaultSupport,
};
use crate::platform::vault_fs::{self, VaultFsError};

/// Hidden directory inside an unlocked vault that holds the app's caches.
pub const CACHE_DIR_NAME: &str = ".frank_sherlock";
const CIPHER_SUFFIX: &str = ".vault";
const STAGING_SUFFIX: &str = ".vault-migrating";
const INDEX_FILE_NAME: &str = "index.json";
const PENDING_DIR_NAME: &str = "vault_pending";
const MIN_PASSWORD_LEN: usize = 4;

/// Whether this machine can create and open vaults.
pub fn support() -> VaultSupport {
    match vault_fs::gocryptfs_binary() {
        Ok(_) => VaultSupport {
            supported: true,
            reason: None,
        },
        Err(e) => VaultSupport {
            supported: false,
            reason: Some(e.to_string()),
        },
    }
}

/// Directory inside an unlocked vault where thumbnails / face crops go.
pub fn cache_dir(root_path: &Path) -> PathBuf {
    root_path.join(CACHE_DIR_NAME)
}

/// Where the sealed index lives inside an unlocked vault.
fn index_file(root_path: &Path) -> PathBuf {
    cache_dir(root_path).join(INDEX_FILE_NAME)
}

/// Where an index waits when the vault could not be written to (see module docs).
fn pending_index_file(db_path: &Path, root_id: i64) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PENDING_DIR_NAME)
        .join(format!("{root_id}.json"))
}

fn write_snapshot(path: &Path, snapshot: &VaultIndexSnapshot) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec(snapshot)
        .map_err(|e| AppError::Config(format!("serialising vault index: {e}")))?;
    std::fs::write(path, json)?;
    Ok(())
}

fn read_snapshot(path: &Path) -> AppResult<Option<VaultIndexSnapshot>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| AppError::Config(format!("reading vault index {}: {e}", path.display())))
}

/// After sealing, make sure the on-disk backup does not still hold the plaintext index.
fn refresh_backup(db_path: &Path) {
    if let Err(e) = db::wal_checkpoint(db_path) {
        log::warn!("vault: WAL checkpoint failed: {e}");
    }
    let Some(dir) = db_path.parent() else { return };
    let backup = dir.join("index.sqlite.bak");
    if backup.exists() {
        if let Err(e) = db::backup_database(db_path, &backup) {
            log::warn!("vault: refreshing DB backup failed: {e}");
        }
    }
}

/// `<parent>/.<name>.vault` — the encrypted store for `folder`.
pub fn cipher_dir_for(folder: &Path) -> AppResult<PathBuf> {
    sibling_with_suffix(folder, CIPHER_SUFFIX)
}

/// `<parent>/.<name>.vault-migrating` — where plaintext waits during conversion.
fn staging_dir_for(folder: &Path) -> AppResult<PathBuf> {
    sibling_with_suffix(folder, STAGING_SUFFIX)
}

fn sibling_with_suffix(folder: &Path, suffix: &str) -> AppResult<PathBuf> {
    let parent = folder.parent().ok_or_else(|| {
        AppError::InvalidPath(format!(
            "cannot create a vault at the filesystem root: {}",
            folder.display()
        ))
    })?;
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .ok_or_else(|| AppError::InvalidPath(format!("invalid folder name: {}", folder.display())))?;
    Ok(parent.join(format!(".{name}{suffix}")))
}

fn vault_err(e: VaultFsError) -> AppError {
    AppError::Vault(e.to_string())
}

fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::Vault(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    if password.contains('\n') || password.contains('\r') {
        return Err(AppError::Vault(
            "Password cannot contain line breaks".to_string(),
        ));
    }
    Ok(())
}

/// State of an in-progress conversion, so a failure can be undone.
struct Migration {
    folder: PathBuf,
    staging: Option<PathBuf>,
    cipher_dir: PathBuf,
    mounted: bool,
}

impl Migration {
    fn rollback(&self) {
        if self.mounted {
            if let Err(e) = vault_fs::unmount(&self.folder) {
                log::error!("vault rollback: unmount {} failed: {e}", self.folder.display());
            }
        }
        if let Err(e) = std::fs::remove_dir_all(&self.cipher_dir) {
            log::warn!("vault rollback: remove {} failed: {e}", self.cipher_dir.display());
        }
        if let Some(staging) = &self.staging {
            // The mount point is empty once unmounted; put the originals back.
            let _ = std::fs::remove_dir(&self.folder);
            if let Err(e) = std::fs::rename(staging, &self.folder) {
                log::error!(
                    "vault rollback: could not restore {} from {}: {e}",
                    self.folder.display(),
                    staging.display()
                );
            }
        }
    }
}

/// Convert `folder_path` into an encrypted vault protected by `password`,
/// register it as a root and leave it unlocked (mounted).
///
/// Existing files are copied into the encrypted store and the plaintext
/// originals are deleted only after the copy is verified. Any failure before
/// that point restores the folder exactly as it was.
/// `report` is called as the conversion advances: encrypting a large folder
/// takes minutes, and without it the modal looks frozen. Pass `&|_| {}` when
/// progress is not needed.
pub fn create_vault(
    db_path: &Path,
    folder_path: &str,
    password: &str,
    report: &dyn Fn(VaultProgress),
) -> AppResult<CreateVaultResult> {
    validate_password(password)?;
    vault_fs::gocryptfs_binary().map_err(vault_err)?;

    let folder = canonical_root_path(folder_path)?;
    let folder_str = folder.display().to_string();
    if db::root_exists_for_path(db_path, &folder_str)? {
        return Err(AppError::Vault(
            "This folder is already in the library. Remove it first, then add it as a secret folder."
                .to_string(),
        ));
    }
    if vault_fs::is_mounted(&folder) {
        return Err(AppError::Vault(
            "This folder is a mount point and cannot be converted".to_string(),
        ));
    }
    let cipher_dir = cipher_dir_for(&folder)?;
    if cipher_dir.exists() {
        return Err(AppError::Vault(format!(
            "An encrypted store already exists at {}. Use \"Reopen secret folder\" to attach it, or remove it first.",
            cipher_dir.display()
        )));
    }
    let staging = staging_dir_for(&folder)?;
    if staging.exists() {
        return Err(AppError::Vault(format!(
            "A previous conversion left files at {}. Move them back manually first.",
            staging.display()
        )));
    }

    let has_content = dir_has_entries(&folder)?;
    // Sizing walks metadata only, but on a big tree it is still worth showing.
    report(VaultProgress {
        phase: "preparing".to_string(),
        processed_files: 0,
        total_files: 0,
        processed_bytes: 0,
        total_bytes: 0,
    });
    let (total_files, total_bytes) = if has_content {
        tree_summary(&folder)?
    } else {
        (0, 0)
    };
    let mut migration = Migration {
        folder: folder.clone(),
        staging: None,
        cipher_dir: cipher_dir.clone(),
        mounted: false,
    };

    // 1. Move plaintext aside (same parent → atomic rename) and recreate the mount point.
    if has_content {
        std::fs::rename(&folder, &staging)?;
        migration.staging = Some(staging.clone());
        if let Err(e) = std::fs::create_dir(&folder) {
            migration.rollback();
            return Err(e.into());
        }
    }

    // 2. Initialise the cipher directory.
    if let Err(e) = std::fs::create_dir_all(&cipher_dir) {
        migration.rollback();
        return Err(e.into());
    }
    if let Err(e) = vault_fs::init_cipher(&cipher_dir, password) {
        migration.rollback();
        return Err(vault_err(e));
    }

    // 3. Mount the decrypted view at the original path.
    if let Err(e) = vault_fs::mount(&cipher_dir, &folder, password) {
        migration.rollback();
        return Err(vault_err(e));
    }
    migration.mounted = true;

    // 4. Copy originals in and verify before deleting anything.
    let mut migrated_files = 0u64;
    if has_content {
        let mut copied_bytes = 0u64;
        let on_file = |files: u64, bytes: u64| {
            copied_bytes = bytes;
            report(VaultProgress {
                phase: "encrypting".to_string(),
                processed_files: files,
                total_files,
                processed_bytes: bytes,
                total_bytes,
            });
        };
        match copy_dir_recursive(&staging, &folder, on_file) {
            Ok(n) => migrated_files = n,
            Err(e) => {
                migration.rollback();
                return Err(AppError::Vault(format!("copying files into the vault failed: {e}")));
            }
        }
        report(VaultProgress {
            phase: "verifying".to_string(),
            processed_files: migrated_files,
            total_files,
            processed_bytes: copied_bytes,
            total_bytes,
        });
        let (src_files, src_bytes) = tree_summary(&staging)?;
        let (dst_files, dst_bytes) = tree_summary(&folder)?;
        if src_files != dst_files || src_bytes != dst_bytes {
            migration.rollback();
            return Err(AppError::Vault(format!(
                "verification failed after copy ({src_files} files / {src_bytes} bytes vs {dst_files} / {dst_bytes}); folder restored"
            )));
        }
    }

    // 5. Register the root (unlocked).
    let root_id = match db::insert_vault_root(db_path, &folder_str, &cipher_dir.display().to_string()) {
        Ok(id) => id,
        Err(e) => {
            migration.rollback();
            return Err(e);
        }
    };

    // 6. Remove the plaintext originals. The vault is already complete and
    //    registered, so a failure here is reported but not rolled back.
    if has_content {
        report(VaultProgress {
            phase: "finishing".to_string(),
            processed_files: migrated_files,
            total_files,
            processed_bytes: total_bytes,
            total_bytes,
        });
        if let Err(e) = std::fs::remove_dir_all(&staging) {
            return Err(AppError::Vault(format!(
                "Vault created, but the unencrypted copy at {} could not be removed: {e}. Delete it manually.",
                staging.display()
            )));
        }
    }

    log::info!(
        "Created vault for {} ({migrated_files} files migrated)",
        folder.display()
    );
    Ok(CreateVaultResult {
        root_id,
        root_path: folder_str,
        migrated_files,
    })
}

fn get_vault(db_path: &Path, root_id: i64) -> AppResult<VaultRoot> {
    db::get_vault_root(db_path, root_id)?
        .ok_or_else(|| AppError::Vault("This folder is not a secret folder".to_string()))
}

/// Mount the vault with `password` and mark it visible.
/// The password is always verified, even if a stale mount is still present.
pub fn unlock_vault(db_path: &Path, root_id: i64, password: &str) -> AppResult<VaultRoot> {
    let vault = get_vault(db_path, root_id)?;
    let mount_point = Path::new(&vault.root_path);
    let cipher_dir = Path::new(&vault.cipher_dir);

    if !cipher_dir.is_dir() {
        return Err(AppError::Vault(format!(
            "Encrypted data folder not found at {}",
            cipher_dir.display()
        )));
    }
    if vault_fs::is_mounted(mount_point) {
        vault_fs::unmount(mount_point).map_err(vault_err)?;
    }
    if !mount_point.exists() {
        std::fs::create_dir_all(mount_point)?;
    } else if dir_has_entries(mount_point)? {
        return Err(AppError::Vault(format!(
            "{} is not empty; move its contents away before unlocking",
            mount_point.display()
        )));
    }

    match vault_fs::mount(cipher_dir, mount_point, password) {
        Ok(()) => {}
        Err(VaultFsError::WrongPassword) => {
            let _ = std::fs::remove_dir(mount_point);
            return Err(AppError::Vault("Incorrect password".to_string()));
        }
        Err(e) => {
            let _ = std::fs::remove_dir(mount_point);
            return Err(vault_err(e));
        }
    }
    if let Err(e) = unseal_index(db_path, &vault) {
        // The vault itself is open; a missing index only means a rescan.
        log::warn!("vault {}: could not restore index: {e}", vault.root_path);
    }
    db::set_vault_scrubbed(db_path, root_id, false)?;
    db::set_vault_locked(db_path, root_id, false)?;
    Ok(VaultRoot {
        locked: false,
        scrubbed: false,
        ..vault
    })
}

/// Put the sealed index back into the DB. Prefers a parked ("pending")
/// snapshot, which is always newer than the one inside the vault.
fn unseal_index(db_path: &Path, vault: &VaultRoot) -> AppResult<()> {
    let mount_point = Path::new(&vault.root_path);
    let pending = pending_index_file(db_path, vault.id);
    let in_vault = index_file(mount_point);
    let (snapshot, from_pending) = match read_snapshot(&pending)? {
        Some(s) => (Some(s), true),
        None => (read_snapshot(&in_vault)?, false),
    };
    let Some(snapshot) = snapshot else {
        if vault.scrubbed {
            log::warn!(
                "vault {}: no sealed index found; files will be re-indexed on the next scan",
                vault.root_path
            );
        }
        return Ok(());
    };
    if vault.scrubbed {
        let n = db::restore_vault_index(db_path, vault.id, &snapshot)?;
        log::info!("vault {}: restored index for {n} files", vault.root_path);
    }
    if from_pending {
        write_snapshot(&in_vault, &snapshot)?;
        std::fs::remove_file(&pending)?;
    }
    Ok(())
}

/// Seal the index (see module docs), unmount and blank the DB rows.
/// Returns Err without changing anything visible if the unmount fails.
fn lock_vault_inner(db_path: &Path, vault: &VaultRoot) -> AppResult<()> {
    let mount_point = Path::new(&vault.root_path);
    let mounted = vault_fs::is_mounted(mount_point);

    if !vault.scrubbed {
        let snapshot = db::snapshot_vault_index(db_path, vault.id)?;
        if mounted {
            write_snapshot(&index_file(mount_point), &snapshot)?;
        } else {
            write_snapshot(&pending_index_file(db_path, vault.id), &snapshot)?;
        }
    }
    if mounted {
        vault_fs::unmount(mount_point).map_err(vault_err)?;
    }
    if !vault.scrubbed {
        db::scrub_vault_index(db_path, vault.id)?;
    }
    db::set_vault_locked(db_path, vault.id, true)?;
    let _ = std::fs::remove_dir(mount_point);
    Ok(())
}

/// Seal the index, unmount the vault, hide its files and remove the empty mount point.
pub fn lock_vault(db_path: &Path, root_id: i64) -> AppResult<VaultRoot> {
    let vault = get_vault(db_path, root_id)?;
    lock_vault_inner(db_path, &vault)?;
    refresh_backup(db_path);
    Ok(VaultRoot {
        locked: true,
        scrubbed: true,
        ..vault
    })
}

/// Lock every vault (startup and shutdown). Failures are logged, not fatal:
/// the DB flag is always set so the app never shows a vault it cannot verify.
pub fn lock_all_vaults(db_path: &Path) -> AppResult<()> {
    let vaults = db::list_vault_roots(db_path)?;
    for vault in &vaults {
        match lock_vault_inner(db_path, vault) {
            Ok(()) => log::info!("Locked vault {} (root {})", vault.root_path, vault.id),
            Err(e) => log::warn!("Could not lock vault {}: {e}", vault.root_path),
        }
    }
    db::set_all_vaults_locked(db_path)?;
    if !vaults.is_empty() {
        refresh_backup(db_path);
    }
    Ok(())
}

/// Work out (mount point, cipher dir) for a path the user picked, when an
/// encrypted store already exists: either the `.name.vault` directory itself
/// or a folder whose sibling `.name.vault` exists.
fn resolve_attach_target(selected: &Path) -> AppResult<(PathBuf, PathBuf)> {
    let selected = dunce::canonicalize(selected)?;
    let name = selected
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if let Some(stem) = name.strip_prefix('.').and_then(|n| n.strip_suffix(CIPHER_SUFFIX)) {
        if !stem.is_empty() && selected.join("gocryptfs.conf").is_file() {
            let parent = selected.parent().ok_or_else(|| {
                AppError::InvalidPath(format!("no parent for {}", selected.display()))
            })?;
            return Ok((parent.join(stem), selected));
        }
    }
    let cipher_dir = cipher_dir_for(&selected)?;
    if cipher_dir.join("gocryptfs.conf").is_file() {
        return Ok((selected, cipher_dir));
    }
    Err(AppError::Vault(format!(
        "No encrypted store found for {}",
        selected.display()
    )))
}

/// Tell the UI whether "Add folder" should offer to reopen an existing vault.
pub fn probe(selected: &str) -> VaultProbe {
    match resolve_attach_target(Path::new(selected)) {
        Ok((mount_point, cipher_dir)) => VaultProbe {
            attachable: true,
            mount_point: Some(mount_point.display().to_string()),
            cipher_dir: Some(cipher_dir.display().to_string()),
        },
        Err(_) => VaultProbe {
            attachable: false,
            mount_point: None,
            cipher_dir: None,
        },
    }
}

/// Register an existing encrypted store as a vault root and unlock it.
/// If the vault carries a sealed index from a previous registration, its
/// file rows are imported so the AI classification does not have to be redone.
pub fn attach_vault(db_path: &Path, selected: &str, password: &str) -> AppResult<CreateVaultResult> {
    if password.is_empty() || password.contains('\n') || password.contains('\r') {
        return Err(AppError::Vault("Enter the vault password".to_string()));
    }
    vault_fs::gocryptfs_binary().map_err(vault_err)?;
    let (mount_point, cipher_dir) = resolve_attach_target(Path::new(selected))?;
    let mount_str = mount_point.display().to_string();
    if db::root_exists_for_path(db_path, &mount_str)? {
        return Err(AppError::Vault(
            "This folder is already in the library".to_string(),
        ));
    }
    // Always verify the password, even if someone mounted it by hand.
    if vault_fs::is_mounted(&mount_point) {
        vault_fs::unmount(&mount_point).map_err(vault_err)?;
    }
    if mount_point.exists() && dir_has_entries(&mount_point)? {
        return Err(AppError::Vault(format!(
            "{} is not empty; move its contents away before reopening the vault",
            mount_point.display()
        )));
    }
    std::fs::create_dir_all(&mount_point)?;
    match vault_fs::mount(&cipher_dir, &mount_point, password) {
        Ok(()) => {}
        Err(VaultFsError::WrongPassword) => {
            let _ = std::fs::remove_dir(&mount_point);
            return Err(AppError::Vault("Incorrect password".to_string()));
        }
        Err(e) => {
            let _ = std::fs::remove_dir(&mount_point);
            return Err(vault_err(e));
        }
    }

    let root_id = db::insert_vault_root(db_path, &mount_str, &cipher_dir.display().to_string())?;
    let mut migrated_files = 0u64;
    let index = index_file(&mount_point);
    match read_snapshot(&index) {
        Ok(Some(snapshot)) => {
            let thumbs = cache_dir(&mount_point).join("thumbnails");
            match db::import_vault_snapshot(db_path, root_id, &mount_point, &thumbs, &snapshot) {
                Ok(n) => {
                    migrated_files = n;
                    log::info!("vault {mount_str}: imported index for {n} files");
                }
                Err(e) => log::warn!("vault {mount_str}: importing sealed index failed: {e}"),
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!("vault {mount_str}: unreadable sealed index ignored: {e}"),
    }
    // The old snapshot refers to the previous root id; a fresh one is written on lock.
    let _ = std::fs::remove_file(&index);

    Ok(CreateVaultResult {
        root_id,
        root_path: mount_str,
        migrated_files,
    })
}

/// Reject scans of a vault that is not currently unlocked.
pub fn ensure_scannable(db_path: &Path, root_path: &str) -> AppResult<()> {
    // A locked vault's mount point does not exist, so canonicalisation may
    // fail; look the raw path up first and fall back to the canonical form.
    let mut found = db::find_vault_root_by_path(db_path, root_path)?;
    if found.is_none() {
        if let Ok(canonical) = canonical_root_path(root_path) {
            found = db::find_vault_root_by_path(db_path, &canonical.display().to_string())?;
        }
    }
    if let Some(vault) = found {
        if vault.locked || !vault_fs::is_mounted(Path::new(&vault.root_path)) {
            return Err(AppError::Vault(format!(
                "Unlock the secret folder \"{}\" before scanning it",
                vault.root_name
            )));
        }
    }
    Ok(())
}

fn dir_has_entries(dir: &Path) -> AppResult<bool> {
    Ok(std::fs::read_dir(dir)?.next().is_some())
}

/// Recursively copy `src` into `dst` (which must exist), preserving mtimes.
/// Returns the number of regular files copied.
/// `on_file(files_copied, bytes_copied)` is called after every regular file so
/// the caller can report progress.
fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    mut on_file: impl FnMut(u64, u64),
) -> AppResult<u64> {
    let mut count = 0u64;
    let mut bytes = 0u64;
    for entry in WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| AppError::InvalidPath(e.to_string()))?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dst.join(rel);
        let file_type = entry.file_type();
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if file_type.is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            bytes += std::fs::copy(entry.path(), &target)?;
            if let Some(modified) = entry.metadata().ok().and_then(|m| m.modified().ok()) {
                if let Ok(f) = std::fs::File::options().write(true).open(&target) {
                    let _ = f.set_modified(modified);
                }
            }
            count += 1;
            on_file(count, bytes);
        } else {
            // Symlinks are not carried into the vault: they would point outside it.
            log::warn!("vault migration: skipping non-regular file {}", entry.path().display());
        }
    }
    Ok(count)
}

/// (regular file count, total bytes) under `dir`.
fn tree_summary(dir: &Path) -> AppResult<(u64, u64)> {
    let mut files = 0u64;
    let mut bytes = 0u64;
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
        if entry.file_type().is_file() {
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok((files, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &[u8]) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn cipher_and_staging_dirs_are_hidden_siblings() {
        let folder = Path::new("/data/photos/Secret");
        assert_eq!(
            cipher_dir_for(folder).unwrap(),
            PathBuf::from("/data/photos/.Secret.vault")
        );
        assert_eq!(
            staging_dir_for(folder).unwrap(),
            PathBuf::from("/data/photos/.Secret.vault-migrating")
        );
        assert!(cipher_dir_for(Path::new("/")).is_err());
    }

    #[test]
    fn cache_dir_is_inside_root() {
        assert_eq!(
            cache_dir(Path::new("/x/y")),
            PathBuf::from("/x/y/.frank_sherlock")
        );
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("abc").is_err());
        assert!(validate_password("abcd").is_ok());
        assert!(validate_password("with\nnewline").is_err());
        assert!(validate_password("çãõ!").is_ok());
    }

    #[test]
    fn copy_dir_recursive_copies_tree_and_counts_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("a.jpg"), b"aaa");
        write(&src.join("sub/deep/b.png"), b"bbbbb");
        std::fs::create_dir_all(src.join("empty")).unwrap();
        std::fs::create_dir_all(&dst).unwrap();

        let n = copy_dir_recursive(&src, &dst, |_, _| {}).unwrap();
        assert_eq!(n, 2);
        assert_eq!(std::fs::read(dst.join("a.jpg")).unwrap(), b"aaa");
        assert_eq!(std::fs::read(dst.join("sub/deep/b.png")).unwrap(), b"bbbbb");
        assert!(dst.join("empty").is_dir());
        assert_eq!(tree_summary(&src).unwrap(), tree_summary(&dst).unwrap());
        assert_eq!(tree_summary(&dst).unwrap(), (2, 8));
    }

    #[test]
    fn copy_dir_recursive_reports_running_file_and_byte_counts() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        write(&src.join("a.jpg"), b"aaa");
        write(&src.join("sub/b.png"), b"bbbbb");
        std::fs::create_dir_all(&dst).unwrap();

        let mut seen: Vec<(u64, u64)> = Vec::new();
        let n = copy_dir_recursive(&src, &dst, |files, bytes| seen.push((files, bytes))).unwrap();
        assert_eq!(n, 2);
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].0, 1);
        assert_eq!(seen[1], (2, 8));
        // Counts only ever grow.
        assert!(seen.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
    }

    #[test]
    fn dir_has_entries_detects_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!dir_has_entries(tmp.path()).unwrap());
        write(&tmp.path().join("f"), b"x");
        assert!(dir_has_entries(tmp.path()).unwrap());
    }

    #[test]
    fn create_vault_rejects_bad_password_before_touching_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("index.sqlite");
        db::init_database(&db_path).unwrap();
        let folder = tmp.path().join("Secret");
        write(&folder.join("a.jpg"), b"aaa");

        let err = create_vault(&db_path, &folder.display().to_string(), "ab", &|_| {}).unwrap_err();
        assert!(err.to_string().contains("at least"));
        assert!(folder.join("a.jpg").exists());
        assert!(!cipher_dir_for(&folder).unwrap().exists());
    }

    /// Full round trip against a real gocryptfs. Skipped when the binary is
    /// missing (CI runners) so the suite stays green everywhere.
    #[test]
    fn vault_round_trip_with_gocryptfs() {
        if vault_fs::gocryptfs_binary().is_err() {
            eprintln!("gocryptfs not installed; skipping round-trip test");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("index.sqlite");
        db::init_database(&db_path).unwrap();
        let folder = tmp.path().join("Secret");
        write(&folder.join("a.jpg"), b"aaa");
        write(&folder.join("sub/b.png"), b"bbbbb");
        let folder_str = folder.display().to_string();

        let seen = std::sync::Mutex::new(Vec::new());
        let created = match create_vault(&db_path, &folder_str, "s3cret!", &|p| {
            seen.lock().unwrap().push(p);
        }) {
            Ok(c) => c,
            Err(e) if e.to_string().contains("fuse") || e.to_string().contains("FUSE") => {
                eprintln!("FUSE unavailable; skipping round-trip test: {e}");
                return;
            }
            Err(e) => panic!("create_vault failed: {e}"),
        };
        assert_eq!(created.migrated_files, 2);
        let seen = seen.into_inner().unwrap();
        let phases: Vec<&str> = seen.iter().map(|p| p.phase.as_str()).collect();
        assert_eq!(phases.first(), Some(&"preparing"));
        assert!(phases.contains(&"encrypting"));
        assert!(phases.contains(&"verifying"));
        assert_eq!(phases.last(), Some(&"finishing"));
        let last_copy = seen.iter().rfind(|p| p.phase == "encrypting").unwrap();
        assert_eq!(last_copy.processed_files, 2);
        assert_eq!(last_copy.total_files, 2);
        assert_eq!(last_copy.processed_bytes, last_copy.total_bytes);
        let cipher = cipher_dir_for(&folder).unwrap();
        assert!(cipher.join("gocryptfs.conf").exists());
        assert!(!staging_dir_for(&folder).unwrap().exists());
        assert!(vault_fs::is_mounted(&folder));
        assert_eq!(std::fs::read(folder.join("sub/b.png")).unwrap(), b"bbbbb");

        let roots = db::list_roots(&db_path).unwrap();
        assert_eq!(roots.len(), 1);
        assert!(roots[0].is_vault);
        assert!(!roots[0].vault_locked);
        assert!(ensure_scannable(&db_path, &folder_str).is_ok());

        // Lock: unmounted, mount point gone, DB flag set.
        let locked = lock_vault(&db_path, created.root_id).unwrap();
        assert!(locked.locked);
        assert!(!vault_fs::is_mounted(&folder));
        assert!(!folder.exists());
        assert!(db::list_roots(&db_path).unwrap()[0].vault_locked);
        assert!(ensure_scannable(&db_path, &folder_str).is_err());

        // Wrong password is rejected and leaves the vault locked.
        let err = unlock_vault(&db_path, created.root_id, "nope").unwrap_err();
        assert!(err.to_string().contains("Incorrect password"), "{err}");
        assert!(!vault_fs::is_mounted(&folder));

        // Right password restores the decrypted view.
        unlock_vault(&db_path, created.root_id, "s3cret!").unwrap();
        assert!(vault_fs::is_mounted(&folder));
        assert_eq!(std::fs::read(folder.join("a.jpg")).unwrap(), b"aaa");
        assert!(!db::list_roots(&db_path).unwrap()[0].vault_locked);

        // lock_all is what startup/exit call.
        lock_all_vaults(&db_path).unwrap();
        assert!(!vault_fs::is_mounted(&folder));
        assert!(db::list_roots(&db_path).unwrap()[0].vault_locked);
    }

    /// Index sealing: while locked nothing about the files is in the DB;
    /// unlocking restores it; reopening a removed vault re-imports it.
    #[test]
    fn vault_seals_index_and_reattaches_with_gocryptfs() {
        if vault_fs::gocryptfs_binary().is_err() {
            eprintln!("gocryptfs not installed; skipping seal test");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("index.sqlite");
        db::init_database(&db_path).unwrap();
        let folder = tmp.path().join("Secret");
        std::fs::create_dir_all(&folder).unwrap();
        let folder_str = folder.display().to_string();
        let created = match create_vault(&db_path, &folder_str, "s3cret!", &|_| {}) {
            Ok(c) => c,
            Err(e) if e.to_string().to_lowercase().contains("fuse") => {
                eprintln!("FUSE unavailable; skipping: {e}");
                return;
            }
            Err(e) => panic!("create_vault failed: {e}"),
        };
        // Pretend a scan classified a file and wrote its thumbnail inside the vault.
        let thumbs = cache_dir(&folder).join("thumbnails");
        write(&thumbs.join("cat.jpg"), b"thumb");
        let mut rec = crate::models::FileRecordUpsert {
            root_id: created.root_id,
            rel_path: "cat.jpg".into(),
            abs_path: folder.join("cat.jpg").display().to_string(),
            filename: "cat.jpg".into(),
            media_type: "photo".into(),
            description: "a very secret cat".into(),
            extracted_text: String::new(),
            canonical_mentions: String::new(),
            confidence: 0.9,
            lang_hint: "en".into(),
            mtime_ns: 1,
            size_bytes: 5,
            fingerprint: "fp-cat".into(),
            scan_marker: 1,
            location_text: String::new(),
            dhash: None,
            duration_secs: None,
            video_width: None,
            video_height: None,
            video_codec: None,
            audio_codec: None,
        };
        let file_id = db::upsert_file_record(&db_path, &rec).unwrap();
        db::update_file_thumb_path_by_id(&db_path, file_id, &thumbs.join("cat.jpg").display().to_string()).unwrap();
        let search = crate::models::SearchRequest {
            query: "secret cat".into(),
            ..Default::default()
        };
        assert_eq!(db::search_images(&db_path, &search).unwrap().total, 1);

        // Lock: index sealed in the vault, DB blanked (even ignoring the visibility filter).
        lock_vault(&db_path, created.root_id).unwrap();
        assert!(!folder.exists());
        let raw = db::open_conn_for_test(&db_path);
        let (name, desc): (String, String) = raw
            .query_row(
                "SELECT filename, description FROM files WHERE id = ?1",
                rusqlite::params![file_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "locked");
        assert!(desc.is_empty());
        drop(raw);
        assert!(db::get_vault_root(&db_path, created.root_id).unwrap().unwrap().scrubbed);
        assert!(!pending_index_file(&db_path, created.root_id).exists());

        // Unlock: everything is back.
        unlock_vault(&db_path, created.root_id, "s3cret!").unwrap();
        assert!(index_file(&folder).is_file());
        assert_eq!(db::search_images(&db_path, &search).unwrap().total, 1);
        assert!(!db::get_vault_root(&db_path, created.root_id).unwrap().unwrap().scrubbed);

        // Reboot scenario: vault vanished (unmounted) while DB still says unlocked.
        vault_fs::unmount(&folder).unwrap();
        lock_all_vaults(&db_path).unwrap();
        assert!(pending_index_file(&db_path, created.root_id).is_file());
        assert_eq!(db::search_images(&db_path, &search).unwrap().total, 0);
        unlock_vault(&db_path, created.root_id, "s3cret!").unwrap();
        assert!(!pending_index_file(&db_path, created.root_id).exists());
        assert_eq!(db::search_images(&db_path, &search).unwrap().total, 1);
        rec.description.clear();

        // Remove from library, then reopen: probe finds it, rows are re-imported.
        lock_vault(&db_path, created.root_id).unwrap();
        db::purge_root(&db_path, created.root_id).unwrap();
        assert!(db::list_roots(&db_path).unwrap().is_empty());
        let cipher = cipher_dir_for(&folder).unwrap();
        let probe_cipher = probe(&cipher.display().to_string());
        assert!(probe_cipher.attachable);
        assert_eq!(probe_cipher.mount_point.as_deref(), Some(folder_str.as_str()));
        assert!(!probe(&tmp.path().display().to_string()).attachable);

        assert!(attach_vault(&db_path, &cipher.display().to_string(), "wrong").is_err());
        let attached = attach_vault(&db_path, &cipher.display().to_string(), "s3cret!").unwrap();
        assert_eq!(attached.root_path, folder_str);
        assert_eq!(attached.migrated_files, 1);
        assert!(vault_fs::is_mounted(&folder));
        let found = db::search_images(&db_path, &search).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.items[0].root_id, attached.root_id);
        assert_eq!(found.items[0].thumbnail_path.as_deref(), Some(thumbs.join("cat.jpg").display().to_string().as_str()));
        assert!(attach_vault(&db_path, &folder_str, "s3cret!").is_err(), "already in library");

        lock_all_vaults(&db_path).unwrap();
        assert!(!vault_fs::is_mounted(&folder));
    }
}
