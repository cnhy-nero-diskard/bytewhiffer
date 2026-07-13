//! Pure, egui-free derived analytics over a scanned tree.
//!
//! Every function here is a whole-subtree aggregation over data a scan
//! already produced (extension size totals, a biggest-entries leaderboard,
//! small-file-blizzard detection, known-junk name matching) — no disk I/O,
//! no new scan pass. It is deliberately kept free of any `egui` dependency,
//! like `treemap.rs` and `scanner/`, so the aggregation logic can be
//! unit-tested without a display; `app.rs` is the adapter that renders the
//! results and wires click-to-focus.
//!
//! The aggregations operate on [`InsightNode`], a minimal borrowed view of a
//! tree node (name, path, size, is_dir, children). Both `app::Node` (the
//! live UI tree) and [`crate::scanner::Entry`] (the engine's final tree)
//! borrow into it, so the functions never depend on either concrete type —
//! mirroring how `treemap::squarify` takes bare sizes rather than a tree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How many direct children a directory must have before it can be
/// considered a small-file blizzard.
const BLIZZARD_MIN_CHILDREN: usize = 100;
/// The largest average child size (bytes) a blizzard directory may have.
/// Above this the directory holds substantial content, not clutter.
const BLIZZARD_MAX_AVG_SIZE: u64 = 64 * 1024;

/// A minimal borrowed view of one tree node the aggregations walk over.
/// Children own their own `InsightNode`s (borrowing name/path from the
/// source tree), so building one is a shallow O(nodes) walk with no string
/// copying.
pub struct InsightNode<'a> {
    pub name: &'a str,
    pub path: &'a Path,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<InsightNode<'a>>,
}

/// A single ranked entry in the biggest-files/folders leaderboard.
#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub name: String,
    /// Names from the focus node down to this entry (inclusive), the same
    /// relative trail `app.rs` appends to `self.focus` to navigate.
    pub trail: Vec<String>,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
}

/// A directory flagged as a small-file blizzard: many children, low average
/// child size.
#[derive(Debug, Clone)]
pub struct BlizzardEntry {
    pub name: String,
    pub trail: Vec<String>,
    pub child_count: usize,
    pub avg_child_size: u64,
}

/// A file or directory whose name matches a known-junk pattern.
#[derive(Debug, Clone)]
pub struct JunkEntry {
    pub name: String,
    pub trail: Vec<String>,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    /// Human-readable category of the matched pattern, e.g. "node_modules".
    pub category: &'static str,
}

