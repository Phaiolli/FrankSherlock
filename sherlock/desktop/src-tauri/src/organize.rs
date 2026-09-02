//! Organize a scanned folder *on disk* the way the app groups it internally.
//!
//! Currently one scheme: **by people**. Every file with at least one
//! recognised face is moved to `<root>/Pessoas/<Person>/<filename>`. A file
//! showing several people is moved into the first person's folder and
//! *copied* into each other person's folder (the user chose copies over
//! links). Files without recognised people are left untouched.
//!
//! The DB is updated in the same pass (moved rows are re-pathed, copies get a
//! cloned row including their face detections), so no rescan is needed and
//! nothing is re-classified. Directories left empty by the moves are removed.
//!
//! This is, together with vault creation, one of the two deliberate
//! exceptions to the "never write to scanned directories" rule.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::models::OrganizeResult;
use crate::platform::paths::normalize_rel_path;

/// Top-level folder created inside the root.
pub const PEOPLE_DIR: &str = "Pessoas";
const UNNAMED_PERSON: &str = "Sem nome";

/// Make a person name safe to use as a directory name on every OS.
pub fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        UNNAMED_PERSON.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Thumbnail location the scanner would pick for `rel_path`.
fn thumb_path_for(thumbnails_dir: &Path, rel_path: &str) -> PathBuf {
    let stem = normalize_rel_path(&Path::new(rel_path).with_extension("jpg").to_string_lossy());
    thumbnails_dir.join(stem)
}

/// `Pessoas/<person>/<filename>`, with `_2`, `_3`… appended while a *different*
/// file already occupies the name.
fn unique_target(root: &Path, person_dir: &str, filename: &str, source_abs: &Path) -> (String, PathBuf) {
    let base = Path::new(filename);
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| filename.to_string());
    let ext = base
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            filename.to_string()
        } else {
            format!("{stem}_{n}{ext}")
        };
        let rel = format!("{PEOPLE_DIR}/{person_dir}/{candidate}");
        let abs = root.join(PEOPLE_DIR).join(person_dir).join(&candidate);
        if !abs.exists() || abs == source_abs {
            return (rel, abs);
        }
        n += 1;
    }
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        // Different filesystem (e.g. a bind mount inside the root): copy then delete.
        Err(e) if e.raw_os_error() == Some(18) => {
            copy_file(from, to)?;
            std::fs::remove_file(from)
        }
        Err(e) => Err(e),
    }
}

fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to)?;
    if let Ok(modified) = std::fs::metadata(from).and_then(|m| m.modified()) {
        if let Ok(f) = std::fs::File::options().write(true).open(to) {
            let _ = f.set_modified(modified);
        }
    }
    Ok(())
}

/// Remove `dir` and its now-empty ancestors up to (excluding) `root`.
fn prune_empty_dirs(root: &Path, mut dir: PathBuf) {
    while dir != root && dir.starts_with(root) {
        let is_empty = match std::fs::read_dir(&dir) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => false,
        };
        if !is_empty || std::fs::remove_dir(&dir).is_err() {
            return;
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return,
        }
    }
}

/// Is `rel_path` already inside `Pessoas/<person_dir>/`?
fn lives_under(rel_path: &str, person_dir: &str) -> bool {
    rel_path.starts_with(&format!("{PEOPLE_DIR}/{person_dir}/"))
}

