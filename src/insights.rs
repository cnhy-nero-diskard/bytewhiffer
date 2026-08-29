//! Pure, egui-free derived analytics over a scanned tree.
//!
//! The drawer's four sections are produced by one visitor over the focused
//! tree.  The visitor borrows the source tree through [`InsightTree`], so it
//! does not construct a second tree-shaped representation merely to aggregate
//! it.  Leaderboard state is capped at the requested top-N size while the
//! traversal is in progress.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};

/// How many direct children a directory must have before it can be
/// considered a small-file blizzard.
const BLIZZARD_MIN_CHILDREN: usize = 100;
/// The largest average child size (bytes) a blizzard directory may have.
const BLIZZARD_MAX_AVG_SIZE: u64 = 64 * 1024;

/// Minimal tree interface needed by the one-pass analytics visitor.
///
/// Returning a slice keeps the visitor borrowed and allocation-free with
/// respect to tree shape. `Entry` implements this here; the UI's
/// `DisplayNode` implementation lives in `display_tree.rs` because that type
/// is owned by the UI-side preparation module.
pub(crate) trait InsightTree: Sized {
    fn insight_name(&self) -> &str;
    fn insight_path(&self) -> &Path;
    fn insight_size(&self) -> u64;
    fn insight_is_dir(&self) -> bool;
    fn insight_children(&self) -> &[Self];
}

impl InsightTree for crate::scanner::Entry {
    fn insight_name(&self) -> &str {
        &self.name
    }

    fn insight_path(&self) -> &Path {
        &self.path
    }

    fn insight_size(&self) -> u64 {
        self.size
    }

    fn insight_is_dir(&self) -> bool {
        self.is_dir
    }

    fn insight_children(&self) -> &[Self] {
        &self.children
    }
}

/// A single ranked entry in the biggest-files/folders leaderboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LeaderboardEntry {
    pub(crate) name: String,
    /// Names from the focus node down to this entry (inclusive), the same
    /// relative trail `app.rs` appends to `self.focus` to navigate.
    pub(crate) trail: Vec<String>,
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) is_dir: bool,
}

/// A directory flagged as a small-file blizzard: many children, low average
/// child size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlizzardEntry {
    pub(crate) name: String,
    pub(crate) trail: Vec<String>,
    pub(crate) child_count: usize,
    pub(crate) avg_child_size: u64,
}

/// A file or directory whose name matches a known-junk pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JunkEntry {
    pub(crate) name: String,
    pub(crate) trail: Vec<String>,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
    /// Human-readable category of the matched pattern, e.g. "node_modules".
    pub(crate) category: &'static str,
}

/// All drawer analytics produced by one traversal of a focused subtree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct InsightSummary {
    pub(crate) ext_totals: Vec<(String, u64)>,
    pub(crate) leaderboard: Vec<LeaderboardEntry>,
    pub(crate) blizzard: Vec<BlizzardEntry>,
    pub(crate) junk: Vec<JunkEntry>,
    pub(crate) total_size: u64,
}

struct Aggregator {
    ext_totals: HashMap<String, u64>,
    leaderboard: BinaryHeap<LeaderboardEntry>,
    blizzard: Vec<BlizzardEntry>,
    junk: Vec<JunkEntry>,
    leaderboard_limit: usize,
}

/// Computes every Insights section in one borrowed traversal.
///
/// The leaderboard heap never grows beyond `leaderboard_limit`; entries are
/// cloned only when they can displace the current worst retained candidate.
/// Other sections intentionally retain their complete result sets because the
/// drawer displays all matching extension/blizzard/junk categories.
pub(crate) fn aggregate<T: InsightTree>(root: &T, leaderboard_limit: usize) -> InsightSummary {
    let mut aggregation = Aggregator {
        ext_totals: HashMap::new(),
        leaderboard: BinaryHeap::with_capacity(leaderboard_limit),
        blizzard: Vec::new(),
        junk: Vec::new(),
        leaderboard_limit,
    };
    let mut trail = Vec::new();
    visit(root, &mut trail, true, true, &mut aggregation);

    let mut ext_totals: Vec<(String, u64)> = aggregation.ext_totals.into_iter().collect();
    ext_totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut leaderboard = aggregation.leaderboard.into_vec();
    leaderboard.sort_by(compare_entries);
    aggregation.blizzard.sort_by(|a, b| {
        b.child_count
            .cmp(&a.child_count)
            .then_with(|| a.trail.cmp(&b.trail))
    });
    aggregation.junk.sort_by(|a, b| {
        b.size
            .cmp(&a.size)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.trail.cmp(&b.trail))
    });

    InsightSummary {
        ext_totals,
        leaderboard,
        blizzard: aggregation.blizzard,
        junk: aggregation.junk,
        total_size: root.insight_size(),
    }
}