impl<'a> InsightNode<'a> {
    /// Borrows a [`crate::scanner::Entry`] tree into the insight view. The
    /// live-scan `app::Node` path lives in `app.rs` (its type is private);
    /// this keeps the two source trees symmetric and is exercised by the
    /// unit tests below.
    #[allow(dead_code)]
    pub fn from_entry(entry: &'a crate::scanner::Entry) -> InsightNode<'a> {
        InsightNode {
            name: &entry.name,
            path: &entry.path,
            size: entry.size,
            is_dir: entry.is_dir,
            children: entry.children.iter().map(InsightNode::from_entry).collect(),
        }
    }

    /// Total size per distinct file extension across every file in the
    /// subtree, sorted largest first (ties broken by extension name). The
    /// extension is lowercased so it keys the same color
    /// `theme::color_for_extension` assigns; extensionless files collapse to
    /// a single `""` entry. Drives both the legend and the size breakdown.
    pub fn extension_totals(&self) -> Vec<(String, u64)> {
        fn walk(node: &InsightNode, totals: &mut HashMap<String, u64>) {
            if node.is_dir {
                for child in &node.children {
                    walk(child, totals);
                }
            } else {
                *totals.entry(extension_of(node.name)).or_insert(0) += node.size;
            }
        }
        let mut totals: HashMap<String, u64> = HashMap::new();
        walk(self, &mut totals);
        let mut out: Vec<(String, u64)> = totals.into_iter().collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// The `n` largest files and folders anywhere in the subtree, ranked by
    /// size, each carrying its relative trail for focus navigation.
    pub fn leaderboard(&self, n: usize) -> Vec<LeaderboardEntry> {
        fn walk(node: &InsightNode, trail: &mut Vec<String>, out: &mut Vec<LeaderboardEntry>) {
            for child in &node.children {
                trail.push(child.name.to_string());
                out.push(LeaderboardEntry {
                    name: child.name.to_string(),
                    trail: trail.clone(),
                    path: child.path.to_path_buf(),
                    size: child.size,
                    is_dir: child.is_dir,
                });
                walk(child, trail, out);
                trail.pop();
            }
        }
        let mut out = Vec::new();
        walk(self, &mut Vec::new(), &mut out);
        out.sort_by(|a, b| b.size.cmp(&a.size));
        out.truncate(n);
        out
    }

    /// Directories in the subtree with a high child count but a low average
    /// child size — `node_modules`-style clutter — sorted most-cluttered
    /// first. The focus node itself is never flagged, only its descendants.
    pub fn blizzard_flags(&self) -> Vec<BlizzardEntry> {
        fn walk(node: &InsightNode, trail: &mut Vec<String>, out: &mut Vec<BlizzardEntry>) {
            for child in &node.children {
                if !child.is_dir {
                    continue;
                }
                trail.push(child.name.to_string());
                let count = child.children.len();
                if count >= BLIZZARD_MIN_CHILDREN {
                    let avg = child.size / count as u64;
                    if avg <= BLIZZARD_MAX_AVG_SIZE {
                        out.push(BlizzardEntry {
                            name: child.name.to_string(),
                            trail: trail.clone(),
                            child_count: count,
                            avg_child_size: avg,
                        });
                    }
                }
                walk(child, trail, out);
                trail.pop();
            }
        }
        let mut out = Vec::new();
        walk(self, &mut Vec::new(), &mut out);
        out.sort_by(|a, b| b.child_count.cmp(&a.child_count));
        out
    }

    /// Files and directories in the subtree whose names match a fixed set of
    /// known-junk patterns (installers, build caches, `node_modules`,
    /// browser cache dirs), sorted largest first. A matched directory is not
    /// descended into — flagging it stands in for everything beneath it.
    pub fn junk_suggestions(&self) -> Vec<JunkEntry> {
        fn walk(node: &InsightNode, trail: &mut Vec<String>, out: &mut Vec<JunkEntry>) {
            for child in &node.children {
                trail.push(child.name.to_string());
                if let Some(category) = junk_category(child.name, child.is_dir) {
                    out.push(JunkEntry {
                        name: child.name.to_string(),
                        trail: trail.clone(),
                        path: child.path.to_path_buf(),
                        is_dir: child.is_dir,
                        size: child.size,
                        category,
                    });
                    // A matched junk directory stands in for its contents;
                    // don't surface junk-within-junk.
                } else if child.is_dir {
                    walk(child, trail, out);
                }
                trail.pop();
            }
        }
        let mut out = Vec::new();
        walk(self, &mut Vec::new(), &mut out);
        out.sort_by(|a, b| b.size.cmp(&a.size));
        out
    }
}

/// The lowercased extension of a file name, or `""` for extensionless files
/// (and dotfiles, whose leading dot is not an extension) — matching
/// `theme`'s own extension logic so the drawer keys the same colors.
fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Classifies a name against the fixed known-junk ruleset, returning the
/// human-readable category if it matches. Conservative on purpose: junk
/// flags are advisory, so false positives are worse than misses.
fn junk_category(name: &str, is_dir: bool) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if is_dir {
        match lower.as_str() {
            "node_modules" => Some("node_modules"),
            "target" | "build" | "dist" | "out" | "__pycache__" | ".gradle" | ".cache"
            | ".next" | ".nuxt" => Some("build cache"),
            "cache" | "code cache" | "gpucache" | "shadercache" => Some("browser cache"),
            _ => None,
        }
    } else {
        match extension_of(name).as_str() {
            "msi" => Some("installer"),
            "exe" if lower.contains("setup") || lower.contains("install") => Some("installer"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::Entry;

    fn file(name: &str, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from(name),
            size,
            is_dir: false,
            children: Vec::new(),
        }
    }

    fn dir(name: &str, children: Vec<Entry>) -> Entry {
        let size = children.iter().map(|c| c.size).sum();
        Entry {
            name: name.to_string(),
            path: PathBuf::from(name),
            size,
            is_dir: true,
            children,
        }
    }

    #[test]
    fn extension_totals_sum_and_sort_largest_first() {
        let tree = dir(
            "root",
            vec![
                file("a.rs", 100),
                file("b.rs", 50),
                file("c.txt", 30),
                dir("sub", vec![file("d.rs", 10), file("Makefile", 5)]),
            ],
        );
        let view = InsightNode::from_entry(&tree);
        let totals = view.extension_totals();
        // rs = 100 + 50 + 10 = 160, txt = 30, "" (Makefile) = 5.
        assert_eq!(
            totals,
            vec![
                ("rs".to_string(), 160),
                ("txt".to_string(), 30),
                (String::new(), 5),
            ]
        );
    }

    #[test]
    fn extension_totals_are_case_insensitive() {
        let tree = dir("root", vec![file("a.PNG", 10), file("b.png", 5)]);
        let totals = InsightNode::from_entry(&tree).extension_totals();
        assert_eq!(totals, vec![("png".to_string(), 15)]);
    }

    #[test]
    fn leaderboard_ranks_by_size_and_carries_trail() {
        let tree = dir(
            "root",
            vec![
                file("small.txt", 10),
                dir("big", vec![file("huge.bin", 900)]),
                file("mid.txt", 100),
            ],
        );
        let board = InsightNode::from_entry(&tree).leaderboard(3);
        // "big" (900) ranks above its own child "huge.bin" (900) only by
        // insertion tie order, but both outrank mid (100) and small (10).
        let names: Vec<&str> = board.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names[0..2].iter().collect::<std::collections::HashSet<_>>(),
                   ["big", "huge.bin"].iter().collect());
        assert!(board.iter().all(|e| e.size >= 100));
        // The nested file's trail is relative to the focus node.
        let huge = board.iter().find(|e| e.name == "huge.bin").unwrap();
        assert_eq!(huge.trail, vec!["big".to_string(), "huge.bin".to_string()]);
        assert!(!huge.is_dir);
    }

    #[test]
    fn leaderboard_truncates_to_n() {
        let tree = dir(
            "root",
            (0..10).map(|i| file(&format!("f{i}.dat"), i)).collect(),
        );
        assert_eq!(InsightNode::from_entry(&tree).leaderboard(3).len(), 3);
    }

    #[test]
    fn blizzard_catches_many_small_children_and_skips_normal_dirs() {
        let clutter = dir(
            "node_modules",
            (0..150).map(|i| file(&format!("m{i}.js"), 1024)).collect(),
        );
        let normal = dir(
            "media",
            (0..3).map(|i| file(&format!("v{i}.mp4"), 500_000_000)).collect(),
        );
        let tree = dir("root", vec![clutter, normal]);
        let flags = InsightNode::from_entry(&tree).blizzard_flags();
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].name, "node_modules");
        assert_eq!(flags[0].child_count, 150);
        assert_eq!(flags[0].avg_child_size, 1024);
    }

