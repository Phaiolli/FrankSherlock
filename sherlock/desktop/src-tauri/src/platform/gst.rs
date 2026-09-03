//! GStreamer plugin path repair.
//!
//! The AppImage runtime exports
//! `GST_PLUGIN_SYSTEM_PATH_1_0=$APPDIR/usr/lib/gstreamer-1.0:` for every
//! process it starts. That directory is empty (the bundle ships the GStreamer
//! core libraries but no plugins), and setting the variable *replaces* the
//! compiled-in system path — so inside the AppImage GStreamer finds only its
//! static elements: no demuxer, no decoder, not even `playbin`. WebKit's media
//! player then aborts the whole web process as soon as a `<video>` starts,
//! which looks like the app freezing.
//!
//! At startup (before WebKit is initialised) we put the host's plugin
//! directories back on that path.

use std::path::Path;

/// Directories where distributions install GStreamer 1.0 plugins.
const SYSTEM_PLUGIN_DIRS: &[&str] = &[
    "/usr/lib64/gstreamer-1.0",
    "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
    "/usr/lib/aarch64-linux-gnu/gstreamer-1.0",
    "/usr/lib/gstreamer-1.0",
    "/usr/local/lib64/gstreamer-1.0",
    "/usr/local/lib/gstreamer-1.0",
];

const PLUGIN_PATH_VARS: &[&str] = &["GST_PLUGIN_SYSTEM_PATH_1_0", "GST_PLUGIN_SYSTEM_PATH"];

/// True when the directory holds at least one `libgst*.so` plugin.
fn has_plugins(dir: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(Path::new(dir)) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.starts_with("libgst") && n.contains(".so"))
    })
}

/// Keep the entries of `current` that really hold plugins, then append the
/// system directories that do. `None` means "unset the variable" — nothing
/// usable was found, so GStreamer's own default is the better guess.
fn rebuild_path(
    current: Option<&str>,
    system_dirs: &[&str],
    dir_has_plugins: impl Fn(&str) -> bool,
) -> Option<String> {
    let mut dirs: Vec<String> = Vec::new();
    let mut push = |dir: &str| {
        if dir.is_empty() || dirs.iter().any(|d| d == dir) {
            return;
        }
        if dir_has_plugins(dir) {
            dirs.push(dir.to_string());
        }
    };
    for dir in current.unwrap_or_default().split(':') {
        push(dir);
    }
    for dir in system_dirs {
        push(dir);
    }
    if dirs.is_empty() {
        None
    } else {
        Some(dirs.join(":"))
    }
}

/// Repair the GStreamer plugin path of *this* process, and therefore of the
/// WebKit processes it spawns. Only touches anything inside an AppImage; a
/// normal install already sees the system plugins.
///
/// Must run before WebKit is initialised, while the process is single
/// threaded.
pub fn repair_plugin_path() {
    if !cfg!(target_os = "linux") || std::env::var_os("APPDIR").is_none() {
        return;
    }
    for var in PLUGIN_PATH_VARS {
        let current = std::env::var(var).ok();
        if current.is_none() {
            continue;
        }
        match rebuild_path(current.as_deref(), SYSTEM_PLUGIN_DIRS, has_plugins) {
            Some(path) if Some(&path) != current.as_ref() => {
                log::info!("AppImage: {var} -> {path}");
                std::env::set_var(var, &path);
            }
            Some(_) => {}
            None => {
                log::warn!("AppImage: no GStreamer plugins found; clearing {var}");
                std::env::remove_var(var);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_system_dirs_and_drops_the_empty_bundle_dir() {
        let stocked = |d: &str| d == "/usr/lib64/gstreamer-1.0";
        let path = rebuild_path(
            Some("/tmp/.mount_app/usr/lib/gstreamer-1.0:"),
            SYSTEM_PLUGIN_DIRS,
            stocked,
        );
        assert_eq!(path.as_deref(), Some("/usr/lib64/gstreamer-1.0"));
    }

    #[test]
    fn keeps_a_bundled_dir_that_does_hold_plugins_and_keeps_it_first() {
        let stocked = |d: &str| d == "/bundle/gstreamer-1.0" || d == "/usr/lib64/gstreamer-1.0";
        let path = rebuild_path(Some("/bundle/gstreamer-1.0"), SYSTEM_PLUGIN_DIRS, stocked);
        assert_eq!(
            path.as_deref(),
            Some("/bundle/gstreamer-1.0:/usr/lib64/gstreamer-1.0")
        );
    }

    #[test]
    fn deduplicates_repeated_entries() {
        let stocked = |d: &str| d == "/usr/lib64/gstreamer-1.0";
        let path = rebuild_path(
            Some("/usr/lib64/gstreamer-1.0:/usr/lib64/gstreamer-1.0"),
            SYSTEM_PLUGIN_DIRS,
            stocked,
        );
        assert_eq!(path.as_deref(), Some("/usr/lib64/gstreamer-1.0"));
    }

    #[test]
    fn returns_none_when_nothing_holds_plugins() {
        assert_eq!(rebuild_path(Some("/nowhere:"), SYSTEM_PLUGIN_DIRS, |_| false), None);
    }

    #[test]
    fn has_plugins_is_false_for_an_empty_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!has_plugins(&tmp.path().display().to_string()));
        std::fs::write(tmp.path().join("libgstcoreelements.so"), b"x").expect("write");
        assert!(has_plugins(&tmp.path().display().to_string()));
    }

    #[test]
    fn repair_is_a_no_op_outside_an_appimage() {
        // APPDIR is not set in the test environment: nothing must change.
        let before = std::env::var("GST_PLUGIN_SYSTEM_PATH_1_0").ok();
        repair_plugin_path();
        assert_eq!(std::env::var("GST_PLUGIN_SYSTEM_PATH_1_0").ok(), before);
    }
}
