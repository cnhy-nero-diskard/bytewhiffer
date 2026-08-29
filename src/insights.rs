//! Pure, egui-free derived analytics over a scanned tree.
//!
//! Every function here is a whole-subtree aggregation over data a scan
//! already produced (extension size totals, a biggest-entries leaderboard,
//! small-file-blizzard detection, cleanup-candidate name matching) — no disk I/O,
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

/// How much confidence the name-only cleanup heuristic can provide. These
/// labels are advisory, never a claim that deleting the entry is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupConfidence {
    High,
    Medium,
    ContextDependent,
}

impl CleanupConfidence {
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "High confidence",
            Self::Medium => "Medium confidence",
            Self::ContextDependent => "Context-dependent",
        }
    }
}

/// Broad kind of cleanup candidate matched by the name-only classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupCategory {
    DependencyCache,
    BuildOutput,
    BrowserCache,
    Installer,
}

impl CleanupCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::DependencyCache => "Dependency cache",
            Self::BuildOutput => "Build output",
            Self::BrowserCache => "Application cache",
            Self::Installer => "Installer",
        }
    }
}

/// Structured explanation for a cleanup-candidate match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupClassification {
    pub category: CleanupCategory,
    pub reason: &'static str,
    pub confidence: CleanupConfidence,
}

