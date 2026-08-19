## 1. Lifecycle Core

- [ ] 1.1 Extract an egui-free `ScanController` with monotonic `ScanId`, current-generation state, typed success/cancel/failure outcomes, and test seams for fake engines.
- [ ] 1.2 Add a non-UI worker reaper that owns and joins every retired `JoinHandle`, including panic and shutdown paths.
- [ ] 1.3 Replace unbounded discovery delivery with a named-capacity bounded channel and non-blocking best-effort emission.

## 2. Application Integration

- [ ] 2.1 Route Scan and Rescan through controller supersession so prior work is cancelled, receivers are disconnected, and only the new generation is authoritative.
- [ ] 2.2 Tag/poll live events, progress, final results, and authoritative assembly by generation and reject every stale transition.
- [ ] 2.3 Replace partial/empty cancellation success with an explicit cancelled outcome that installs no result and shows no error.
- [ ] 2.4 Introduce one normalized requested-target state and use it for Scan, Rescan, capability checks, and Turbo elevation.
- [ ] 2.5 Disable Delete during active scanning and authoritative assembly with concise unavailable-state UI text.

## 3. Regression Coverage

- [ ] 3.1 Add fake-engine tests for A-to-B supersession, rescan supersession, late A events/completion, and cancelled generations.
- [ ] 3.2 Add tests proving cooperative workers are eventually joined, panics are contained, and a scan after panic succeeds.
- [ ] 3.3 Add a synthetic saturation test proving event count is bounded, producers do not block, and the final tree stays complete despite dropped preview events.
- [ ] 3.4 Add UI-state tests for Delete gating during scan/assembly and for typed-folder target precedence during Turbo elevation.

## 4. Verification

- [ ] 4.1 Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo test --release`.
- [ ] 4.2 Manually exercise rapid Scan/Rescan/target changes on Windows and record that no stale tree replaces the newest scan and the UI remains responsive.

