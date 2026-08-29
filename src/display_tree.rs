//! GUI-independent owned tree used by the UI and its background preparation.
//!
//! `Entry` is the scanner's authoritative tree. `DisplayNode` is the mutable
//! UI-side representation: it owns the same data, keeps a name index for fast
//! live insertion/navigation, and maintains the metadata that rendering caches
//! need without rescanning a subtree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::scanner::{Entry, ScanProgress};

/// An owned tree node prepared for display.
///
/// `descendant_count` excludes this node and is maintained by mutation rather
/// than derived with a recursive walk. `structural_rev` changes whenever this
/// node's accounted size or child structure changes; a cache keyed by that
/// value can therefore reuse work across pointer-only frames.
#[derive(Debug, Clone)]
pub(crate) struct DisplayNode {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) is_dir: bool,
    pub(crate) children: Vec<DisplayNode>,
    pub(crate) child_index: HashMap<String, usize>,
    descendant_count: usize,
    structural_rev: u64,
}

impl DisplayNode {
    pub(crate) fn new(name: String, path: PathBuf, size: u64, is_dir: bool) -> Self {
        Self {
            name,
            path,
            size,
            is_dir,
            children: Vec::new(),
            child_index: HashMap::new(),
            descendant_count: 0,
            structural_rev: 1,
        }
    }

    /// Builds a display tree from a borrowed authoritative tree.
    ///
    /// This compatibility entry point owns a clone so callers that need a
    /// synchronous reference build (tests and the performance harness) retain
    /// the old API. Completion paths should use [`from_owned_entry`] so the
    /// authoritative tree can be moved instead of cloned.
    pub(crate) fn from_entry(entry: &Entry) -> Self {
        Self::from_owned_entry(entry.clone())
    }

    /// Converts an authoritative tree without cloning paths or repeatedly
    /// walking from the root to find a parent. The explicit stack keeps the
    /// conversion safe for deeply nested filesystem trees while each finished
    /// child is moved directly into its already-created parent.
    pub(crate) fn from_owned_entry(entry: Entry) -> Self {
        let progress = ScanProgress::default();
        let cancel = AtomicBool::new(false);
        Self::from_owned_entry_with_progress(entry, &progress, &cancel)
            .expect("non-cancellable display-tree conversion must complete")
    }

    /// Converts an authoritative tree while publishing preparation progress.
    /// The node count is computed iteratively on the worker so the published
    /// fraction has a fixed denominator; conversion itself never walks from
    /// the root to locate a parent. Cancellation is checked during both the
    /// count and conversion passes, and a cancelled conversion returns no
    /// partial display tree.
    pub(crate) fn from_owned_entry_with_progress(
        entry: Entry,
        progress: &ScanProgress,
        cancel: &AtomicBool,
    ) -> Option<Self> {
        struct Frame {
            node: DisplayNode,
            children: std::vec::IntoIter<Entry>,
        }

        fn count_entries(root: &Entry, cancel: &AtomicBool) -> Option<u64> {
            let mut pending = vec![root];
            let mut total = 0u64;
            while let Some(entry) = pending.pop() {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                total = total.saturating_add(1);
                pending.extend(entry.children.iter());
            }
            Some(total)
        }

        fn frame(entry: Entry) -> Frame {
            let Entry {
                name,
                path,
                size,
                is_dir,
                children,
            } = entry;
            Frame {
                node: DisplayNode::new(name, path, size, is_dir),
                children: children.into_iter(),
            }
        }

        let total = count_entries(&entry, cancel)?;
        progress.start_conversion(total);
        let mut stack = vec![frame(entry)];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }

            let next = stack
                .last_mut()
                .expect("conversion stack always contains a frame")
                .children
                .next();
            if let Some(child) = next {
                stack.push(frame(child));
                continue;
            }