/// A file or directory whose name matches a cleanup-candidate pattern.
#[derive(Debug, Clone)]
pub struct CleanupCandidate {
    pub name: String,
    pub trail: Vec<String>,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub classification: CleanupClassification,
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
        out.sort_by_key(|b| std::cmp::Reverse(b.size));
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
        out.sort_by_key(|b| std::cmp::Reverse(b.child_count));
        out
    }

    /// Files and directories in the subtree whose names match a fixed set of
    /// cleanup-candidate patterns (installers, build outputs, dependency
    /// caches, and application caches), sorted largest first. A matched
    /// directory is not descended into — the candidate stands in for
    /// everything beneath it.
    pub fn cleanup_candidates(&self) -> Vec<CleanupCandidate> {
        fn walk(node: &InsightNode, trail: &mut Vec<String>, out: &mut Vec<CleanupCandidate>) {
            for child in &node.children {
                trail.push(child.name.to_string());
                if let Some(classification) = classify_cleanup_candidate(child.name, child.is_dir) {
                    out.push(CleanupCandidate {
                        name: child.name.to_string(),
                        trail: trail.clone(),
                        path: child.path.to_path_buf(),
                        is_dir: child.is_dir,
                        size: child.size,
                        classification,
                    });
                    // A matched directory stands in for its contents; don't
                    // surface nested candidates that would be acted on twice.
                } else if child.is_dir {
                    walk(child, trail, out);
                }
                trail.pop();
            }
        }
        let mut out = Vec::new();
        walk(self, &mut Vec::new(), &mut out);
        out.sort_by_key(|candidate| std::cmp::Reverse(candidate.size));
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

/// Classifies a name against the fixed cleanup-candidate ruleset. The
/// classifier intentionally returns structured, advisory information rather
/// than an unqualified deletion recommendation: a name match cannot establish that deleting
/// the entry is safe.
pub fn classify_cleanup_candidate(name: &str, is_dir: bool) -> Option<CleanupClassification> {
    let lower = name.to_ascii_lowercase();
    if is_dir {
        match lower.as_str() {
            "__pycache__" | ".cache" | "code cache" | "gpucache" | "shadercache" => {
                Some(CleanupClassification {
                    category: CleanupCategory::BrowserCache,
                    reason: "A narrowly recognized application cache that is normally regenerated.",
                    confidence: CleanupConfidence::High,
                })
            }
            "node_modules" => Some(CleanupClassification {
                category: CleanupCategory::DependencyCache,
                reason:
                    "A package dependency directory that can be restored by its package manager.",
                confidence: CleanupConfidence::Medium,
            }),
            ".gradle" | ".next" | ".nuxt" => Some(CleanupClassification {
                category: if lower == ".gradle" {
                    CleanupCategory::DependencyCache
                } else {
                    CleanupCategory::BuildOutput
                },
                reason: "A tool-managed cache or generated directory that can usually be rebuilt.",
                confidence: CleanupConfidence::Medium,
            }),
            "target" | "build" | "dist" | "out" => Some(CleanupClassification {
                category: CleanupCategory::BuildOutput,
                reason:
                    "A generic build/output name; inspect its contents and project usage first.",
                confidence: CleanupConfidence::ContextDependent,
            }),
            "cache" => Some(CleanupClassification {
                category: CleanupCategory::BrowserCache,
                reason: "A generic cache name whose purpose depends on the parent application.",
                confidence: CleanupConfidence::ContextDependent,
            }),
            _ => None,
        }
    } else {
        match extension_of(name).as_str() {
            "msi" => Some(CleanupClassification {
                category: CleanupCategory::Installer,
                reason: "An installer package that may still be needed for repair or reinstall.",
                confidence: CleanupConfidence::ContextDependent,
            }),
            "exe" if lower.contains("setup") || lower.contains("install") => {
                Some(CleanupClassification {
                    category: CleanupCategory::Installer,
                    reason: "An installer-like executable whose purpose should be checked first.",
                    confidence: CleanupConfidence::ContextDependent,
                })
            }
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
        assert_eq!(
            names[0..2].iter().collect::<std::collections::HashSet<_>>(),
            ["big", "huge.bin"].iter().collect()
        );
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
            (0..3)
                .map(|i| file(&format!("v{i}.mp4"), 500_000_000))
                .collect(),
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
            (0..120)
                .map(|i| file(&format!("a{i}.bin"), 10 * 1024 * 1024))
                .collect(),
        );
        let tree = dir("root", vec![big]);
        assert!(InsightNode::from_entry(&tree).blizzard_flags().is_empty());
    }

    #[test]
    fn cleanup_candidates_return_structured_matches_and_skip_unrelated() {
        let tree = dir(
            "root",
            vec![
                dir("node_modules", vec![file("index.js", 100)]),
                dir("target", vec![file("app", 5000)]),
                dir(".cache", vec![file("index", 4000)]),
                dir("src", vec![file("main.rs", 200)]),
                file("setup_v2.exe", 9000),
                file("game.msi", 8000),
                file("photo.jpg", 300),
            ],
        );
        let candidates = InsightNode::from_entry(&tree).cleanup_candidates();
        let names: Vec<&str> = candidates.iter().map(|entry| entry.name.as_str()).collect();

        assert_eq!(
            names,
            vec![
                "setup_v2.exe",
                "game.msi",
                "target",
                ".cache",
                "node_modules"
            ]
        );
        assert!(!candidates
            .iter()
            .any(|entry| entry.name == "src" || entry.name == "photo.jpg"));

        let node_modules = candidates
            .iter()
            .find(|entry| entry.name == "node_modules")
            .unwrap();
        assert_eq!(
            node_modules.classification,
            CleanupClassification {
                category: CleanupCategory::DependencyCache,
                reason:
                    "A package dependency directory that can be restored by its package manager.",
                confidence: CleanupConfidence::Medium,
            }
        );
        assert_eq!(
            node_modules.classification.confidence.label(),
            "Medium confidence"
        );
    }

    #[test]
    fn cleanup_classifier_covers_high_medium_and_context_dependent_matches() {
        let high = classify_cleanup_candidate(".CACHE", true).unwrap();
        assert_eq!(high.category, CleanupCategory::BrowserCache);
        assert_eq!(high.confidence, CleanupConfidence::High);

        let medium = classify_cleanup_candidate("NODE_MODULES", true).unwrap();
        assert_eq!(medium.category, CleanupCategory::DependencyCache);
        assert_eq!(medium.confidence, CleanupConfidence::Medium);

        for (name, is_dir, category) in [
            ("build", true, CleanupCategory::BuildOutput),
            ("dist", true, CleanupCategory::BuildOutput),
            ("out", true, CleanupCategory::BuildOutput),
            ("package.msi", false, CleanupCategory::Installer),
            ("setup.exe", false, CleanupCategory::Installer),
        ] {
            let classification = classify_cleanup_candidate(name, is_dir).unwrap();
            assert_eq!(
                classification.category, category,
                "classification for {name}"
            );
            assert_eq!(
                classification.confidence,
                CleanupConfidence::ContextDependent,
                "confidence for {name}"
            );
            assert!(
                !classification.reason.to_ascii_lowercase().contains("safe"),
                "reason must remain advisory for {name}"
            );
        }
    }

    #[test]
    fn cleanup_classifier_is_case_insensitive_and_rejects_non_matches() {
        assert!(classify_cleanup_candidate("Code Cache", true).is_some());
        assert!(classify_cleanup_candidate("INSTALLER.EXE", false).is_some());
        assert!(classify_cleanup_candidate("src", true).is_none());
        assert!(classify_cleanup_candidate("launch.exe", false).is_none());
        assert!(classify_cleanup_candidate("photo.jpg", false).is_none());
    }

    #[test]
    fn cleanup_candidates_do_not_descend_into_matched_directories() {
        // A node_modules holding a nested node_modules should surface only
        // the outer one, because the outer candidate represents its subtree.
        let tree = dir(
            "root",
            vec![dir(
                "node_modules",
                vec![dir("node_modules", vec![file("x.js", 10)])],
            )],
        );
        let candidates = InsightNode::from_entry(&tree).cleanup_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].trail, vec!["node_modules".to_string()]);
    }
}