/// Visits the entire tree once. `junk_allowed` is separate from the other
/// analytics: a matched junk directory suppresses nested junk suggestions,
/// but its descendants still contribute to totals, ranking, and blizzard
/// detection.
fn visit<'a, T: InsightTree>(
    node: &'a T,
    trail: &mut Vec<&'a str>,
    is_root: bool,
    junk_allowed: bool,
    aggregation: &mut Aggregator,
) {
    if !is_root {
        retain_leaderboard(node, trail, aggregation);
    }

    if node.insight_is_dir() {
        let child_count = node.insight_children().len();
        if !is_root && child_count >= BLIZZARD_MIN_CHILDREN {
            let avg_child_size = node.insight_size() / child_count as u64;
            if avg_child_size <= BLIZZARD_MAX_AVG_SIZE {
                aggregation.blizzard.push(BlizzardEntry {
                    name: node.insight_name().to_owned(),
                    trail: owned_trail(trail),
                    child_count,
                    avg_child_size,
                });
            }
        }

        for child in node.insight_children() {
            trail.push(child.insight_name());
            let matched_junk = if junk_allowed {
                junk_category(child.insight_name(), child.insight_is_dir())
            } else {
                None
            };
            if let Some(category) = matched_junk {
                aggregation.junk.push(JunkEntry {
                    name: child.insight_name().to_owned(),
                    trail: owned_trail(trail),
                    path: child.insight_path().to_path_buf(),
                    is_dir: child.insight_is_dir(),
                    size: child.insight_size(),
                    category,
                });
            }
            visit(
                child,
                trail,
                false,
                junk_allowed && !(matched_junk.is_some() && child.insight_is_dir()),
                aggregation,
            );
            trail.pop();
        }
    } else {
        *aggregation
            .ext_totals
            .entry(extension_of(node.insight_name()))
            .or_insert(0) += node.insight_size();
    }
}

fn owned_trail(trail: &[&str]) -> Vec<String> {
    trail.iter().map(|part| (*part).to_owned()).collect()
}

fn retain_leaderboard<T: InsightTree>(node: &T, trail: &[&str], aggregation: &mut Aggregator) {
    if aggregation.leaderboard_limit == 0 {
        return;
    }
    if aggregation.leaderboard.len() >= aggregation.leaderboard_limit {
        let worst = aggregation
            .leaderboard
            .peek()
            .expect("a full leaderboard has a worst candidate");
        if compare_candidate(node, trail, worst) != Ordering::Less {
            return;
        }
    }

    aggregation.leaderboard.push(LeaderboardEntry {
        name: node.insight_name().to_owned(),
        trail: owned_trail(trail),
        path: node.insight_path().to_path_buf(),
        size: node.insight_size(),
        is_dir: node.insight_is_dir(),
    });
    if aggregation.leaderboard.len() > aggregation.leaderboard_limit {
        aggregation.leaderboard.pop();
    }
}

/// Ordering used by both retained candidates and final output: largest size
/// first, then path/name/trail for deterministic ties. Because `BinaryHeap`
/// keeps its greatest item at the root, this ordering makes the root the
/// *worst* retained candidate: smaller entries and later tie-break values
/// compare greater than better entries.
fn compare_entries(a: &LeaderboardEntry, b: &LeaderboardEntry) -> Ordering {
    b.size
        .cmp(&a.size)
        .then_with(|| a.path.cmp(&b.path))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.trail.cmp(&b.trail))
}

impl Ord for LeaderboardEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_entries(self, other)
    }
}

impl PartialOrd for LeaderboardEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_candidate<T: InsightTree>(
    candidate: &T,
    trail: &[&str],
    existing: &LeaderboardEntry,
) -> Ordering {
    existing
        .size
        .cmp(&candidate.insight_size())
        .then_with(|| candidate.insight_path().cmp(existing.path.as_path()))
        .then_with(|| candidate.insight_name().cmp(&existing.name))
        .then_with(|| compare_trails(trail, &existing.trail))
}

