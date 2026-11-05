#!/bin/sh
# The sample's build phase. Like any app, it finds the aurea crate its Cargo.toml depends on and
# runs the scripts in that crate's mobile/ios/aurea.
set -eu
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

manifest="$SRCROOT/../../Cargo.toml"
meta=$(mktemp)
trap 'rm -f "$meta"' EXIT
cargo metadata --format-version 1 --manifest-path "$manifest" >"$meta"
i=0
while name=$(plutil -extract "packages.$i.name" raw -o - "$meta" 2>/dev/null); do
    [ "$name" = aurea ] && break
    i=$((i + 1))
done
aurea_manifest=$(plutil -extract "packages.$i.manifest_path" raw -o - "$meta" 2>/dev/null) ||
    { echo "error: $manifest does not depend on aurea"; exit 1; }
aurea="$(dirname "$aurea_manifest")/mobile/ios/aurea"

sh "$aurea/aurea-identity.sh" "$manifest" "$SRCROOT/Identity.xcconfig" \
    "$SRCROOT/AureaSmoke/Assets.xcassets/AppIcon.appiconset"
sh "$aurea/aurea-cargo.sh" "$manifest" ios_demo
