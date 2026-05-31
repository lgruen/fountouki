# fountouki on iOS

How to build the macroquad app for the iOS Simulator, install it on a
physical iPad/iPhone, and ship it to TestFlight.

> **What this is.** macroquad/miniquad apps are *self-contained binaries*:
> the whole game is the compiled executable, which opens its own GL surface.
> There is **no** Rust-static-lib + UIKit-host split here — an iOS app is just
> a folder named `fountouki.app` holding the executable + `Info.plist` +
> icons. fountouki bakes its fonts into the binary via `include_bytes!`
> (`app/src/text.rs`), so there is **no `assets/` folder to bundle** either.
>
> Source: the official macroquad iOS guide (<https://macroquad.rs/articles/ios/>),
> which is the flow this directory automates.

## Files here

| file | purpose |
|------|---------|
| `build.sh` | cargo-builds `fountouki` for an iOS target and lays out `build/fountouki.app` |
| `Info.plist` | bundle metadata: name "fountouki", landscape-only, launch storyboard, icons |
| `LaunchScreen.storyboard` | splash that paints the screen `#fef6e4` (no white flash) |
| `fountouki.entitlements.plist` | codesign entitlements template (fill in your Team ID) |
| `build/` | generated; the `.app` bundle (git-ignored, created by `build.sh`) |

Landscape is locked in `Info.plist` (`UISupportedInterfaceOrientations` =
LandscapeLeft/LandscapeRight only, for both iPhone and iPad). No portrait.

## One-time setup

```sh
# Rust iOS targets (device + both simulator flavours)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# Xcode + command-line tools (provides clang, the iOS SDKs, simctl, ibtool,
# codesign, xcodebuild). Install Xcode from the App Store, then:
xcode-select --install            # if not already done
sudo xcodebuild -license accept   # accept the SDK licence once

# For installing to a physical device from the CLI (optional but easy):
brew install ios-deploy
```

`build.sh` calls plain `cargo build` (the repo `.cargo/config.toml` only adds
*wasm* linker flags; iOS targets link with the Apple SDK clang that Xcode
provides, so no extra config is needed). If linking ever fails with a missing
SDK, run `xcrun --sdk iphoneos --show-sdk-path` to confirm Xcode is selected.

## Run in the Simulator (fastest inner loop)

```sh
ios/build.sh sim          # Apple-silicon Mac  (target aarch64-apple-ios-sim)
# ios/build.sh sim-x86    # Intel Mac          (target x86_64-apple-ios)

xcrun simctl list devices available     # pick an available device UDID
xcrun simctl boot <UDID>                # skip if one is already Booted
open -a Simulator
xcrun simctl install booted ios/build/fountouki.app
xcrun simctl launch booted org.gruenschloss.fountouki
```

The Simulator does not enforce signing, so no Apple account is needed for this
step. Rotate the simulated device to landscape if it starts in portrait.

## Install on a physical iPad / iPhone

This needs an Apple ID with a signing identity. A **free** Apple ID can sign
for *your own* devices (7-day profile, must re-sign weekly); TestFlight and
non-expiring installs need the **paid Apple Developer Program ($99/yr)**.

**1. Get a provisioning profile + signing identity.** The simplest path is to
let Xcode mint them:

   - Open Xcode → Settings → Accounts → add your Apple ID.
   - Create a throwaway Xcode project (any "App" template) whose **Bundle
     Identifier is exactly `org.gruenschloss.fountouki`** (or pick your own and
     update it in `Info.plist`, `build.sh`'s `BUNDLE_ID`, and
     `fountouki.entitlements.plist`).
   - Select your Team under Signing & Capabilities; let Xcode "Automatically
     manage signing". Build+run that dummy project once onto the connected
     device. Xcode now created a matching App ID, profile, and dev cert.
   - Find the freshly-minted profile:
     `ls -t ~/Library/MobileDevice/Provisioning\ Profiles/*.mobileprovision | head`
   - Find your signing identity name:
     `security find-identity -v -p codesigning`
     (look for `Apple Development: you@example.com (XXXXXXXXXX)`).
   - Find your Team ID: Apple Developer portal → Membership, or the 10-char
     code in the identity above.

**2. Edit `fountouki.entitlements.plist`** — replace both `TEAMID` occurrences
with your Team ID (keep the bundle id matching your App ID).

**3. Build, sign, and install:**

```sh
ios/build.sh device      # target aarch64-apple-ios, release

APP=ios/build/fountouki.app
cp ~/Library/MobileDevice/Provisioning\ Profiles/<your>.mobileprovision \
   "$APP/embedded.mobileprovision"

# Sign the inner binary first, then the bundle (Sonoma+ flow).
codesign --force --timestamp=none \
  --sign "Apple Development: you@example.com (XXXXXXXXXX)" "$APP/fountouki"
codesign --force --timestamp=none \
  --sign "Apple Development: you@example.com (XXXXXXXXXX)" \
  --entitlements ios/fountouki.entitlements.plist "$APP"

ios-deploy --bundle "$APP"            # installs + launches on the attached device
```

On newer setups `ios-deploy` may be flaky; the Apple-supported alternative is:

```sh
xcrun devicectl device list                                # find the device id
xcrun devicectl device install app --device <id> "$APP"
```

Trust the developer profile on the device the first time:
Settings → General → VPN & Device Management → trust your Apple ID.

> Honesty note: codesigning a *hand-assembled* `.app` is fiddly and Xcode/macOS
> versions move the goalposts (the `--scent` / `--generate-entitlement-der`
> variant in the macroquad article is for pre-Sonoma). If the CLI signing
> fights you, the **robust** path is the Xcode-wrapper approach below — it lets
> Xcode handle signing, which is also what you need for TestFlight anyway.

## Ship to TestFlight (paid Apple Developer Program required)

TestFlight distribution requires an `.ipa` uploaded to App Store Connect, and
App Store Connect only accepts builds **archived and signed by Xcode** (or
`xcodebuild`) against a real **distribution** profile. A loose `.app` you
codesigned by hand cannot be uploaded directly. So for TestFlight you wrap the
macroquad binary in a thin Xcode project. This is a one-time setup; afterwards
each release is "Archive → Distribute".

**Recommended: thin Xcode wrapper that runs `build.sh` as a build phase.**

1. **Create the Xcode project.** New Project → iOS → App → name `fountouki`,
   interface SwiftUI/Storyboard (doesn't matter — its views are never shown),
   bundle id `org.gruenschloss.fountouki`. This gives you a real, signable,
   uploadable target.

2. **Delete the default app entry point** is *not* needed — instead, the
   cleanest trick is to make Xcode's product *be* the macroquad binary:
   - Add a **Run Script** build phase (before "Compile Sources") that calls
     `"$SRCROOT/../ios/build.sh" device` so cargo produces the binary.
   - Then either (a) let the macroquad binary become the app's executable, or
     (b) keep the stock Swift `@main` and have it `exec`/load nothing — option
     (a) is simplest: set the target's **`CFBundleExecutable`** to `fountouki`
     and add a **Copy Files** (Destination: Executables / bundle root) phase
     that copies `target/aarch64-apple-ios/release/fountouki` into the product.
   - Point the target's Info.plist settings at the orientation/launch keys in
     this directory's `Info.plist` (or paste those keys into the project's
     generated plist): landscape-only, `UILaunchStoryboardName = LaunchScreen`,
     and add `LaunchScreen.storyboard` + the icons to the project so they land
     in the bundle.

   > Verify this wiring yourself — the exact build-phase ordering and whether
   > Xcode's own compiled Swift stub must be removed varies by Xcode version.
   > The miniquad iOS example
   > (<https://github.com/Gordon-F/miniquad_ios_example>, uses **XcodeGen** —
   > `brew install xcodegen`, `cd ios && xcodegen`, open the project, set the
   > signing team, Run) is the canonical worked reference for a project that
   > links a Rust build into an iOS target. Note that repo is archived (2023)
   > and links the *miniquad* C/Rust glue as a static lib; for a pure-macroquad
   > app the simpler "copy the binary in as `CFBundleExecutable`" route above
   > avoids the static-lib + Obj-C `main` shim entirely. Confirm whichever you
   > pick actually launches before relying on it.

3. **Set up signing for distribution.** In the target's Signing &
   Capabilities, pick your Team. For upload you need an *App Store* /
   distribution provisioning profile — automatic signing in Xcode will create
   it, or make one in the Apple Developer portal.

4. **Archive and upload:**
   - Select destination "Any iOS Device (arm64)".
   - Product → Archive.
   - In the Organizer that opens: Distribute App → App Store Connect → Upload.
   - Or via CLI:
     `xcodebuild -scheme fountouki -archivePath build/fountouki.xcarchive archive`
     then
     `xcodebuild -exportArchive -archivePath build/fountouki.xcarchive -exportOptionsPlist ExportOptions.plist -exportPath build/`
     and upload the resulting `.ipa` with
     `xcrun altool --upload-app -f build/fountouki.ipa -t ios -u APPLE_ID -p APP_SPECIFIC_PASSWORD`
     (or Transporter.app).

5. **In App Store Connect:** the build appears under your app's TestFlight tab
   after processing (a few minutes). Add internal testers (no review) or set up
   external testing (needs a quick Beta App Review). Testers install via the
   TestFlight app.

### What is genuinely the maintainer's manual step

- **Apple Developer Program enrolment ($99/yr)** — required for TestFlight and
  for any non-7-day install. Cannot be scripted.
- **Signing identity + provisioning profiles + Team ID** — tied to your Apple
  account; fill them into `fountouki.entitlements.plist` and the Xcode signing
  panel.
- **The Xcode wrapper project itself** — App Store Connect will not accept a
  hand-codesigned loose `.app`, only an Xcode-archived signed build. Creating
  that project (step above) is a manual, one-time Xcode UI task; the exact
  build-phase plumbing should be verified on your Xcode version.

## Uncertainty / things to verify

- The bundle id `org.gruenschloss.fountouki` is a guess based on the
  maintainer's email domain; set it to whatever your App ID uses and keep
  `Info.plist`, `build.sh`, and the entitlements in sync.
- Icon handling for a hand-built bundle (flat `AppIcon60x60@2x.png` PNGs vs a
  compiled `Assets.car`) is best-effort — the home-screen icon may fall back to
  a generic glyph until you set icons in the Xcode wrapper. Cosmetic only.
- macOS/Xcode versions change the codesign incantation; the README targets
  Sonoma+ (`--entitlements`, no `--scent`). On older macOS use the
  `--scent`/`--generate-entitlement-der` form from the macroquad article.

## References

- macroquad iOS guide — <https://macroquad.rs/articles/ios/>
- miniquad iOS example (XcodeGen, archived 2023) —
  <https://github.com/Gordon-F/miniquad_ios_example>