pub fn organize_root_by_people(
    db_path: &Path,
    root_id: i64,
    thumbnails_dir: &Path,
) -> AppResult<OrganizeResult> {
    let root_info = db::get_root(db_path, root_id)?
        .ok_or_else(|| AppError::Config(format!("root {root_id} not found")))?;
    let root = PathBuf::from(&root_info.root_path);
    if !root.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "folder is not accessible: {}",
            root.display()
        )));
    }

    let files = db::list_files_with_persons(db_path, root_id)?;
    let mut result = OrganizeResult::default();
    let mut people: BTreeMap<String, u64> = BTreeMap::new();
    let mut touched_dirs: Vec<PathBuf> = Vec::new();

    for file in files {
        // Deduplicate and order people by name so runs are deterministic.
        let mut dirs: Vec<String> = file
            .persons
            .iter()
            .map(|(_, name)| sanitize_dir_name(name))
            .collect();
        dirs.sort();
        dirs.dedup();
        if dirs.is_empty() {
            continue;
        }
        let source_abs = PathBuf::from(&file.abs_path);
        if !source_abs.is_file() {
            result.errors.push(format!("{}: file not found on disk", file.rel_path));
            continue;
        }
        let filename = Path::new(&file.rel_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.rel_path.clone());

        // Home folder: where the file already is (if under a matching person), else the first person.
        let home_idx = dirs.iter().position(|d| lives_under(&file.rel_path, d));
        let home = home_idx.map(|i| dirs[i].clone()).unwrap_or_else(|| dirs[0].clone());
        let mut current_rel = file.rel_path.clone();
        let mut current_abs = source_abs.clone();

        if home_idx.is_none() {
            let (new_rel, new_abs) = unique_target(&root, &home, &filename, &source_abs);
            if let Err(e) = move_file(&source_abs, &new_abs) {
                result.errors.push(format!("{}: move failed: {e}", file.rel_path));
                continue;
            }
            // Keep the thumbnail in step with the new relative path.
            let new_thumb = thumb_path_for(thumbnails_dir, &new_rel);
            let mut thumb_str: Option<String> = None;
            if let Some(old_thumb) = file.thumb_path.as_deref().map(Path::new) {
                if old_thumb.is_file() {
                    if let Some(parent) = new_thumb.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if std::fs::rename(old_thumb, &new_thumb).is_ok() {
                        thumb_str = Some(new_thumb.display().to_string());
                    }
                }
            }
            let new_filename = Path::new(&new_rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| filename.clone());
            if let Err(e) = db::rename_file_record(
                db_path,
                file.id,
                &new_rel,
                &new_abs.display().to_string(),
                &new_filename,
            ) {
                // Put the file back so disk and DB stay consistent.
                let _ = move_file(&new_abs, &source_abs);
                result.errors.push(format!("{}: database update failed: {e}", file.rel_path));
                continue;
            }
            if let Some(t) = &thumb_str {
                let _ = db::update_file_thumb_path_by_id(db_path, file.id, t);
            }
            if let Some(parent) = source_abs.parent() {
                touched_dirs.push(parent.to_path_buf());
            }
            result.moved += 1;
            *people.entry(home.clone()).or_default() += 1;
            current_rel = new_rel;
            current_abs = new_abs;
        } else {
            result.skipped += 1;
            *people.entry(home.clone()).or_default() += 1;
        }

        // Copies for every other person in the picture.
        for dir in dirs.iter().filter(|d| **d != home) {
            // A previous run already placed this exact file with that person.
            if db::has_file_with_fingerprint_under(
                db_path,
                root_id,
                &format!("{PEOPLE_DIR}/{dir}/"),
                &file.fingerprint,
            )? {
                result.skipped += 1;
                *people.entry(dir.clone()).or_default() += 1;
                continue;
            }
            let (copy_rel, copy_abs) = unique_target(&root, dir, &filename, &current_abs);
            if let Err(e) = copy_file(&current_abs, &copy_abs) {
                result.errors.push(format!("{current_rel}: copy to {dir} failed: {e}"));
                continue;
            }
            let copy_thumb = thumb_path_for(thumbnails_dir, &copy_rel);
            let mut copy_thumb_str: Option<String> = None;
            if let Some(src_thumb) = db::get_file_path_info(db_path, file.id)
                .ok()
                .and_then(|(_, _, t)| t)
            {
                let src_thumb = Path::new(&src_thumb);
                if src_thumb.is_file() && copy_file(src_thumb, &copy_thumb).is_ok() {
                    copy_thumb_str = Some(copy_thumb.display().to_string());
                }
            }
            let copy_filename = Path::new(&copy_rel)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| filename.clone());
            if let Err(e) = db::clone_file_record(
                db_path,
                file.id,
                &copy_rel,
                &copy_abs.display().to_string(),
                &copy_filename,
                copy_thumb_str.as_deref(),
            ) {
                let _ = std::fs::remove_file(&copy_abs);
                result.errors.push(format!("{current_rel}: database clone failed: {e}"));
                continue;
            }
            result.copied += 1;
            *people.entry(dir.clone()).or_default() += 1;
        }
    }

    // Tidy up folders emptied by the moves (deepest first).
    touched_dirs.sort();
    touched_dirs.dedup();
    touched_dirs.reverse();
    for dir in touched_dirs {
        prune_empty_dirs(&root, dir);
    }

    result.people = people.len() as u64;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::FileRecordUpsert;

    fn record(root_id: i64, root: &Path, rel: &str) -> FileRecordUpsert {
        FileRecordUpsert {
            root_id,
            rel_path: rel.to_string(),
            abs_path: root.join(rel).display().to_string(),
            filename: crate::platform::paths::rel_path_filename(rel).to_string(),
            media_type: "photo".to_string(),
            description: format!("desc of {rel}"),
            extracted_text: String::new(),
            canonical_mentions: String::new(),
            confidence: 0.8,
            lang_hint: "en".to_string(),
            mtime_ns: 1,
            size_bytes: 3,
            fingerprint: format!("fp-{rel}"),
            scan_marker: 1,
            location_text: String::new(),
            dhash: None,
            duration_secs: None,
            video_width: None,
            video_height: None,
            video_codec: None,
            audio_codec: None,
        }
    }

    fn write(path: &Path, content: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn sanitize_dir_name_rules() {
        assert_eq!(sanitize_dir_name("Maria"), "Maria");
        assert_eq!(sanitize_dir_name(" a/b\\c:d "), "a_b_c_d");
        assert_eq!(sanitize_dir_name("dots..."), "dots");
        assert_eq!(sanitize_dir_name("   "), "Sem nome");
        assert_eq!(sanitize_dir_name("Jo\u{e3}o"), "Jo\u{e3}o");
    }

    #[test]
    fn unique_target_appends_suffix_on_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let other = root.join("elsewhere.jpg");
        write(&other, b"x");
        write(&root.join("Pessoas/Ana/pic.jpg"), b"taken");
        let (rel, abs) = unique_target(root, "Ana", "pic.jpg", &other);
        assert_eq!(rel, "Pessoas/Ana/pic_2.jpg");
        assert_eq!(abs, root.join("Pessoas/Ana/pic_2.jpg"));
        // The file already at the target counts as "itself", not a collision.
        let (rel, _) = unique_target(root, "Ana", "pic.jpg", &root.join("Pessoas/Ana/pic.jpg"));
        assert_eq!(rel, "Pessoas/Ana/pic.jpg");
    }

    #[test]
    fn organizes_moves_copies_and_updates_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("index.sqlite");
        db::init_database(&db_path).unwrap();
        let root = tmp.path().join("Fotos");
        let thumbs = tmp.path().join("thumbs");
        std::fs::create_dir_all(&root).unwrap();
        let root_id = db::upsert_root(&db_path, &root.display().to_string()).unwrap();

        write(&root.join("praia/a.jpg"), b"aaa");
        write(&root.join("praia/b.jpg"), b"bbb");
        write(&root.join("solo.jpg"), b"ccc");
        write(&root.join("Pessoas/Ana/done.jpg"), b"ddd");
        let a = db::upsert_file_record(&db_path, &record(root_id, &root, "praia/a.jpg")).unwrap();
        let b = db::upsert_file_record(&db_path, &record(root_id, &root, "praia/b.jpg")).unwrap();
        let solo = db::upsert_file_record(&db_path, &record(root_id, &root, "solo.jpg")).unwrap();
        let done = db::upsert_file_record(&db_path, &record(root_id, &root, "Pessoas/Ana/done.jpg")).unwrap();
        write(&thumbs.join("praia/a.jpg"), b"thumb-a");
        db::update_file_thumb_path_by_id(&db_path, a, &thumbs.join("praia/a.jpg").display().to_string()).unwrap();

        let ana = db::create_person_for_test(&db_path, "Ana");
        let bruno = db::create_person_for_test(&db_path, "Bruno");
        db::insert_face_for_test(&db_path, a, Some(ana));
        db::insert_face_for_test(&db_path, a, Some(bruno));
        db::insert_face_for_test(&db_path, b, Some(bruno));
        db::insert_face_for_test(&db_path, done, Some(ana));
        db::insert_face_for_test(&db_path, solo, None); // unassigned face: stays put

        let result = organize_root_by_people(&db_path, root_id, &thumbs).unwrap();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.moved, 2, "a.jpg and b.jpg");
        assert_eq!(result.copied, 1, "a.jpg copied for Bruno");
        assert_eq!(result.skipped, 1, "done.jpg already in place");
        assert_eq!(result.people, 2);

        // Disk layout.
        assert!(root.join("Pessoas/Ana/a.jpg").is_file());
        assert!(root.join("Pessoas/Bruno/a.jpg").is_file());
        assert!(root.join("Pessoas/Bruno/b.jpg").is_file());
        assert!(root.join("solo.jpg").is_file());
        assert!(!root.join("praia").exists(), "emptied folder pruned");
        assert_eq!(std::fs::read(root.join("Pessoas/Bruno/a.jpg")).unwrap(), b"aaa");
        assert!(thumbs.join("Pessoas/Ana/a.jpg").is_file());
        assert!(thumbs.join("Pessoas/Bruno/a.jpg").is_file());

        // DB: moved rows re-pathed, copy cloned with its faces and description.
        let (abs_a, rel_a, thumb_a) = db::get_file_path_info(&db_path, a).unwrap();
        assert_eq!(rel_a, "Pessoas/Ana/a.jpg");
        assert_eq!(abs_a, root.join("Pessoas/Ana/a.jpg").display().to_string());
        assert_eq!(thumb_a.as_deref(), Some(thumbs.join("Pessoas/Ana/a.jpg").display().to_string().as_str()));
        let all = db::search_images(&db_path, &crate::models::SearchRequest::default()).unwrap();
        assert_eq!(all.total, 5);
        let copy = all.items.iter().find(|i| i.rel_path == "Pessoas/Bruno/a.jpg").unwrap();
        assert_eq!(copy.description, "desc of praia/a.jpg");
        assert_ne!(copy.id, a);
        let bruno_files = db::list_files_with_persons(&db_path, root_id).unwrap();
        let copy_persons = bruno_files.iter().find(|f| f.id == copy.id).unwrap();
        assert_eq!(copy_persons.persons.len(), 2, "faces cloned onto the copy");

        // Idempotent: a second run changes nothing.
        let again = organize_root_by_people(&db_path, root_id, &thumbs).unwrap();
        assert_eq!((again.moved, again.copied), (0, 0));
        assert!(again.errors.is_empty());
    }
}
