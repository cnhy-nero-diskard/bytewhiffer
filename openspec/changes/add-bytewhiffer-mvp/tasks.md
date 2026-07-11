## 1. Project & toolchain setup

- [ ] 1.1 Decide which side(s) (WSL, Windows, or both) get a Rust toolchain via
      `rustup`, install it, and confirm `cargo --version` works there
- [ ] 1.2 Add a `.gitattributes` (e.g. `* text=auto` or `*.rs text eol=lf`) so
      CRLF/LF drift stops recurring between WSL and Windows edits
- [ ] 1.3 Create `Cargo.toml` declaring `eframe`/`egui` (check current stable
      versions rather than trusting the overview doc's snapshot) and `rfd`
- [ ] 1.4 Create the `src/` layout from design.md (`main.rs`, `app.rs`,
      `scanner/mod.rs`, `treemap.rs`, `theme.rs`, `util.rs`)
- [ ] 1.5 Move existing root-level `scanner.rs` content into `src/scanner/`
      and `treemap.rs` into `src/treemap.rs`; confirm `cargo test` passes
      unchanged before any refactor

## 2. Disk-scanning: ScanEngine trait

- [ ] 2.1 Define the `ScanEngine` trait, `Availability` enum (`Available` /
      `RequiresElevation` / `UnsupportedFilesystem` / `NotApplicable`), and
      `ScanError` type in `scanner/mod.rs`
- [ ] 2.2 Decide the final `ScanProgress` shape (reuse the existing two flat
      atomics vs. a richer phase-aware type — design.md's open question) and
      implement it
- [ ] 2.3 Refactor the existing walker `scan()` function into a `WalkerEngine`
      struct implementing `ScanEngine`, returning `Result<Entry, ScanError>`
- [ ] 2.4 Decide sequential vs. parallel (`rayon`/`jwalk`) walker traversal
      (design.md's open question) and implement the chosen approach
- [ ] 2.5 Implement `WalkerEngine::is_available` (should report `Available`
      for any readable target)
- [ ] 2.6 Port existing scanner unit tests to the new trait-based shape; add
      tests for the `Result` split (partial-skip vs. total-failure) and for
      `is_available`

## 3. Treemap layout integration

- [ ] 3.1 Confirm `treemap.rs`'s existing `squarify` unit tests pass unchanged
      after the module move
- [ ] 3.2 Add a small adapter that converts an `Entry`'s children sizes into
      `squarify` input and zips the resulting `Rect`s back to their `Entry`s

## 4. App shell & scan orchestration

- [ ] 4.1 Implement `main.rs` (`eframe::run_native` setup) and `app.rs`'s
      `eframe::App` skeleton
- [ ] 4.2 Wire up the native folder picker (`rfd`) to choose a scan root
- [ ] 4.3 Run the selected `ScanEngine` on a background thread; poll its
      progress from the UI thread and trigger repaints while a scan is in
      flight
- [ ] 4.4 Implement navigation state (current focus path / breadcrumb stack)
      in `app.rs`

## 5. Treemap rendering & interactivity

- [ ] 5.1 Render the current focus node's children as treemap blocks using
      the layout adapter from Task 3.2
- [ ] 5.2 Re-render as new scan progress arrives, so the map visibly fills in
      while scanning is still running
- [ ] 5.3 Implement per-block hit-testing and hover highlighting
- [ ] 5.4 Implement click-to-drill: clicking a folder block updates focus to
      that folder's children; clicking a file block is a no-op for
      navigation
- [ ] 5.5 Implement breadcrumb / back control reflecting the navigation state
      from Task 4.4
- [ ] **[WSLg checkpoint]** 5.6 Interactively verify hit-testing, hover,
      click-to-drill, and breadcrumb navigation under WSLg — this exercises
      the overview doc's named render-interactivity risk (§7) directly

## 6. File actions

- [ ] 6.1 Implement the right-click context menu (Delete / Open / Reveal in
      Explorer) on a treemap block
- [ ] 6.2 Implement the Delete action, including surfacing filesystem errors
      (e.g. permission denied, file in use) to the user instead of failing
      silently
- [ ] 6.3 Implement the Open action (OS default handler for the target)
- [ ] 6.4 Implement the Reveal in Explorer action
- [ ] **[Windows checkpoint]** 6.5 Verify Reveal in Explorer opens the real
      Explorer shell with the item selected, on an actual Windows session
- [ ] **[Windows checkpoint]** 6.6 Verify Delete's real recycle-bin/removal
      semantics on an actual Windows session

## 7. Theming

- [ ] 7.1 Implement the dark base palette (`#0d1117`-`#0f1420` family) in
      `theme.rs`
- [ ] 7.2 Implement deterministic hash-derived hue-from-extension mapping,
      constrained to a fixed saturation/lightness band; add unit tests for
      determinism (same extension -> same color) and band constraints
- [ ] 7.3 Implement the depth/elevation lightness shift for nested blocks
- [ ] 7.4 Reserve and wire up the single accent color for hover, breadcrumb,
      and selection states only
- [ ] 7.5 Finalize exact hex values (overview doc explicitly defers this to
      implementation time, §4)

## 8. Wrap-up verification

- [ ] 8.1 Run the full `cargo test` suite (scanner + treemap unit tests)
- [ ] 8.2 Full interactive pass under WSLg: scan a real directory tree, watch
      it live-fill, drill in/out, hover, and confirm theming reads as
      intended
- [ ] **[Windows checkpoint]** 8.3 Build/run the Windows target build at
      least once end-to-end (scan, navigate, all three actions) on a real
      Windows session before considering the MVP done
