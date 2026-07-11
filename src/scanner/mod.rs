//! Recursively scans a directory tree and builds an [`Entry`] tree annotated
//! with disk usage (in bytes) at every node. This module has no GUI
//! dependencies so it can be exercised with plain `cargo test`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// A single file or directory discovered during a scan.
///
/// Directories store the *total* size of everything beneath them in `size`,
/// which is what makes the treemap meaningful. `children` is empty for
/// files, and is sorted largest-first for directories once a scan finishes
/// (see [`Entry::sort_children_recursive`]).
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Entry>,
}

impl Entry {
    /// Sorts this entry's children largest-first, recursively. Doing this
    /// once after a scan (rather than re-sorting every frame) keeps the UI
    /// code simple: it can assume children already arrive in the order the
    /// treemap should lay them out.
    pub fn sort_children_recursive(&mut self) {
        self.children
            .sort_unstable_by(|a, b| b.size.cmp(&a.size));
        for child in &mut self.children {
            child.sort_children_recursive();
        }
    }

    /// Number of direct children.
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// Shared, lock-free counters a caller can poll from another thread to show
/// "N files / X GB scanned so far" while a scan is in flight.
#[derive(Default)]
pub struct ScanProgress {
    pub files_scanned: AtomicU64,
    pub bytes_scanned: AtomicU64,
}

/// Recursively scans `root`, returning a tree of [`Entry`] with computed
/// sizes. Pass a shared [`AtomicBool`] as `cancel` if you want to be able to
/// abort a long-running scan from another thread (set it to `true`); the
/// scan will stop descending soon after and return whatever partial tree it
/// had built. `progress`, if given, is updated as files are discovered so a
/// caller can display live counters.
pub fn scan(root: &Path, cancel: &AtomicBool, progress: Option<&ScanProgress>) -> Entry {
    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());

    let mut entry = scan_dir(root, name, cancel, progress);
    entry.sort_children_recursive();
    entry
}

fn scan_dir(path: &Path, name: String, cancel: &AtomicBool, progress: Option<&ScanProgress>) -> Entry {
    let mut children = Vec::new();
    let mut total: u64 = 0;

    if !cancel.load(Ordering::Relaxed) {
        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry_result in read_dir {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }

                let Ok(dir_entry) = entry_result else {
                    continue;
                };

                // `file_type()` reflects the entry itself and does not
                // follow symlinks/junctions, unlike `metadata()` on some
                // platforms. Skipping symlinks avoids double-counting and
                // infinite loops on cyclic junctions.
                let Ok(file_type) = dir_entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() {
                    continue;
                }

                let child_name = dir_entry.file_name().to_string_lossy().into_owned();
                let child_path = dir_entry.path();

                if file_type.is_dir() {
                    let child = scan_dir(&child_path, child_name, cancel, progress);
                    total += child.size;
                    children.push(child);
                } else {
                    let size = dir_entry.metadata().map(|m| m.len()).unwrap_or(0);
                    total += size;
                    if let Some(p) = progress {
                        p.files_scanned.fetch_add(1, Ordering::Relaxed);
                        p.bytes_scanned.fetch_add(size, Ordering::Relaxed);
                    }
                    children.push(Entry {
                        name: child_name,
                        path: child_path,
                        size,
                        is_dir: false,
                        children: Vec::new(),
                    });
                }
            }
        }
    }

    Entry {
        name,
        path: path.to_path_buf(),
        size: total,
        is_dir: true,
        children,
    }
}

/// Formats a byte count the way SpaceSniffer-style tools do: a small number
/// of significant digits and the largest unit that keeps the value >= 1.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    /// Minimal RAII temp-directory helper so tests don't need an external
    /// crate: creates a uniquely-named directory under the OS temp dir and
    /// removes it (and everything inside it) when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "space_sniffer_test_{}_{nanos}_{n}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Builds a small throwaway directory tree under the OS temp dir:
    /// root/
    ///   a.txt          (100 bytes)
    ///   sub/
    ///     b.txt        (50 bytes)
    ///     empty_dir/
    fn make_fixture() -> TempDir {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), vec![0u8; 100]).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("b.txt"), vec![0u8; 50]).unwrap();
        fs::create_dir(sub.join("empty_dir")).unwrap();
        dir
    }

    #[test]
    fn computes_recursive_sizes() {
        let dir = make_fixture();
        let cancel = AtomicBool::new(false);
        let tree = scan(dir.path(), &cancel, None);

        assert!(tree.is_dir);
        assert_eq!(tree.size, 150);
        assert_eq!(tree.child_count(), 2); // a.txt + sub/

        let sub = tree
            .children
            .iter()
            .find(|e| e.name == "sub")
            .expect("sub dir present");
        assert_eq!(sub.size, 50);
        assert_eq!(sub.child_count(), 2); // b.txt + empty_dir/
    }

    #[test]
    fn children_sorted_largest_first() {
        let dir = make_fixture();
        let cancel = AtomicBool::new(false);
        let tree = scan(dir.path(), &cancel, None);

        // a.txt (100 bytes) should sort before sub/ (50 bytes).
        assert_eq!(tree.children[0].name, "a.txt");
        assert_eq!(tree.children[1].name, "sub");
    }

    #[test]
    fn progress_counters_track_files_and_bytes() {
        let dir = make_fixture();
        let cancel = AtomicBool::new(false);
        let progress = ScanProgress::default();
        let _tree = scan(dir.path(), &cancel, Some(&progress));

        assert_eq!(progress.files_scanned.load(Ordering::Relaxed), 2);
        assert_eq!(progress.bytes_scanned.load(Ordering::Relaxed), 150);
    }

    #[test]
    fn cancel_stops_the_scan_early() {
        let dir = make_fixture();
        let cancel = AtomicBool::new(true); // already cancelled
        let tree = scan(dir.path(), &cancel, None);

        // With cancel already set, scan_dir should not even read the top
        // directory's entries.
        assert_eq!(tree.size, 0);
        assert_eq!(tree.child_count(), 0);
    }

    #[test]
    fn format_size_uses_sensible_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }
}