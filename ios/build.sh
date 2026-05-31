#!/usr/bin/env bash
#
# build.sh — assemble fountouki.app for iOS (device or simulator).
#
# macroquad/miniquad apps are self-contained: the whole game IS the binary
# (no Rust static-lib + Xcode UIKit host needed). An iOS app is just a folder
# named `Foo.app` holding the executable + Info.plist + icons. This script
# cargo-builds the `fountouki` binary for an iOS target and lays out that
# bundle in ios/build/fountouki.app.
#
# Fonts are baked into the binary via include_bytes! (see app/src/text.rs), so
# there is NO external assets/ folder to copy — the bundle is binary + plist
# + icon only.
#
# Usage:
#   ios/build.sh device      # aarch64-apple-ios   (release) — physical iPhone/iPad
#   ios/build.sh sim         # aarch64-apple-ios-sim (debug) — Apple-silicon Simulator
#   ios/build.sh sim-x86     # x86_64-apple-ios    (debug)  — Intel-Mac Simulator
#
# Signing, provisioning, install-to-device and TestFlight upload are NOT done
# here — they need your Apple Developer identity. See ios/README.md.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BIN="fountouki"                 # crate `name` in app/Cargo.toml == executable name
BUNDLE_ID="org.gruenschloss.fountouki"   # change to match your provisioning profile
APP="$HERE/build/${BIN}.app"

mode="${1:-sim}"
case "$mode" in
  device)  TARGET="aarch64-apple-ios";      PROFILE="release"; FLAG="--release" ;;
  sim)     TARGET="aarch64-apple-ios-sim";  PROFILE="debug";   FLAG="" ;;
  sim-x86) TARGET="x86_64-apple-ios";       PROFILE="debug";   FLAG="" ;;
  *) echo "usage: $0 {device|sim|sim-x86}" >&2; exit 2 ;;
esac

echo ">> ensuring rust target $TARGET is installed"
rustup target add "$TARGET" >/dev/null 2>&1 || {
  echo "   could not add target via rustup; assuming it is already present" >&2
}

echo ">> cargo build -p $BIN --target $TARGET ($PROFILE)"
# shellcheck disable=SC2086
cargo build --manifest-path "$ROOT/Cargo.toml" -p "$BIN" --target "$TARGET" $FLAG

echo ">> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP"
cp "$ROOT/target/$TARGET/$PROFILE/$BIN" "$APP/$BIN"
cp "$HERE/Info.plist" "$APP/Info.plist"

# Home-screen icon (180x180 reused for all slots; iOS scales it down fine).
if [ -f "$ROOT/public/icon-180.png" ]; then
  cp "$ROOT/public/icon-180.png" "$APP/AppIcon60x60@2x.png"
  cp "$ROOT/public/icon-180.png" "$APP/AppIcon76x76@2x~ipad.png"
fi

# Launch screen storyboard (gives the #fef6e4 splash). Compile it if ibtool is
# available; harmless to skip on minimal setups.
if command -v ibtool >/dev/null 2>&1 && [ -f "$HERE/LaunchScreen.storyboard" ]; then
  echo ">> compiling LaunchScreen.storyboard"
  ibtool --compile "$APP/LaunchScreen.storyboardc" "$HERE/LaunchScreen.storyboard" \
    >/dev/null 2>&1 || echo "   (ibtool failed; launch screen will be plain — non-fatal)"
fi

echo ">> done: $APP"
echo "   bundle id: $BUNDLE_ID"
case "$mode" in
  sim|sim-x86)
    cat <<EOF

Run in the Simulator:
  xcrun simctl list devices available        # pick a booted/available device UDID
  xcrun simctl boot <UDID>                    # if not already booted
  open -a Simulator
  xcrun simctl install booted "$APP"
  xcrun simctl launch booted $BUNDLE_ID
EOF
    ;;
  device)
    cat <<EOF

Sign + install on a physical device (needs your signing identity + profile —
see ios/README.md):
  cp ~/Library/MobileDevice/Provisioning\\ Profiles/<your>.mobileprovision \\
     "$APP/embedded.mobileprovision"
  codesign --force --timestamp=none --sign "<CODESIGN IDENTITY>" "$APP/$BIN"
  codesign --force --timestamp=none --sign "<CODESIGN IDENTITY>" \\
     --entitlements "$HERE/fountouki.entitlements.plist" "$APP"
  ios-deploy --bundle "$APP"        # or: xcrun devicectl device install app ...
EOF
    ;;
esac
