## 1. Project & toolchain setup

- [x] 1.1 Decide which side(s) (WSL, Windows, or both) get a Rust toolchain via
      `rustup`, install it, and confirm `cargo --version` works there
      *(Decision: WSL side, per design.md's dev-loop split — rustup stable,
      Rust 1.97.0. Windows-side toolchain deferred until the Windows
      checkpoints need a native build.)*
- [x] 1.2 Add a `.gitattributes` (e.g. `* text=auto` or `*.rs text eol=lf`) so
      CRLF/LF drift stops recurring between WSL and Windows edits
- [x] 1.3 Create `Cargo.toml` declaring `eframe`/`egui` (check current stable
      versions rather than trusting the overview doc's snapshot) and `rfd`
      *(eframe 0.35, rfd 0.17, plus `open` 5.3 and `trash` 5.2 for the
      file-actions capability — recycle-bin-semantics delete via `trash`.)*
- [x] 1.4 Create the `src/` layout from design.md (`main.rs`, `app.rs`,
      `scanner/mod.rs`, `treemap.rs`, `theme.rs`, `util.rs`)
- [x] 1.5 Move existing root-level `scanner.rs` content into `src/scanner/`
      and `treemap.rs` into `src/treemap.rs`; confirm `cargo test` passes
      unchanged before any refactor *(all 13 baseline tests passed
      post-move, pre-refactor)*

## 2. Disk-scanning: ScanEngine trait

- [x] 2.1 Define the `ScanEngine` trait, `Availability` enum (`Available` /
      `RequiresElevation` / `UnsupportedFilesystem` / `NotApplicable`), and
      `ScanError` type in `scanner/mod.rs`
- [x] 2.2 Decide the final `ScanProgress` shape (reuse the existing two flat
      atomics vs. a richer phase-aware type — design.md's open question) and
      implement it *(Decision: kept the two flat atomics plus a `complete`
      flag for the spec's "no longer in flight" final state. Implementation
      also surfaced that counters alone can't make the map itself fill in —
      the walker's partial subtrees live on its recursion stack — so
      `ScanContext` gained an optional best-effort `mpsc` event sink
      (`ScanEvent::Discovered`) the UI uses to build an incremental mirror
      tree; an MFT engine may ignore it or emit late, matching design.md's
      "live-fill is a walker nicety, not a trait guarantee" contract.)*
- [x] 2.3 Refactor the existing walker `scan()` function into a `WalkerEngine`
      struct implementing `ScanEngine`, returning `Result<Entry, ScanError>`
- [x] 2.4 Decide sequential vs. parallel (`rayon`/`jwalk`) walker traversal
      (design.md's open question) and implement the chosen approach
      *(Decision: rayon-parallel recursion into subdirectories — speed is
      the product's reason to exist, the overview doc's Phase 1 names rayon
      explicitly, and the cancel/progress atomics were already
      thread-safe.)*
- [x] 2.5 Implement `WalkerEngine::is_available` (should report `Available`
      for any readable target)
- [x] 2.6 Port existing scanner unit tests to the new trait-based shape; add
      tests for the `Result` split (partial-skip vs. total-failure) and for
      `is_available` *(18 tests total: ported suite + unreadable-root error,
      unix permission-denied partial-skip, completion flag, event stream)*

## 3. Treemap layout integration

- [x] 3.1 Confirm `treemap.rs`'s existing `squarify` unit tests pass unchanged
      after the module move
- [x] 3.2 Add a small adapter that converts an `Entry`'s children sizes into
      `squarify` input and zips the resulting `Rect`s back to their `Entry`s
      *(lives in `app.rs::draw_children`: sorts the UI tree's children
      largest-first per frame, feeds sizes to `squarify`, zips back via the
      sort-order index)*

## 4. App shell & scan orchestration

- [x] 4.1 Implement `main.rs` (`eframe::run_native` setup) and `app.rs`'s
      `eframe::App` skeleton *(note: eframe 0.35 renamed `App::update(ctx)`
      to `App::ui(ui)` and `TopBottomPanel` to `Panel::top` — the overview
      doc's "check current API at implementation time" warning was right)*
- [x] 4.2 Wire up the native folder picker (`rfd`) to choose a scan root
      *(plus an editable path field + Scan button as an alternative entry
      path — genuinely useful, and it keeps WSL testing unblocked since this
      distro has no xdg-desktop-portal daemon for rfd's Linux backend)*
- [x] 4.3 Run the selected `ScanEngine` on a background thread; poll its
      progress from the UI thread and trigger repaints while a scan is in
      flight *(verified via mid-scan screenshot: spinner, live file/byte
      counters, Cancel button, partially-filled map)*
- [x] 4.4 Implement navigation state (current focus path / breadcrumb stack)
      in `app.rs`

## 5. Treemap rendering & interactivity

- [x] 5.1 Render the current focus node's children as treemap blocks using
      the layout adapter from Task 3.2 *(recursive nesting with depth-based
      lightness lift, per-block labels when they fit, min-area/max-depth
      cutoffs)*
- [x] 5.2 Re-render as new scan progress arrives, so the map visibly fills in
      while scanning is still running *(the UI builds an incremental mirror
      tree from the walker's `ScanEvent` stream, swapped for the
      authoritative tree at completion; verified via mid-scan screenshot)*
- [x] 5.3 Implement per-block hit-testing and hover highlighting *(deepest
      block under pointer wins; accent stroke + name/size tooltip — verified
      with a real pointer under WSLg, tooltip visible in screenshot)*
- [x] 5.4 Implement click-to-drill: clicking a folder block updates focus to
      that folder's children; clicking a file block is a no-op for
      navigation
- [x] 5.5 Implement breadcrumb / back control reflecting the navigation state
      from Task 4.4 *(drilled-in breadcrumb rendering verified via
      screenshot: `registry › src` with accent on the current crumb)*
- [ ] **[WSLg checkpoint]** 5.6 Interactively verify hit-testing, hover,
      click-to-drill, and breadcrumb navigation under WSLg — this exercises
      the overview doc's named render-interactivity risk (§7) directly
      *(partially evidenced: hover/hit-testing verified with a real pointer,
      drilled-in state verified via the debug-screenshot `drill` mode;
      real mouse click-to-drill and context-menu still need human hands —
      run `cargo run` and click around)*

## 6. File actions

- [x] 6.1 Implement the right-click context menu (Delete / Open / Reveal in
      Explorer) on a treemap block
- [x] 6.2 Implement the Delete action, including surfacing filesystem errors
      (e.g. permission denied, file in use) to the user instead of failing
      silently *(deletes via the `trash` crate for recycle-bin semantics;
      on success the node is removed from the tree in place with ancestor
      sizes adjusted, and a focus inside the deleted path falls back to its
      parent; errors show in a modal)*
- [x] 6.3 Implement the Open action (OS default handler for the target)
- [x] 6.4 Implement the Reveal in Explorer action *(Windows: `explorer
      /select,`; non-Windows dev fallback: open the containing folder)*
- [ ] **[Windows checkpoint]** 6.5 Verify Reveal in Explorer opens the real
      Explorer shell with the item selected, on an actual Windows session
- [ ] **[Windows checkpoint]** 6.6 Verify Delete's real recycle-bin/removal
      semantics on an actual Windows session

## 7. Theming

- [x] 7.1 Implement the dark base palette (`#0d1117`-`#0f1420` family) in
      `theme.rs`
- [x] 7.2 Implement deterministic hash-derived hue-from-extension mapping,
      constrained to a fixed saturation/lightness band; add unit tests for
      determinism (same extension -> same color) and band constraints
      *(FNV-1a over the lowercased extension → hue; S/V fixed; dirs get a
      muted slate so hue-coded files carry the color signal; extensionless
      files and dotfiles share a stable fallback)*
- [x] 7.3 Implement the depth/elevation lightness shift for nested blocks
- [x] 7.4 Reserve and wire up the single accent color for hover, breadcrumb,
      and selection states only
- [x] 7.5 Finalize exact hex values (overview doc explicitly defers this to
      implementation time, §4) *(BG `#0d1117`, panel `#131923`, accent
      electric blue `#58a6ff`, block band S=0.42/V=0.46 — committed in
      `theme.rs` constants; taste-tuning after hands-on use is a normal
      follow-up, not an open task)*

## 8. Wrap-up verification

- [x] 8.1 Run the full `cargo test` suite (scanner + treemap unit tests)
      *(23 passing: walker/trait 9, treemap 8, theme 5, util 1; build is
      warning-free)*
- [ ] 8.2 Full interactive pass under WSLg: scan a real directory tree, watch
      it live-fill, drill in/out, hover, and confirm theming reads as
      intended *(scan/live-fill/hover/drilled-state/theming all evidenced by
      the three debug screenshots; the remaining human part is the same as
      5.6 — real clicks)*
- [ ] **[Windows checkpoint]** 8.3 Build/run the Windows target build at
      least once end-to-end (scan, navigate, all three actions) on a real
      Windows session before considering the MVP done
