# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Release automation: `scripts/release.ps1` / `scripts/release.sh` bump the
  version, sync `Cargo.lock`, commit, and tag in one step.
- `.github/workflows/release.yml` now verifies the pushed tag matches
  `Cargo.toml`'s version before building.
- `LICENSE` (MIT), `CONTRIBUTING.md`, issue/PR templates.

## [0.1.7]

Prototype/development versions prior to this changelog's introduction are not
individually documented. See `git log` for full history.
