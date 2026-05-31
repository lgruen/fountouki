#!/usr/bin/env bash
# Build the fountouki Android APK.
#
# Two modes:
#   ./build.sh            -> Docker build (default, recommended; no host NDK needed)
#   ./build.sh --local    -> local build using host Android SDK + NDK
#
# Output APK: app/target/android-artifacts/release/apk/fountouki.apk
#
# Prereqs / caveats live in android/README.md. Read it before first run.
# The APK metadata ([package.metadata.android]) is NOT in app/Cargo.toml yet —
# the README has the exact block to paste in. Without it you get defaults
# (package id rust.fountouki, no landscape lock, no icon).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCKER_IMAGE="notfl3/cargo-apk"
APK_OUT="${REPO_ROOT}/app/target/android-artifacts/release/apk"

mode="docker"
if [[ "${1:-}" == "--local" ]]; then
  mode="local"
fi

echo "==> fountouki Android build (mode: ${mode})"
echo "==> repo root: ${REPO_ROOT}"

if [[ "${mode}" == "docker" ]]; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: docker not found. Install Docker, or run: ./build.sh --local" >&2
    exit 1
  fi
  # The not-fl3 image bundles a known-good Android SDK + NDK + cargo-quad-apk,
  # which sidesteps the host NDK toolchain-path breakage seen on newer NDKs.
  # It builds every target in [package.metadata.android].build_targets, so it
  # is slow on the first run (one full release compile per ABI).
  echo "==> pulling ${DOCKER_IMAGE} (skip with: docker pull yourself once)"
  docker pull "${DOCKER_IMAGE}" || echo "    (pull failed; using local cache if present)"

  echo "==> building (this does one release build per ABI — be patient)"
  docker run --rm \
    -v "${REPO_ROOT}":/root/src \
    -w /root/src \
    "${DOCKER_IMAGE}" \
    cargo quad-apk build --release
else
  # Local build. Requires:
  #   - cargo install cargo-quad-apk        (from the not-fl3 repo; see README)
  #   - rustup target add aarch64-linux-android armv7-linux-androideabi \
  #         i686-linux-android x86_64-linux-android
  #   - ANDROID_HOME + NDK_HOME exported (see README for a known-good NDK)
  : "${ANDROID_HOME:?ERROR: export ANDROID_HOME (Android SDK path) — see README}"
  : "${NDK_HOME:?ERROR: export NDK_HOME (Android NDK path) — see README}"

  if ! cargo quad-apk --version >/dev/null 2>&1; then
    echo "ERROR: 'cargo quad-apk' not installed. See android/README.md." >&2
    exit 1
  fi

  echo "==> ANDROID_HOME=${ANDROID_HOME}"
  echo "==> NDK_HOME=${NDK_HOME}"
  ( cd "${REPO_ROOT}" && cargo quad-apk build --release )
fi

echo
echo "==> done. APK(s) under:"
echo "    ${APK_OUT}"
ls -la "${APK_OUT}" 2>/dev/null || echo "    (not found — check the build log above)"
echo
echo "Install to a connected device:  adb install -r <apk-above>"
echo "(adb install -r reinstalls; drop -r for a first install.)"
