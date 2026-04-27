#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION_FILE="$ROOT_DIR/VERSION"
CARGO_TOML="$ROOT_DIR/Cargo.toml"
PACKAGE_JSON="$ROOT_DIR/web/package.json"

usage() {
    cat <<EOF
Usage: $(basename "$0") <command>

Commands:
  show          Print the current version
  patch         Bump patch (0.1.0 → 0.1.1)
  minor         Bump minor (0.1.0 → 0.2.0)
  major         Bump major (0.1.0 → 1.0.0)
  tag           Create an annotated git tag for the current version
  set <ver>     Set an explicit version (e.g., 2.3.1)
EOF
    exit 1
}

read_version() {
    tr -d '[:space:]' < "$VERSION_FILE"
}

write_version() {
    local old="$1" new="$2"

    echo "$new" > "$VERSION_FILE"
    gsed -i "s/^version = \"$old\"/version = \"$new\"/" "$CARGO_TOML"
    gsed -i "s/\"version\": \"$old\"/\"version\": \"$new\"/" "$PACKAGE_JSON"

    # Update lockfiles to reflect new version
    (cd "$ROOT_DIR" && cargo update --workspace)
    (cd "$ROOT_DIR/web" && bun install)

    echo "$old → $new"
}

cmd_show() {
    read_version
}

cmd_patch() {
    local v major minor patch new
    v=$(read_version)
    major=$(echo "$v" | cut -d. -f1)
    minor=$(echo "$v" | cut -d. -f2)
    patch=$(echo "$v" | cut -d. -f3)
    new="$major.$minor.$((patch + 1))"
    write_version "$v" "$new"
}

cmd_minor() {
    local v major minor new
    v=$(read_version)
    major=$(echo "$v" | cut -d. -f1)
    minor=$(echo "$v" | cut -d. -f2)
    new="$major.$((minor + 1)).0"
    write_version "$v" "$new"
}

cmd_major() {
    local v major new
    v=$(read_version)
    major=$(echo "$v" | cut -d. -f1)
    new="$((major + 1)).0.0"
    write_version "$v" "$new"
}

cmd_tag() {
    local v
    v=$(read_version)
    git tag -a "v$v" -m "Release v$v"
    echo "Tagged v$v"
}

cmd_set() {
    local new="$1" v
    if [[ ! "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "Error: version must be semver (e.g., 1.2.3)" >&2
        exit 1
    fi
    v=$(read_version)
    write_version "$v" "$new"
}

[[ $# -lt 1 ]] && usage

case "$1" in
    show)   cmd_show ;;
    patch)  cmd_patch ;;
    minor)  cmd_minor ;;
    major)  cmd_major ;;
    tag)    cmd_tag ;;
    set)    [[ $# -lt 2 ]] && usage; cmd_set "$2" ;;
    *)      usage ;;
esac
