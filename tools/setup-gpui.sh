#!/usr/bin/env sh
# Fetch the pinned GPUI source and apply the repository's small dependency patch.
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sh "$repo_root/tools/setup-tiny-skia.sh"
checkout="$repo_root/vendor/zed-gpui"
patch_file="$repo_root/patches/gpui-minimal.patch"
revision=$(sed -n 's/^gpui = .*rev = "\([^"]*\)".*/\1/p' "$repo_root/Cargo.toml")
if [ -z "$revision" ]; then
    echo "Cannot find the pinned GPUI revision in Cargo.toml" >&2
    exit 1
fi

if [ ! -d "$checkout/.git" ]; then
    if [ -e "$checkout" ]; then
        echo "Refusing to initialize an existing non-Git directory: $checkout" >&2
        exit 1
    fi
    git init "$checkout"
fi
if ! current_revision=$(git -C "$checkout" rev-parse --verify HEAD 2>/dev/null); then
    git -C "$checkout" fetch --depth 1 https://github.com/zed-industries/zed "$revision"
    git -C "$checkout" checkout --detach FETCH_HEAD
elif [ "$current_revision" != "$revision" ]; then
    echo "GPUI checkout is at $current_revision; expected $revision." >&2
    echo "Move or remove vendor/zed-gpui and rerun this script to regenerate it." >&2
    exit 1
fi

if git -C "$checkout" apply --reverse --check "$patch_file" 2>/dev/null; then
    echo "GPUI patch already applied at $revision"
    exit 0
fi
if ! git -C "$checkout" diff --quiet; then
    echo "GPUI checkout has changes that do not match the patch; regenerate vendor/zed-gpui." >&2
    exit 1
fi
git -C "$checkout" apply --check "$patch_file"
git -C "$checkout" apply "$patch_file"
echo "GPUI patch applied at $revision"