            let mut finished = stack
                .pop()
                .expect("conversion stack always contains a frame")
                .node;
            // Descendants have already been finalized before this frame is
            // popped, so only this node's direct child vector needs sorting.
            finished.sort_children();
            progress.conversion_node_finished();
            let Some(parent) = stack.last_mut() else {
                progress.finish_conversion();
                return Some(finished);
            };
            parent.node.append_child_unsorted(finished);
        }
    }

    /// Number of descendants below this node, excluding the node itself.
    pub(crate) fn descendant_count(&self) -> usize {
        self.descendant_count
    }

    /// Revision for caches whose result depends on this node's child sizes or
    /// structure. Descendant revisions are intentionally independent; a
    /// mutation bubbles a revision only through its ancestor chain.
    pub(crate) fn structural_rev(&self) -> u64 {
        self.structural_rev
    }

    /// Inserts a discovery path relative to this node.
    ///
    /// Newly-created nodes and size changes update descendant counts and
    /// revisions on the affected ancestor chain. Children are sorted at the
    /// mutation boundary, so rendering never needs to sort an unchanged node.
    pub(crate) fn insert(&mut self, rel: &Path, size: u64, is_dir: bool) {
        let components: Vec<String> = rel
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect();
        if components.is_empty() {
            return;
        }
        self.insert_components(&components, size, is_dir);
    }

    fn insert_components(&mut self, components: &[String], size: u64, is_dir: bool) -> usize {
        self.size = self.size.saturating_add(size);
        self.bump_revision();

        let name = &components[0];
        if components.len() == 1 {
            if let Some(&index) = self.child_index.get(name) {
                let child = &mut self.children[index];
                child.size = child.size.saturating_add(size);
                child.bump_revision();
                self.reposition_child(index);
                return 0;
            }

            let path = self.path.join(name);
            let index = self.children.len();
            self.children
                .push(DisplayNode::new(name.clone(), path, size, is_dir));
            self.descendant_count += 1;
            self.reposition_child(index);
            return 1;
        }

        let (index, created) = match self.child_index.get(name).copied() {
            Some(index) => (index, false),
            None => {
                let index = self.children.len();
                self.children.push(DisplayNode::new(
                    name.clone(),
                    self.path.join(name),
                    0,
                    true,
                ));
                (index, true)
            }
        };

        let added_below = self.children[index].insert_components(&components[1..], size, is_dir);
        self.descendant_count += added_below + usize::from(created);
        self.reposition_child(index);
        added_below + usize::from(created)
    }

    /// Removes the node at `names`, which is relative to this node.
    ///
    /// Returns `false` when the path is absent. The removed subtree's size and
    /// node count are subtracted from every surviving ancestor without raw
    /// pointers or a root re-walk.
    pub(crate) fn remove(&mut self, names: &[String]) -> bool {
        !names.is_empty() && self.remove_components(names).is_some()
    }

    fn remove_components(&mut self, names: &[String]) -> Option<(u64, usize)> {
        let name = names.first()?;
        let index = *self.child_index.get(name)?;

        if names.len() == 1 {
            let removed = self.children.remove(index);
            let removed_nodes = 1 + removed.descendant_count;
            self.size = self.size.saturating_sub(removed.size);
            self.descendant_count = self.descendant_count.saturating_sub(removed_nodes);
            self.bump_revision();
            self.rebuild_child_index();
            return Some((removed.size, removed_nodes));
        }

        let result = self.children[index].remove_components(&names[1..])?;
        self.size = self.size.saturating_sub(result.0);
        self.descendant_count = self.descendant_count.saturating_sub(result.1);
        self.bump_revision();
        self.reposition_child(index);
        Some(result)
    }

    pub(crate) fn find(&self, names: &[String]) -> Option<&DisplayNode> {
        let mut node = self;
        for name in names {
            node = &node.children[*node.child_index.get(name)?];
        }
        Some(node)
    }

    /// Appends a child without reordering the vector. The owned conversion
    /// keeps stable child insertion indices while it drains its explicit
    /// stack, then sorts each parent when that parent is finalized.
    pub(crate) fn append_child_unsorted(&mut self, child: DisplayNode) {
        let index = self.children.len();
        self.descendant_count += 1 + child.descendant_count;
        self.child_index.insert(child.name.clone(), index);
        self.children.push(child);
        self.bump_revision();
    }

    fn bump_revision(&mut self) {
        self.structural_rev = self.structural_rev.wrapping_add(1);
    }

    fn compare_children(a: &DisplayNode, b: &DisplayNode) -> std::cmp::Ordering {
        b.size
            .cmp(&a.size)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
    }

    fn sort_children(&mut self) {
        self.children.sort_unstable_by(Self::compare_children);
        self.rebuild_child_index();
    }

    /// Restores the sorted position of one child after its size or identity
    /// changes. Repositioning avoids sorting an entire wide sibling vector for
    /// every live discovery event while preserving deterministic order.
    fn reposition_child(&mut self, index: usize) {
        let child = self.children.remove(index);
        let target = self
            .children
            .binary_search_by(|existing| Self::compare_children(existing, &child))
            .unwrap_or_else(|position| position);
        self.children.insert(target, child);
        self.rebuild_child_index();
    }

    fn rebuild_child_index(&mut self) {
        self.child_index.clear();
        for (index, child) in self.children.iter().enumerate() {
            self.child_index.insert(child.name.clone(), index);
        }
    }
}

