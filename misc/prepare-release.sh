#!/bin/sh
# Prepare a release commit: bump every workspace version, refresh the
# lockfile, and record the release in the AppStream metainfo, so the
# tagged tree carries its own release entry (Flathub builds from the
# tag; a post-tag fixup is forever one release behind).
#
# Usage: misc/prepare-release.sh 0.5.26
# Then review the diff, commit, tag vX.Y.Z, and push both.
set -eu

version="${1:?usage: misc/prepare-release.sh <version, e.g. 0.5.26>}"
version="${version#v}"
# Validate BEFORE any mutation: a bad version must not leave a
# half-bumped tree behind.
if ! printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "error: '$version' is not a plain X.Y.Z version" >&2
    exit 1
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

current="$(perl -ne 'print $1 and exit if /^version = "([^"]+)"/' Cargo.toml)"
if [ -z "$current" ]; then
    echo "error: could not read the workspace version from Cargo.toml" >&2
    exit 1
fi
if [ "$current" = "$version" ]; then
    echo "error: workspace is already at $version" >&2
    exit 1
fi

# Workspace version plus every path dependency pinned to it. Only
# lines that declare the workspace version or a path dep are touched,
# so a third-party crate that happens to share the version string can
# never be rewritten.
CUR="$current" NEW="$version" perl -pi -e \
    's/version = "\Q$ENV{CUR}\E"/version = "$ENV{NEW}"/ if /^version = / || /path = /' \
    Cargo.toml
cargo update --workspace --quiet

# AppStream metainfo: newest release first, right under <releases>.
metainfo="misc/com.rioterm.Rio.metainfo.xml"
if grep -q "release version=\"$version\"" "$metainfo"; then
    echo "metainfo already has $version, leaving it as is"
else
    NEW="$version" TODAY="$(date +%Y-%m-%d)" perl -pi -e 'print qq(    <release version="$ENV{NEW}" date="$ENV{TODAY}">\n      <url type="details">https://github.com/raphamorim/rio/releases/tag/v$ENV{NEW}</url>\n    </release>\n) if $. == $insert_line; $insert_line = $. + 1 if /^  <releases>$/' "$metainfo"
    grep -q "release version=\"$version\"" "$metainfo" || {
        echo "error: failed to insert the metainfo release entry" >&2
        exit 1
    }
fi

echo "prepared $current -> $version"
echo "next: git add -A && git commit -m 'prepare $version' && git tag v$version"
