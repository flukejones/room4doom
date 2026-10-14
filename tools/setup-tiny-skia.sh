#!/usr/bin/env sh
# Materialize the pinned crate with a smaller initial edge reservation.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
checkout="$repo_root/vendor/tiny-skia"
patch_file="$repo_root/patches/tiny-skia-edges.patch"
version=0.12.0
checksum=47ffee5eaaf5527f630fb0e356b90ebdec84d5d18d937c5e440350f88c5a91ea
# Apply relative to the extracted crate, without discovering the parent repo.
export GIT_CEILING_DIRECTORIES="$repo_root/vendor"

if [ -d "$checkout" ]; then
    if git -C "$checkout" apply --reverse --check "$patch_file" 2>/dev/null; then
        echo "Tiny-Skia patch already applied at $version"
        exit 0
    fi
    echo "Unexpected Tiny-Skia checkout; move or remove vendor/tiny-skia and rerun." >&2
    exit 1
fi

mkdir -p "$repo_root/vendor"
temporary=$(mktemp -d "$repo_root/vendor/.tiny-skia.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
curl --fail --location --retry 3 --silent --show-error \
    "https://static.crates.io/crates/tiny-skia/tiny-skia-$version.crate" \
    --output "$temporary/source.crate"
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$temporary/source.crate")
else
    actual=$(shasum -a 256 "$temporary/source.crate")
fi
if [ "${actual%% *}" != "$checksum" ]; then
    echo "Tiny-Skia archive checksum mismatch" >&2
    exit 1
fi
tar -xzf "$temporary/source.crate" -C "$temporary"
git -C "$temporary/tiny-skia-$version" apply "$patch_file"
mv "$temporary/tiny-skia-$version" "$checkout"
echo "Tiny-Skia patch applied at $version"
