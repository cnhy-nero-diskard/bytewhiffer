## 1. Pure insights module (`src/insights.rs`, no `egui` dependency)

- [ ] 1.1 Define an `InsightNode`-style minimal view (name, size, is_dir, children) that `app::Node` and `scanner::Entry` can both be converted/borrowed into, so aggregation functions don't depend on either concrete tree type directly (mirrors how `treemap::squarify` stays independent of both)
- [ ] 1.2 Implement `extension_totals(&self) -> Vec<(String, u64)>`: total size per distinct extension across all files in a subtree, sorted largest first (drives both the legend and the breakdown)
- [ ] 1.3 Implement `leaderboard(&self, n: usize) -> Vec<LeaderboardEntry>`: the N largest files/folders by size in a subtree, each carrying its path/trail for focus navigation
- [ ] 1.4 Implement `blizzard_flags(&self) -> Vec<BlizzardEntry>`: directories in a subtree with high child count and low average child size, per the design's threshold approach
- [ ] 1.5 Implement `junk_suggestions(&self) -> Vec<JunkEntry>`: files/directories in a subtree matching a fixed set of known junk name patterns (installers, build caches, `node_modules`, browser cache dirs)
- [ ] 1.6 Unit tests for each function using small synthetic trees: extension totals sum correctly and sort largest-first, leaderboard ranks correctly and returns paths, blizzard flags catch the many-small-children case and skip normal directories, junk suggestions match known patterns and skip unrelated names

## 2. Drawer toggle and panel layout (`src/app.rs`)

- [ ] 2.1 Add `insights_open: bool` (default `false`) to `BytewhifferApp`
- [ ] 2.2 Add a toolbar toggle button (styled via the existing `chrome_button` helper) that flips `insights_open`
- [ ] 2.3 Add an `egui::SidePanel::left` shown only when `insights_open`, sized within a fixed min/max width range, rendered alongside the existing top/bottom panels and central treemap panel
- [ ] 2.4 Verify via `--debug-screenshot` that opening the drawer narrows the treemap's available width rather than overlapping it, and that toolbar/breadcrumb layout stays intact with the drawer open

## 3. Recompute-on-change wiring

- [ ] 3.1 Track the last (focus path, tree revision) the drawer computed insights for; recompute only when either changes, per the design's "not per frame" decision
- [ ] 3.2 Wire recomputation to read from the currently focused `Node` (same node `treemap_panel` resolves via `root.find(&self.focus)`), so insights always describe what the treemap is currently showing
- [ ] 3.3 Confirm insights refresh when a live scan streams in new entries and when a scan completes, without requiring the drawer to be closed and reopened

## 4. Drawer content: legend and breakdown

- [ ] 4.1 Render the extension legend: one row per distinct extension in the focused subtree, swatch colored via `theme::color_for_extension`, extension label
- [ ] 4.2 Render the extensionless-files legend entry using the same fallback-color mechanism `theme::color_for_extension("")` already produces
- [ ] 4.3 Render the top-extensions-by-size breakdown using `insights::extension_totals`, each entry colored to match its legend swatch, sorted largest first

## 5. Drawer content: leaderboard

- [ ] 5.1 Render the leaderboard list using `insights::leaderboard`, showing name and size per entry
- [ ] 5.2 Wire entry activation to set `self.focus` to the entry's trail, the same navigation path breadcrumb/click-to-drill already use

## 6. Drawer content: small-file-blizzard flag

- [ ] 6.1 Render the blizzard-flag list using `insights::blizzard_flags`, showing directory name, child count, and average child size
- [ ] 6.2 Wire entry activation to focus the treemap on that directory, consistent with the leaderboard's navigation behavior

## 7. Drawer content: known-junk suggestions

- [ ] 7.1 Render the junk-suggestions list using `insights::junk_suggestions`, showing name and matched pattern category
- [ ] 7.2 Wire each entry to reuse the existing context-menu actions (Delete / Open / Reveal in Explorer) — set `self.context_target` the same way a right-click on a treemap block does, rather than building new action UI
- [ ] 7.3 Confirm no entry is deleted or modified by merely appearing in this list — flags are advisory only until the user explicitly activates an action

## 8. Empty/placeholder states

- [ ] 8.1 Show a neutral placeholder in the drawer when no scan has ever completed (mirrors `treemap_panel`'s "Pick a folder…" placeholder)
- [ ] 8.2 Show neutral empty states per section when a focused subtree has nothing to report for it (e.g. no junk matches) rather than an empty gap

## 9. Verification

- [ ] 9.1 Run `cargo test` and confirm the new `insights` unit tests pass alongside existing scanner/treemap/theme/util suites
- [ ] 9.2 Manually verify via `--debug-screenshot` that the legend's swatch colors visually match the treemap's block colors for the same extensions
- [ ] 9.3 Manually verify via `--debug-screenshot-drill` that drilling into a directory updates all drawer sections to describe that subtree
- [ ] 9.4 Manually verify leaderboard and blizzard-flag entries correctly navigate the treemap focus when activated
- [ ] 9.5 Manually verify a junk-suggestion entry's Delete/Open/Reveal actions behave identically to the same actions from a treemap block's context menu
