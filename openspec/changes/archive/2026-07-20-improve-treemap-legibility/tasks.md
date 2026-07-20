## 1. Theming: proportional label-darken color rule

- [x] 1.1 Add a new `theme.rs` color rule for on-block label text: an
      alpha-blended black overlay (not a fixed gray constant) that darkens
      whatever color/lightness sits beneath it.
- [x] 1.2 Add a `theme.rs` unit test asserting the new label color darkens a
      representative file-band color and a directory-band color (mirroring
      the existing `gradient_stops_bracket_the_base_in_lightness`-style
      assertions already in that test module).

## 2. Block labels: size label + fit gating

- [x] 2.1 Add a size-label fit-gate helper distinct from the existing
      `label_fits` (`app.rs:1780`): measure the formatted size string's
      rendered width via the same galley-measurement pattern the chrome
      toggle buttons already use (`app.rs:1978`-`2077`), and require enough
      spare width beyond a reserved name column before it passes.
- [x] 2.2 Wire that gate into `draw_children`'s flat branch: paint the
      block's `util::format_size(child.size)` in the top-right corner when
      the gate passes, using the new label color from 1.1.
- [x] 2.3 Add a directory-tray-specific version of the gate that measures
      the tray header's actual rendered label width (including a collapsed
      chain's joined name from `collapse_chain`, `app.rs:1702`) rather than
      reusing the file-card gate.
- [x] 2.4 Wire that tray gate into `draw_tray_shell` (`app.rs:1861`): paint
      the tray's total size in the header's top-right corner when it
      passes, using the new label color from 1.1.
- [x] 2.5 Apply the new label color from 1.1 to the two existing name-label
      paint calls (`draw_children`'s flat-branch name, `draw_tray_shell`'s
      header name), replacing the hardcoded `theme::TEXT`.

## 3. Insights drawer: proportional fill bars

- [x] 3.1 Add a `total_size: u64` field to `InsightsData` (`app.rs:358`),
      set from `view.size` in `refresh_insights` (`app.rs:1124`) alongside
      the existing `ext_totals`/`leaderboard` fields.
- [x] 3.2 Add a new flat neutral (green) bar-fill color constant to
      `theme.rs`.
- [x] 3.3 Add a row bar-paint helper alongside `swatch()` (`app.rs:1597`):
      reserve the row's `Rect` (`ui.allocate_exact_size`, mirroring
      `swatch()`'s own pattern) and paint a proportional-width fill behind
      the row's existing content using the color from 3.2.
- [x] 3.4 Wire the bar helper into the "File types" section's rows
      (`app.rs:1170`-`1183`), sizing each bar to
      `ext_size as f64 / total_size as f64`.
- [x] 3.5 Wire the bar helper into the "Biggest items" leaderboard's rows
      (`app.rs:1194`-`1205`), sizing each bar to
      `entry.size as f64 / total_size as f64`.

## 4. Scan responsiveness: paced event drain + resumable tree assembly

- [x] 4.1 Add a per-frame wall-clock time-budget constant to `app.rs`,
      shared by the event-drain loop and the resumable assembly (or two
      separate constants — decide at implementation time per the design's
      Open Questions).
- [x] 4.2 Change `drain_scan`'s event-processing loop (`app.rs:557`) to
      stop once elapsed time since the loop started exceeds the budget,
      leaving the remainder of `scan.events`'s backlog queued in the
      channel for the next frame's call rather than draining it all via
      unconditional `try_iter()`.
- [x] 4.3 Restructure `Node::from_entry` (`app.rs:134`) from a plain
      recursion into an explicit, resumable worklist traversal (a
      stack/queue of pending `(Entry, target-parent)` items) that can
      process a bounded slice per call and be resumed on the next call
      from the same in-progress state.
- [x] 4.4 Add new state (on `ActiveScan` or `BytewhifferApp`) to hold an
      in-progress resumable assembly across frames, replacing the direct
      `Node::from_entry` call at scan completion (`app.rs:612`-`621`) with
      "start (or continue) the resumable assembly this frame, budget-
      limited."
- [x] 4.5 Keep displaying and allow interaction with the existing live
      `self.root` tree while the resumable assembly runs in the
      background; swap `self.root` for the fully-assembled authoritative
      tree only once assembly completes, in a single atomic replace —
      never show a partially-assembled authoritative tree.
- [x] 4.6 Add a new unit test: assemble a synthetic large `Entry` tree via
      the resumable traversal across multiple simulated "frames" (a small
      per-call budget/step count) and assert the result matches a
      one-shot reference build on the same input (same sizes, same
      structure, same child counts).

## 5. Verification

- [x] 5.1 Run `cargo test` — confirm the new `theme.rs` test, the new
      resumable-assembly test, and all existing scanner/treemap/theme/util
      tests still pass headlessly.
- [x] 5.2 Run `cargo check --target x86_64-pc-windows-gnu` — confirm the
      Windows-only surface still type-checks (this change shouldn't touch
      any `#[cfg(windows)]` code, but verify).
- [x] 5.3 Capture `--debug-screenshot`, `--debug-screenshot-live`, and
      `--debug-screenshot-drill` against a real, varied tree (a dense
      small-file cluster, a deep single-child chain, a large scan) and
      visually confirm: size labels appear/disappear at sensible
      thresholds without clipping or colliding with name labels, label
      text reads clearly against multiple block hues, and the Insights
      bars render and rescale correctly when drilling in and out.
- [x] 5.4 On real Windows hardware, scan a large/dense real directory
      (e.g. a deep `node_modules` tree or a full user profile) before and
      after this change and confirm the mid-scan and post-scan stutters
      are gone or substantially shortened.