impl crate::insights::InsightTree for DisplayNode {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: impl Into<String>, path: impl Into<PathBuf>, size: u64) -> Entry {
        Entry {
            name: name.into(),
            path: path.into(),
            size,
            is_dir: false,
            children: Vec::new(),
        }
    }

    fn dir(name: impl Into<String>, path: impl Into<PathBuf>, children: Vec<Entry>) -> Entry {
        let size = children.iter().map(|child| child.size).sum();
        Entry {
            name: name.into(),
            path: path.into(),
            size,
            is_dir: true,
            children,
        }
    }

    fn equivalent(a: &DisplayNode, b: &DisplayNode) -> bool {
        a.name == b.name
            && a.path == b.path
            && a.size == b.size
            && a.is_dir == b.is_dir
            && a.descendant_count == b.descendant_count
            && a.children.len() == b.children.len()
            && a.children.iter().all(|child| {
                b.child_index
                    .get(&child.name)
                    .is_some_and(|&index| equivalent(child, &b.children[index]))
            })
    }

    fn synthetic_tree(width: usize, depth: usize, prefix: &str) -> Entry {
        if depth == 0 {
            return file(
                format!("{}.bin", prefix),
                PathBuf::from(prefix),
                (prefix.len() as u64 + 1) * 17,
            );
        }
        let children: Vec<Entry> = (0..width)
            .map(|index| synthetic_tree(width, depth - 1, &format!("{}_{}", prefix, index)))
            .collect();
        dir(prefix, PathBuf::from(prefix), children)
    }

    #[test]
    fn owned_conversion_matches_borrowed_reference_for_wide_deep_tree() {
        let entry = synthetic_tree(7, 5, "root");
        let borrowed = DisplayNode::from_entry(&entry);
        let owned = DisplayNode::from_owned_entry(entry);

        assert!(equivalent(&owned, &borrowed));
        assert_eq!(owned.descendant_count(), 19_607);
        assert!(owned.structural_rev() > 1);
    }

    #[test]
    fn owned_conversion_reports_exact_progress_for_a_wide_tree() {
        let entry = synthetic_tree(3, 2, "root");
        let progress = ScanProgress::default();
        let cancel = AtomicBool::new(false);

        let converted = DisplayNode::from_owned_entry_with_progress(entry, &progress, &cancel);

        assert!(converted.is_some());
        assert!(progress.conversion_started());
        assert!(progress.conversion_complete());
        assert_eq!(progress.conversion_counts(), (13, 13));
        assert_eq!(progress.conversion_progress(), 1.0);
    }

    #[test]
    fn insertion_maintains_counts_revisions_and_deterministic_order() {
        let mut root = DisplayNode::new("root".to_owned(), PathBuf::from("root"), 0, true);
        let initial_revision = root.structural_rev();

        root.insert(Path::new("z/leaf.bin"), 10, false);
        root.insert(Path::new("a/leaf.bin"), 30, false);
        root.insert(Path::new("z/second.bin"), 20, false);

        assert_eq!(root.size, 60);
        assert_eq!(root.descendant_count(), 5);
        assert_eq!(root.children[0].name, "a");
        assert_eq!(root.children[1].name, "z");
        assert_eq!(root.children[1].children[0].name, "second.bin");
        assert!(root.structural_rev() > initial_revision);

        let z_revision = root.find(&["z".to_owned()]).unwrap().structural_rev();
        root.insert(Path::new("z/third.bin"), 1, false);
        assert!(root.find(&["z".to_owned()]).unwrap().structural_rev() > z_revision);
        assert_eq!(root.descendant_count(), 6);

        // A later discovery can change a directory's accumulated size enough
        // to cross a sibling. The affected child is repositioned without
        // sorting the entire wide sibling vector.
        root.insert(Path::new("z/huge.bin"), 100, false);
        assert_eq!(root.children[0].name, "z");
        assert_eq!(root.children[1].name, "a");
        assert_eq!(root.descendant_count(), 7);
    }

    #[test]
    fn removal_updates_ancestors_without_touching_unrelated_branch() {
        let mut root = DisplayNode::new("root".to_owned(), PathBuf::from("root"), 0, true);
        root.insert(Path::new("a/deep/file.bin"), 100, false);
        root.insert(Path::new("b/other.bin"), 25, false);
        let b_revision = root.find(&["b".to_owned()]).unwrap().structural_rev();

        assert!(root.remove(&["a".to_owned(), "deep".to_owned(), "file.bin".to_owned()]));
        assert_eq!(root.size, 25);
        assert_eq!(root.descendant_count(), 4);
        assert_eq!(root.children[0].name, "b");
        assert!(root.find(&["a".to_owned(), "deep".to_owned()]).is_some());
        assert_eq!(
            root.find(&["b".to_owned()]).unwrap().structural_rev(),
            b_revision
        );
        assert!(!root.remove(&["missing".to_owned()]));
    }
}
