#!/bin/sh
# Builds an app crate's example as a static library for the platform and architectures Xcode is
# building, and leaves lib<example>.a in BUILT_PRODUCTS_DIR for the app target to link:
#   aurea-cargo.sh <Cargo.toml> <example>
# AUREA_FEATURES adds cargo features to `mobile`, such as `zengpu` to draw on the GPU.
set -eu
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

manifest=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
lib="$2"
target_dir="$(dirname "$manifest")/target/ios"
if [ "$CONFIGURATION" = "Release" ]; then
    profile=release
    release_flag=--release
else
    profile=debug
    release_flag=
fi

# The built libraries, one per architecture, collected as arguments since paths may hold spaces.
set --
for arch in $ARCHS; do
    case "$PLATFORM_NAME:$arch" in
        iphoneos:arm64) triple=aarch64-apple-ios ;;
        iphonesimulator:arm64) triple=aarch64-apple-ios-sim ;;
        iphonesimulator:x86_64) triple=x86_64-apple-ios ;;
        *)
            echo "error: no Rust target for $PLATFORM_NAME $arch"
            exit 1
            ;;
    esac
    if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed | grep -qx "$triple"; then
        echo "error: the Rust target $triple is missing; run: rustup target add $triple"
        exit 1
    fi
    # Xcode's environment describes the iOS app; cargo also builds and runs build scripts for
    # the Mac, so it gets a clean one.
    env -i HOME="$HOME" USER="${USER:-}" PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
        DEVELOPER_DIR="$DEVELOPER_DIR" CARGO_TARGET_DIR="$target_dir" \
        cargo build --manifest-path "$manifest" --example "$lib" \
            --features "mobile${AUREA_FEATURES:+,$AUREA_FEATURES}" --target "$triple" $release_flag
    set -- "$@" "$target_dir/$triple/$profile/examples/lib$lib.a"
done

out="$BUILT_PRODUCTS_DIR/lib$lib.a"
mkdir -p "$BUILT_PRODUCTS_DIR"
lipo -create "$@" -output "$out.new"
# Only a changed library relinks the app.
if cmp -s "$out.new" "$out"; then
    rm "$out.new"
else
    mv "$out.new" "$out"
fi
