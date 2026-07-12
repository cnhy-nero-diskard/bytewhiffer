//! Phase 1 scanning engine: a parallel recursive directory walker. Needs no
//! elevation and works on any filesystem or drive, which is why it is the
//! universal fallback engine; a future NTFS `$MFT` reader can be faster on
//! local NTFS volumes but can never replace this one entirely.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use rayon::prelude::*;

use super::{Availability, Entry, ScanContext, ScanEngine, ScanError, ScanEvent};

/// The parallel directory-walk engine.
pub struct WalkerEngine;

impl ScanEngine for WalkerEngine {
    fn name(&self) -> &'static str {
        "walker"
    }

    fn is_available(&self, _target: &Path) -> Availability {
        // Walking works on anything the process can read_dir; per-entry
        // permission problems are handled by skipping during the scan, not
        // by declaring the whole target off-limits up front.
        Availability::Available
    }

    fn scan(&self, target: &Path, ctx: &ScanContext) -> Result<Entry, ScanError> {
        // The root must at least be readable as a directory; anything less
        // is a total failure the caller should see, unlike unreadable
        // entries deeper in the tree, which are skipped.
        if let Err(err) = std::fs::read_dir(target) {
            ctx.progress.mark_complete();
            return Err(ScanError::RootUnreadable(err));
        }

        let name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.to_string_lossy().into_owned());

        let mut entry = scan_dir(target, name, ctx);
        entry.sort_children_recursive();
        ctx.progress.mark_complete();
        Ok(entry)
    }
}

fn scan_dir(path: &Path, name: String, ctx: &ScanContext) -> Entry {
    let mut children = Vec::new();
    let mut subdirs: Vec<(PathBuf, String)> = Vec::new();
    let mut total: u64 = 0;

    if !ctx.is_cancelled() {
        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry_result in read_dir {
                if ctx.is_cancelled() {
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
                    ctx.progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
                    ctx.emit(ScanEvent::Discovered {
                        path: child_path.clone(),
                        size: 0,
                        is_dir: true,
                    });
                    subdirs.push((child_path, child_name));
                } else {
                    let size = dir_entry.metadata().map(|m| m.len()).unwrap_or(0);
                    total += size;
                    ctx.progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                    ctx.progress.bytes_scanned.fetch_add(size, Ordering::Relaxed);
                    ctx.emit(ScanEvent::Discovered {
                        path: child_path.clone(),
                        size,
                        is_dir: false,
                    });
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

    // Subdirectories recurse in parallel; rayon's work stealing handles the
    // nesting. Files were already accounted for above, so this only adds
    // the directory subtotals.
    let dir_children: Vec<Entry> = subdirs
        .into_par_iter()
        .map(|(child_path, child_name)| scan_dir(&child_path, child_name, ctx))
        .collect();
    for child in dir_children {
        total += child.size;
        children.push(child);
    }

    Entry {
        name,
        path: path.to_path_buf(),
        size: total,
        is_dir: true,
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::ScanProgress;
    use std::fs;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;

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
                "bytewhiffer_test_{}_{nanos}_{n}",
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
        let ctx = ScanContext::new();
        let tree = WalkerEngine.scan(dir.path(), &ctx).unwrap();

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
        let ctx = ScanContext::new();
        let tree = WalkerEngine.scan(dir.path(), &ctx).unwrap();

        // a.txt (100 bytes) should sort before sub/ (50 bytes).
        assert_eq!(tree.children[0].name, "a.txt");
        assert_eq!(tree.children[1].name, "sub");
    }

    #[test]
    fn progress_counters_track_files_bytes_and_dirs() {
        let dir = make_fixture();
        let ctx = ScanContext::new();
        let _tree = WalkerEngine.scan(dir.path(), &ctx).unwrap();

        assert_eq!(ctx.progress.files_scanned.load(Ordering::Relaxed), 2);
        assert_eq!(ctx.progress.bytes_scanned.load(Ordering::Relaxed), 150);
        // sub/ and sub/empty_dir/ — the fixture's two subdirectories.
        assert_eq!(ctx.progress.dirs_scanned.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn progress_marked_complete_when_scan_returns() {
        let dir = make_fixture();
        let ctx = ScanContext::new();
        assert!(!ctx.progress.is_complete());
        let _tree = WalkerEngine.scan(dir.path(), &ctx).unwrap();
        assert!(ctx.progress.is_complete());
    }

    #[test]
    fn cancel_stops_the_scan_early() {
        let dir = make_fixture();
        let ctx = ScanContext {
            cancel: Arc::new(AtomicBool::new(true)), // already cancelled
            ..ScanContext::new()
        };
        let tree = WalkerEngine.scan(dir.path(), &ctx).unwrap();

        // With cancel already set, scan_dir should not even read the top
        // directory's entries.
        assert_eq!(tree.size, 0);
        assert_eq!(tree.child_count(), 0);
    }

    #[test]
    fn is_available_reports_available() {
        let dir = make_fixture();
        assert_eq!(
            WalkerEngine.is_available(dir.path()),
            Availability::Available
        );
    }

    #[test]
    fn unreadable_root_is_a_scan_error() {
        let dir = TempDir::new();
        let missing = dir.path().join("does_not_exist");
        let ctx = ScanContext::new();
        let result = WalkerEngine.scan(&missing, &ctx);
        assert!(matches!(result, Err(ScanError::RootUnreadable(_))));
        // Even a failed scan must leave progress in a final state.
        assert!(ctx.progress.is_complete());
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_subdirectory_is_skipped_not_fatal() {
        use std::os::unix::fs::PermissionsExt;

        let dir = make_fixture();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::write(locked.join("hidden.txt"), vec![0u8; 25]).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let ctx = ScanContext::new();
        let result = WalkerEngine.scan(dir.path(), &ctx);

        // Restore permissions so TempDir::drop can clean up.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        let tree = result.expect("partial success, not a ScanError");
        // The locked dir appears (it was listable) but its contents don't.
        let locked_entry = tree
            .children
            .iter()
            .find(|e| e.name == "locked")
            .expect("locked dir present");
        assert_eq!(locked_entry.size, 0);
        assert_eq!(locked_entry.child_count(), 0);
        // The rest of the fixture is unaffected.
        assert_eq!(tree.size, 150);
    }

    #[test]
    fn events_stream_every_discovered_entry() {
        let dir = make_fixture();
        let (tx, rx) = mpsc::channel();
        let ctx = ScanContext::new().with_events(tx);
        let _tree = WalkerEngine.scan(dir.path(), &ctx).unwrap();

        let events: Vec<ScanEvent> = rx.try_iter().collect();
        let mut files = 0;
        let mut dirs = 0;
        for event in &events {
            let ScanEvent::Discovered { size, is_dir, .. } = event;
            if *is_dir {
                dirs += 1;
            } else {
                files += 1;
                assert!(*size > 0);
            }
        }
        assert_eq!(files, 2); // a.txt + b.txt
        assert_eq!(dirs, 2); // sub + empty_dir
    }
}