fn compare_trails(candidate: &[&str], existing: &[String]) -> Ordering {
    for (left, right) in candidate.iter().zip(existing) {
        let ordering = (*left).cmp(right.as_str());
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    candidate.len().cmp(&existing.len())
}

/// The lowercased extension of a file name, or `""` for extensionless files
/// (and dotfiles, whose leading dot is not an extension).
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
    fn one_aggregate_matches_all_drawer_sections() {
        let clutter = dir(
            "node_modules",
            (0..150).map(|i| file(&format!("m{i}.js"), 1024)).collect(),
        );
        let tree = dir(
            "root",
            vec![
                file("a.rs", 100),
                file("b.rs", 50),
                file("c.txt", 30),
                dir("sub", vec![file("d.rs", 10), file("Makefile", 5)]),
                clutter,
                file("setup_v2.exe", 9000),
            ],
        );
        let summary = aggregate(&tree, 15);

        assert_eq!(
            summary.ext_totals,
            vec![
                ("js".to_string(), 150 * 1024),
                ("exe".to_string(), 9000),
                ("rs".to_string(), 160),
                ("txt".to_string(), 30),
                (String::new(), 5),
            ]
        );
        assert_eq!(summary.total_size, tree.size);
        assert_eq!(summary.blizzard.len(), 1);
        assert_eq!(summary.blizzard[0].name, "node_modules");
        assert_eq!(summary.junk.len(), 2);
        assert_eq!(summary.junk[0].name, "node_modules");
        assert_eq!(summary.junk[1].name, "setup_v2.exe");
    }

    #[test]
    fn extension_totals_are_case_insensitive_and_dotfiles_are_extensionless() {
        let tree = dir(
            "root",
            vec![file("a.PNG", 10), file("b.png", 5), file(".env", 2)],
        );
        assert_eq!(
            aggregate(&tree, 0).ext_totals,
            vec![("png".to_string(), 15), (String::new(), 2)]
        );
    }

    #[test]
    fn leaderboard_is_bounded_and_has_deterministic_path_ties() {
        let tree = dir(
            "root",
            (0..100)
                .map(|i| file(&format!("f{i:03}.dat"), 10_000 - i))
                .collect(),
        );
        let board = aggregate(&tree, 3).leaderboard;
        assert_eq!(board.len(), 3);
        assert_eq!(
            board
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["f000.dat", "f001.dat", "f002.dat"]
        );

        let tied = dir(
            "root",
            vec![file("z.bin", 10), file("a.bin", 10), file("m.bin", 10)],
        );
        let board = aggregate(&tied, 3).leaderboard;
        assert_eq!(
            board
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a.bin", "m.bin", "z.bin"]
        );
    }

    #[test]
    fn leaderboard_carries_relative_trails_and_files_focus_their_parent() {
        let tree = dir(
            "root",
            vec![
                dir("big", vec![file("huge.bin", 900)]),
                file("mid.txt", 100),
            ],
        );
        let board = aggregate(&tree, 3).leaderboard;
        let huge = board.iter().find(|entry| entry.name == "huge.bin").unwrap();
        assert_eq!(huge.trail, vec!["big", "huge.bin"]);
        assert!(!huge.is_dir);
    }

    #[test]
    fn blizzard_skips_dirs_with_large_average_children() {
        let big = dir(
            "assets",
            (0..120)
                .map(|i| file(&format!("a{i}.bin"), 10 * 1024 * 1024))
                .collect(),
        );
        assert!(aggregate(&dir("root", vec![big]), 0).blizzard.is_empty());
    }

    #[test]
    fn junk_matches_patterns_and_does_not_duplicate_nested_junk() {
        let tree = dir(
            "root",
            vec![
                dir(
                    "node_modules",
                    vec![dir("node_modules", vec![file("x.js", 10)])],
                ),
                dir("target", vec![file("app", 5000)]),
                file("game.msi", 8000),
                file("photo.jpg", 300),
            ],
        );
        let junk = aggregate(&tree, 0).junk;
        assert_eq!(
            junk.iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["game.msi", "target", "node_modules"]
        );
        assert_eq!(junk[2].trail, vec!["node_modules"]);
    }
}
