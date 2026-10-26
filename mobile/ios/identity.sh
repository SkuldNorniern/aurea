#!/bin/sh
# Writes Identity.xcconfig, the bundle id, display name and version Xcode builds the app with,
# from the app crate's [package.metadata.aurea] table: identity.sh <Cargo.toml> <out.xcconfig>.
# Uses cargo and macOS plutil, which reads the JSON cargo prints.
set -eu
manifest="$1"
out="$2"
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
// Made by identity.sh from $(basename "$manifest"); edit [package.metadata.aurea] there instead.
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
