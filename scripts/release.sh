#!/usr/bin/env bash
# Bumps the version in Cargo.toml, commits it, and creates the release tag.
# WSL/bash counterpart to release.ps1 — same behavior, does NOT push.
#
# Usage:
#   ./scripts/release.sh patch|minor|major
#   ./scripts/release.sh 1.0.0
#
# Review the commit/tag, then push yourself:
#   git push && git push origin <tag>

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <patch|minor|major|X.Y.Z>" >&2
    exit 1
fi

arg="$1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_toml="$repo_root/Cargo.toml"
cargo_lock="$repo_root/Cargo.lock"

status="$(git -C "$repo_root" status --porcelain)"
if [ -n "$status" ]; then
    echo "Working tree is not clean. Commit or stash changes before releasing:" >&2
    echo "$status" >&2
    exit 1
fi

current_version="$(grep -m1 -E '^version\s*=\s*"[0-9]+\.[0-9]+\.[0-9]+"' "$cargo_toml" | sed -E 's/^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"/\1/')"
if [ -z "$current_version" ]; then
    echo "Could not find a version = \"X.Y.Z\" line in Cargo.toml" >&2
    exit 1
fi

IFS='.' read -r major minor patch <<< "$current_version"

case "$arg" in
    major) major=$((major + 1)); minor=0; patch=0 ;;
    minor) minor=$((minor + 1)); patch=0 ;;
    patch) patch=$((patch + 1)) ;;
    [0-9]*.[0-9]*.[0-9]*)
        if [[ ! "$arg" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "Explicit version must be in X.Y.Z form, got: $arg" >&2
            exit 1
        fi
        IFS='.' read -r major minor patch <<< "$arg"
        ;;
    *)
        echo "First argument must be patch, minor, major, or an explicit X.Y.Z version" >&2
        exit 1
        ;;
esac

new_version="$major.$minor.$patch"
tag="v$new_version"

if git -C "$repo_root" tag -l "$tag" | grep -q .; then
    echo "Tag $tag already exists" >&2
    exit 1
fi

echo "Bumping version: $current_version -> $new_version"

sed -i -E "s/^version\s*=\s*\"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$new_version\"/" "$cargo_toml"

if [ -f "$cargo_lock" ]; then
    perl -0pi -e "s/(name = \"bytewhiffer\"\nversion = \")\d+\.\d+\.\d+(\")/\${1}$new_version\${2}/" "$cargo_lock"
fi

git -C "$repo_root" add Cargo.toml Cargo.lock
git -C "$repo_root" commit -m "chore: bump version to $new_version"
git -C "$repo_root" tag "$tag"

echo ""
echo "Done. Review the commit, then push with:"
echo "  git push && git push origin $tag"
