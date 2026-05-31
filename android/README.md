# Android build (fountouki)

Native Android APK of the macroquad app. macroquad/miniquad ship an Android
backend, so the *same* `app` crate that builds the desktop `fountouki` binary
and the `wasm32` web build also builds an `.apk` — no separate native code.

Status: **scaffolding, not yet verified end to end.** The commands below match
the current (2026) official macroquad guidance, but no APK has been produced or
sideloaded from this repo yet. Treat the "verify" notes as real TODOs.

## How macroquad does Android (the short version)

- Official tool: **`cargo-quad-apk`** — not-fl3's miniquad-aware fork of
  `cargo-apk`. Plain `cargo-apk` (the rust-mobile one) is a *different* tool and
  is **not** what macroquad documents; don't mix them up.
- Recommended driver: the prebuilt Docker image **`notfl3/cargo-apk`**, which
  bundles a known-good Android SDK + NDK + `cargo-quad-apk`. This is the path
  that avoids the recurring "NDK toolchain binary not found" breakage people hit
  with hand-installed modern NDKs.
- Config lives in a `[package.metadata.android]` block in `app/Cargo.toml`
  (label, package id, orientation, icon, target ABIs).

Primary docs: <https://macroquad.rs/articles/android/> and
<https://github.com/not-fl3/cargo-quad-apk>.

## 1. Add the Android metadata to `app/Cargo.toml`

This block is **not committed yet** (the task that created this dir wasn't
allowed to edit `app/Cargo.toml`). Paste it at the end of `app/Cargo.toml`:

```toml
[package.metadata.android]
# App id on device. Defaults to "rust.fountouki" if omitted.
package_name = "org.gruenschloss.fountouki"
label = "fountouki"
version_code = 1
version_name = "0.1.0"
# Launcher icon + resources. Run android/make-icons.sh first to populate this.
res = "android/res"
icon = "@mipmap/ic_launcher"
# ABIs to build. Each adds a full release compile. arm64 + armv7 cover real
# phones/tablets; the x86 ones are only for emulators — drop them to build
# ~2x faster if you only target hardware.
build_targets = [
    "aarch64-linux-android",
    "armv7-linux-androideabi",
    "x86_64-linux-android",
]
# SDK levels. android_version is the compile SDK; cargo-quad-apk defaults are
# old (29 / min 18). Bump android_version/target_sdk_version if a newer device
# or Play upload demands it — see caveats below.
android_version = 33
target_sdk_version = 33
min_sdk_version = 24

# Landscape lock + API 31+ launch requirement. "userLandscape" allows both
# landscape rotations (matches the PWA's "landscape-primary").
[package.metadata.android.activity_attributes]
"android:screenOrientation" = "userLandscape"
"android:exported" = "true"
```

Notes:
- The web/PWA manifest uses `orientation: "landscape-primary"`; `userLandscape`
  is the closest Android equivalent that still allows 180° flips.
- `"android:exported" = "true"` is **mandatory** on `target_sdk_version >= 31`
  for the launcher activity, or the app won't install/launch.
- macroquad games render full-screen anyway; add `fullscreen = true` under
  `[package.metadata.android]` if you want to hide the status bar too.

## 2. Generate the launcher icon (once)

```bash
android/make-icons.sh
```

Reads `public/icon-512.png` and writes `android/res/mipmap-*/ic_launcher.png`
for each density. Needs ImageMagick; if you don't have it, the script prints the
exact sizes to make by hand. Commit `android/res/` once generated.

## 3a. Build with Docker (recommended)

Needs only Docker running.

```bash
android/build.sh
```

Under the hood:

```bash
docker run --rm -v "$(pwd)":/root/src -w /root/src \
  notfl3/cargo-apk cargo quad-apk build --release
```

First run pulls the image and does **one full release build per ABI**, so it's
slow (many minutes). Output:

```
app/target/android-artifacts/release/apk/fountouki.apk
```

(The exact filename/subdir can vary by cargo-quad-apk version — `ls` the
`apk/` dir if `fountouki.apk` isn't there. **Verify.**)

## 3b. Build locally (no Docker)

More fragile — only if you can't use Docker.

1. Install the tool from source (the crates.io `cargo-quad-apk` is stale; the
   git version tracks current macroquad):
   ```bash
   cargo install --git https://github.com/not-fl3/cargo-quad-apk
   ```
2. Add the Rust Android targets:
   ```bash
   rustup target add aarch64-linux-android armv7-linux-androideabi \
       i686-linux-android x86_64-linux-android
   ```
3. Install the Android SDK + a **known-good NDK**. The macroquad guide pins
   **NDK r25** (`android-ndk-r25` / 25.x); newer NDKs (r26+) have moved the
   toolchain layout and have repeatedly broken cargo-quad-apk (see
   not-fl3/macroquad issues #400/#490). Use r25 unless you've verified newer.
   Easiest via Android Studio's SDK Manager, or `sdkmanager`:
   ```bash
   sdkmanager "platform-tools" "platforms;android-33" "ndk;25.2.9519653"
   ```
4. Export paths and build:
   ```bash
   export ANDROID_HOME="$HOME/Library/Android/sdk"        # macOS default
   export NDK_HOME="$ANDROID_HOME/ndk/25.2.9519653"
   android/build.sh --local
   ```

## 4. Sideload to a device

1. On the device: Settings → enable **Developer options** → **USB debugging**.
2. Plug in over USB, accept the RSA prompt, confirm it's visible:
   ```bash
   adb devices
   ```
3. Install:
   ```bash
   adb install -r app/target/android-artifacts/release/apk/fountouki.apk
   ```
   `-r` reinstalls over an existing copy; omit it for a clean first install.
4. Launch from the app drawer (label "fountouki").

The Docker/cargo-quad-apk build produces a **debug-signed** APK, which installs
fine via `adb` for personal/family sideloading. It is **not** acceptable for
Play Store upload — that needs a release keystore:

```bash
keytool -genkey -v -keystore fountouki.keystore -alias fountouki \
    -keyalg RSA -keysize 2048 -validity 10000
apksigner sign --ks fountouki.keystore <unsigned.apk>
```

(Play Store also wants an **`.aab`**, not an `.apk`; cargo-quad-apk emits APKs
only. Out of scope for sideloading — flagged for later.)

## Caveats / things to verify before trusting this

- **End-to-end unverified.** No APK has been built from this repo yet. First run
  may surface a missing target, an NDK path issue, or a different output path.
- **Docker image freshness.** `notfl3/cargo-apk` is community-maintained and has
  gone long stretches without updates; its bundled SDK/NDK/target-SDK may be
  older than current Play requirements. Fine for sideloading; re-check if you
  ever publish. Verify it still builds against the current macroquad 0.4.x.
- **NDK version (local build).** r25 is the safe pin. r26+ has broken this
  toolchain before — only move up after a successful build.
- **APK filename/path.** Confirm `fountouki.apk` vs a `-debug`/per-ABI name in
  `app/target/android-artifacts/release/apk/` after the first build and fix the
  `adb install` path here if it differs.
- **No audio/asset wiring checked.** If the app loads runtime asset files, point
  `assets = "..."` at them in the metadata block; bundled `include_bytes!`
  assets need nothing.

## Files in this dir

- `build.sh` — Docker (default) or `--local` APK build wrapper.
- `make-icons.sh` — generate `res/mipmap-*/ic_launcher.png` from `icon-512.png`.
- `res/` — generated Android resources (created by `make-icons.sh`).
- `README.md` — this file.
