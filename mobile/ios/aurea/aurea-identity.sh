#!/bin/sh
# Writes the xcconfig with the bundle id, display name and version Xcode builds the app with, from
# the app crate's [package.metadata.aurea] table, and the app icon set from its `icon` key:
#   aurea-identity.sh <Cargo.toml> <out.xcconfig> <AppIcon.appiconset>
# Uses cargo and macOS plutil, which reads the JSON cargo prints.
set -eu
manifest="$1"
out="$2"
iconset="$3"
meta=$(mktemp)
trap 'rm -f "$meta"' EXIT
cargo metadata --format-version 1 --no-deps --manifest-path "$manifest" >"$meta"

# the package whose manifest is the one given; a workspace root lists its members too
manifest_abs=$(cd "$(dirname "$manifest")" && pwd)/$(basename "$manifest")
i=0
while path=$(plutil -extract "packages.$i.manifest_path" raw -o - "$meta" 2>/dev/null); do
    [ "$path" = "$manifest_abs" ] && break
    i=$((i + 1))
done
get() { plutil -extract "packages.$i.$1" raw -o - "$meta" 2>/dev/null || true; }

package=$(get name)
[ -n "$package" ] || { echo "error: no package for $manifest"; exit 1; }
name=$(get metadata.aurea.name)
id=$(get metadata.aurea.id)
version=$(get metadata.aurea.version)
build=$(get metadata.aurea.build)
[ -n "$id" ] || { echo "error: $manifest has no [package.metadata.aurea] id"; exit 1; }
[ -n "$name" ] || name=$package
[ -n "$version" ] && [ "$version" != auto ] || version=$(get version)
[ -n "$build" ] || build=1

next=$(mktemp)
cat >"$next" <<CONF
// Made by aurea-identity.sh from $(basename "$manifest"); edit [package.metadata.aurea] there instead.
PRODUCT_BUNDLE_IDENTIFIER = $id
INFOPLIST_KEY_CFBundleDisplayName = $name
MARKETING_VERSION = $version
CURRENT_PROJECT_VERSION = $build
CONF
if cmp -s "$next" "$out"; then
    rm -f "$next"
else
    mv "$next" "$out"
    echo "warning: $(basename "$out") changed; build again to use the new identity"
fi

# In the aurea repo aurea-build runs from source; with aurea from crates.io it is the installed one.
[ -n "$(get metadata.aurea.icon)" ] || exit 0
aurea_build=$(cd "$(dirname "$0")/../../.." && pwd)/aurea-build/Cargo.toml
if [ -f "$aurea_build" ]; then
    cargo run --quiet --release --manifest-path "$aurea_build" --bin aurea-build -- \
        "$manifest" icons ios "$iconset"
elif command -v aurea-build >/dev/null 2>&1; then
    aurea-build "$manifest" icons ios "$iconset"
else
    echo "error: the icon needs aurea-build; run: cargo install aurea-build"
    exit 1
fi