    #[test]
    fn blizzard_skips_dir_with_high_count_but_large_average() {
        // 120 children, but each is large, so average is well over the cap.
        let big = dir(
            "assets",
            (0..120).map(|i| file(&format!("a{i}.bin"), 10 * 1024 * 1024)).collect(),
        );
        let tree = dir("root", vec![big]);
        assert!(InsightNode::from_entry(&tree).blizzard_flags().is_empty());
    }

    #[test]
    fn junk_matches_known_patterns_and_skips_unrelated() {
        let tree = dir(
            "root",
            vec![
                dir("node_modules", vec![file("index.js", 100)]),
                dir("target", vec![file("app", 5000)]),
                dir("src", vec![file("main.rs", 200)]),
                file("setup_v2.exe", 9000),
                file("game.msi", 8000),
                file("photo.jpg", 300),
            ],
        );
        let junk = InsightNode::from_entry(&tree).junk_suggestions();
        let names: Vec<&str> = junk.iter().map(|e| e.name.as_str()).collect();
        // Matched: node_modules, target, setup_v2.exe, game.msi. Sorted by
        // size descending.
        assert_eq!(names, vec!["setup_v2.exe", "game.msi", "target", "node_modules"]);
        assert!(!junk.iter().any(|e| e.name == "src" || e.name == "photo.jpg"));
        assert_eq!(
            junk.iter().find(|e| e.name == "node_modules").unwrap().category,
            "node_modules"
        );
    }

    #[test]
    fn junk_does_not_descend_into_matched_directory() {
        // A node_modules holding a nested node_modules should surface only
        // the outer one.
        let tree = dir(
            "root",
            vec![dir(
                "node_modules",
                vec![dir("node_modules", vec![file("x.js", 10)])],
            )],
        );
        let junk = InsightNode::from_entry(&tree).junk_suggestions();
        assert_eq!(junk.len(), 1);
        assert_eq!(junk[0].trail, vec!["node_modules".to_string()]);
    }
}
