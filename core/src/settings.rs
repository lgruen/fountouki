//! Shared + per-game settings — pure parity with the TS app's `settings.ts`
//! and the patterns settings section.
//!
//! - [`SharedSettings`]: app-wide blob at `fountouki.shared.settings.v1`
//!   (mute + sync config). Load = DEFAULTS merged with the stored blob so any
//!   missing key falls back to its default.
//! - [`PatternsSettings`]: per-game blob at `fountouki.patterns.settings.v1`.
//!   Load only overrides a default when the field is present in the blob.
//! - [`generate_token`]: mint a 16-char `[a-z0-9]` family sync token.
//!
//! JSON key strings and defaults are load-bearing for save-compat / sync
//! interop with existing TS clients — do not rename or re-default.

// nanoserde's DeJson derive expands to code that trips this lint; the allow
// must be module-wide because the generated impls sit outside the items.
#![allow(clippy::question_mark)]
use crate::rng::Mulberry32;
use crate::storage::{ns_key, KeyValueStore};
use nanoserde::{DeJson, SerJson};

// ---------------------------------------------------------------------------
// Shared settings
// ---------------------------------------------------------------------------

/// App-wide settings blob. Stored at `fountouki.shared.settings.v1`.
///
/// `muted` is the single app-wide mute toggle. `sync_token` is the family-level
/// sync namespace (None / empty = sync disabled). `sync_endpoint` overrides the
/// default Worker URL (None / empty = use the default).
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson, Default)]
pub struct SharedSettings {
    #[nserde(rename = "muted")]
    pub muted: bool,
    #[nserde(rename = "syncToken")]
    pub sync_token: Option<String>,
    #[nserde(rename = "syncEndpoint")]
    pub sync_endpoint: Option<String>,
}


/// Fully-optional parse view of [`SharedSettings`] so a blob with any subset of
/// keys deserializes cleanly; missing keys then fall back to the defaults
/// (the TS `{ ...DEFAULTS, ...stored }` merge).
#[derive(DeJson)]
struct SharedSettingsPatch {
    #[nserde(rename = "muted")]
    muted: Option<bool>,
    #[nserde(rename = "syncToken")]
    sync_token: Option<String>,
    #[nserde(rename = "syncEndpoint")]
    sync_endpoint: Option<String>,
}

/// Load shared settings: DEFAULTS merged with the stored blob. A missing key,
/// an absent blob, or a parse error all fall back to defaults (errorless).
pub fn load_shared<S: KeyValueStore + ?Sized>(store: &S) -> SharedSettings {
    let mut s = SharedSettings::default();
    let key = ns_key("shared", "settings");
    if let Some(raw) = store.get(&key) {
        if let Ok(patch) = SharedSettingsPatch::deserialize_json(&raw) {
            if let Some(m) = patch.muted {
                s.muted = m;
            }
            // `syncToken` / `syncEndpoint`: a present null and an absent key both
            // mean "default" (None). nanoserde maps both to `None` here, so a
            // stored non-null is the only thing that overrides the default.
            if patch.sync_token.is_some() {
                s.sync_token = patch.sync_token;
            }
            if patch.sync_endpoint.is_some() {
                s.sync_endpoint = patch.sync_endpoint;
            }
        }
    }
    s
}

/// Persist the whole shared-settings object.
pub fn save_shared<S: KeyValueStore + ?Sized>(store: &mut S, settings: &SharedSettings) {
    let key = ns_key("shared", "settings");
    store.set(&key, &settings.serialize_json());
}

// ---------------------------------------------------------------------------
// Patterns settings
// ---------------------------------------------------------------------------

/// Per-game patterns prefs. Stored at `fountouki.patterns.settings.v1`.
///
/// `theme_choice` enum values: `mix`, `emoji-animals`, `emoji-fruit`,
/// `emoji-vehicles`, `emoji-construction`, `emoji-dinosaurs`, `shapes`,
/// `letters-upper`, `letters-lower`, `numbers`.
/// `difficulty`: `auto` | `easy` | `hard`. `mode`: `next` | `unit`.
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct PatternsSettings {
    #[nserde(rename = "themeChoice")]
    pub theme_choice: String,
    #[nserde(rename = "difficulty")]
    pub difficulty: String,
    #[nserde(rename = "mode")]
    pub mode: String,
    #[nserde(rename = "showHint")]
    pub show_hint: bool,
}

impl Default for PatternsSettings {
    fn default() -> Self {
        Self {
            theme_choice: "mix".to_string(),
            difficulty: "auto".to_string(),
            mode: "next".to_string(),
            show_hint: false,
        }
    }
}

/// Fully-optional parse view of [`PatternsSettings`]; load only overrides a
/// default when the field is present (and, for `show_hint`, a bool — nanoserde
/// enforces the type on parse).
#[derive(DeJson)]
struct PatternsSettingsPatch {
    #[nserde(rename = "themeChoice")]
    theme_choice: Option<String>,
    #[nserde(rename = "difficulty")]
    difficulty: Option<String>,
    #[nserde(rename = "mode")]
    mode: Option<String>,
    #[nserde(rename = "showHint")]
    show_hint: Option<bool>,
}

/// Load patterns settings: defaults overridden only by fields present in the
/// stored blob. Absent blob / parse error → all defaults.
pub fn load_patterns<S: KeyValueStore + ?Sized>(store: &S) -> PatternsSettings {
    let mut s = PatternsSettings::default();
    let key = ns_key("patterns", "settings");
    if let Some(raw) = store.get(&key) {
        if let Ok(patch) = PatternsSettingsPatch::deserialize_json(&raw) {
            if let Some(v) = patch.theme_choice {
                s.theme_choice = v;
            }
            if let Some(v) = patch.difficulty {
                s.difficulty = v;
            }
            if let Some(v) = patch.mode {
                s.mode = v;
            }
            if let Some(v) = patch.show_hint {
                s.show_hint = v;
            }
        }
    }
    s
}

/// Persist the whole patterns-settings object.
pub fn save_patterns<S: KeyValueStore + ?Sized>(store: &mut S, settings: &PatternsSettings) {
    let key = ns_key("patterns", "settings");
    store.set(&key, &settings.serialize_json());
}

// ---------------------------------------------------------------------------
// Sing Back settings
// ---------------------------------------------------------------------------

/// Per-game "Sing Back" memory prefs. Stored at `fountouki.singback.settings.v1`.
///
/// `difficulty` is a pacing choice stored as a string (mirroring
/// [`PatternsSettings`]' string choice fields so nanoserde serializes it the
/// same way): `gentle` (slow playback, generous reproduce window) | `normal` |
/// `speedy` (faster playback, tighter window). Default `normal`.
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct SingbackSettings {
    #[nserde(rename = "difficulty")]
    pub difficulty: String,
}

impl Default for SingbackSettings {
    fn default() -> Self {
        Self {
            difficulty: "normal".to_string(),
        }
    }
}

/// Fully-optional parse view of [`SingbackSettings`]; load only overrides a
/// default when the field is present (nanoserde enforces the type on parse).
#[derive(DeJson)]
struct SingbackSettingsPatch {
    #[nserde(rename = "difficulty")]
    difficulty: Option<String>,
}

/// Load Sing Back settings: defaults overridden only by fields present in the
/// stored blob. Absent blob / parse error → all defaults.
pub fn load_singback<S: KeyValueStore + ?Sized>(store: &S) -> SingbackSettings {
    let mut s = SingbackSettings::default();
    let key = ns_key("singback", "settings");
    if let Some(raw) = store.get(&key) {
        if let Ok(patch) = SingbackSettingsPatch::deserialize_json(&raw) {
            if let Some(v) = patch.difficulty {
                s.difficulty = v;
            }
        }
    }
    s
}

/// Persist the whole Sing Back-settings object.
pub fn save_singback<S: KeyValueStore + ?Sized>(store: &mut S, settings: &SingbackSettings) {
    let key = ns_key("singback", "settings");
    store.set(&key, &settings.serialize_json());
}

// ---------------------------------------------------------------------------
// Clock ("Frog's Day") settings
// ---------------------------------------------------------------------------

/// Per-game "Frog's Day" analog-clock prefs. Stored at
/// `fountouki.clock.settings.v1`.
///
/// `difficulty` is the parent-chosen level, stored as a string (mirroring the
/// other games' string choice fields): `match` (the target number glows on the
/// dial — drag the little hand onto it; big hand pinned) | `routine` (no dial
/// highlight; still little-hand only) | `clock` (set BOTH hands for o'clock) |
/// `halfpast` (adds half-past targets — set the big hand up OR down). Default
/// `match`, the gentlest.
///
/// Unlike the other games' settings, this one is **cross-device synced** (under
/// the `clockcfg` key) so a difficulty the parent picks on one device follows
/// the family: `last_seen` timestamps the parent's choice and drives the
/// last-write-wins [`merge_clock`]. JSON keys (`difficulty`, `lastSeen`) are
/// load-bearing for that sync — do not rename.
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct ClockSettings {
    #[nserde(rename = "difficulty")]
    pub difficulty: String,
    /// epoch ms when the parent last changed `difficulty`; 0 = never. The newer
    /// timestamp wins [`merge_clock`]; an absent key (older blob) reads as 0.
    #[nserde(rename = "lastSeen")]
    #[nserde(default)]
    pub last_seen: i64,
}

impl Default for ClockSettings {
    fn default() -> Self {
        Self { difficulty: "match".to_string(), last_seen: 0 }
    }
}

/// Fully-optional parse view of [`ClockSettings`]; load only overrides a default
/// when the field is present (nanoserde enforces the type on parse).
#[derive(DeJson)]
struct ClockSettingsPatch {
    #[nserde(rename = "difficulty")]
    difficulty: Option<String>,
    #[nserde(rename = "lastSeen")]
    last_seen: Option<i64>,
}

/// Parse a clock-settings blob (local store value or a remote sync body) onto
/// the defaults: only fields present in valid JSON override. Garbage → defaults.
pub fn parse_clock(raw: &str) -> ClockSettings {
    let mut s = ClockSettings::default();
    if let Ok(patch) = ClockSettingsPatch::deserialize_json(raw) {
        if let Some(v) = patch.difficulty {
            s.difficulty = v;
        }
        if let Some(v) = patch.last_seen {
            s.last_seen = v;
        }
    }
    s
}

/// Merge a remote clock-settings blob into local (cross-device sync):
/// **last-write-wins** by `last_seen` — the more-recent parent choice wins. A
/// tie keeps the lexicographically-greater `difficulty`, so the result is
/// commutative + idempotent regardless of merge order.
pub fn merge_clock(local: &ClockSettings, remote: &ClockSettings) -> ClockSettings {
    use std::cmp::Ordering;
    match local.last_seen.cmp(&remote.last_seen) {
        Ordering::Greater => local.clone(),
        Ordering::Less => remote.clone(),
        Ordering::Equal if local.difficulty >= remote.difficulty => local.clone(),
        Ordering::Equal => remote.clone(),
    }
}

/// Load clock settings: defaults overridden only by fields present in the stored
/// blob. Absent blob / parse error → all defaults.
pub fn load_clock<S: KeyValueStore + ?Sized>(store: &S) -> ClockSettings {
    let key = ns_key("clock", "settings");
    store.get(&key).map(|raw| parse_clock(&raw)).unwrap_or_default()
}

/// Persist the whole clock-settings object.
pub fn save_clock<S: KeyValueStore + ?Sized>(store: &mut S, settings: &ClockSettings) {
    let key = ns_key("clock", "settings");
    store.set(&key, &settings.serialize_json());
}

// ---------------------------------------------------------------------------
// Token generation
// ---------------------------------------------------------------------------

/// Charset for sync tokens: lowercase ASCII + digits (36 symbols). Matches the
/// TS `generateToken` alphabet and the server's `[a-z0-9]{8,64}` bound.
const TOKEN_CHARS: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// Token length minted by the parent "Generate new" button.
const TOKEN_LEN: usize = 16;

/// Mint a 16-char family sync token from `[a-z0-9]`. Uses the deterministic
/// [`Mulberry32`] so it is testable; the host seeds it from a real entropy
/// source (the TS app uses `crypto.getRandomValues`).
pub fn generate_token(rng: &mut Mulberry32) -> String {
    let mut out = String::with_capacity(TOKEN_LEN);
    for _ in 0..TOKEN_LEN {
        let idx = rng.below(TOKEN_CHARS.len());
        out.push(TOKEN_CHARS[idx] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemStore;

    #[test]
    fn shared_defaults() {
        let s = SharedSettings::default();
        assert!(!s.muted);
        assert_eq!(s.sync_token, None);
        assert_eq!(s.sync_endpoint, None);
    }

    #[test]
    fn shared_load_empty_store_is_defaults() {
        let store = MemStore::new();
        assert_eq!(load_shared(&store), SharedSettings::default());
    }

    #[test]
    fn shared_roundtrip() {
        let mut store = MemStore::new();
        let s = SharedSettings {
            muted: true,
            sync_token: Some("abc123def456ghij".to_string()),
            sync_endpoint: Some("https://example.test".to_string()),
        };
        save_shared(&mut store, &s);
        assert_eq!(load_shared(&store), s);
    }

    #[test]
    fn shared_serializes_with_exact_keys() {
        // Populate every field: nanoserde omits `None` Options (harmless for
        // this local-only blob, which loads tolerantly), so to verify the exact
        // camelCase key names we serialize a fully-populated value.
        let s = SharedSettings {
            muted: true,
            sync_token: Some("tok".to_string()),
            sync_endpoint: Some("https://e.test".to_string()),
        };
        let json = s.serialize_json();
        assert!(json.contains("\"muted\""), "json: {json}");
        assert!(json.contains("\"syncToken\""), "json: {json}");
        assert!(json.contains("\"syncEndpoint\""), "json: {json}");
        // No snake_case leakage.
        assert!(!json.contains("sync_token"), "json: {json}");
        assert!(!json.contains("sync_endpoint"), "json: {json}");
    }

    #[test]
    fn shared_partial_blob_merges_over_defaults() {
        let mut store = MemStore::new();
        // Only `muted` present; sync fields absent → fall back to None.
        store.set(&ns_key("shared", "settings"), "{\"muted\":true}");
        let s = load_shared(&store);
        assert!(s.muted);
        assert_eq!(s.sync_token, None);
        assert_eq!(s.sync_endpoint, None);
    }

    #[test]
    fn shared_explicit_null_sync_falls_back_to_default() {
        let mut store = MemStore::new();
        store.set(
            &ns_key("shared", "settings"),
            "{\"muted\":false,\"syncToken\":null,\"syncEndpoint\":null}",
        );
        let s = load_shared(&store);
        assert_eq!(s, SharedSettings::default());
    }

    #[test]
    fn shared_garbage_blob_falls_back_to_defaults() {
        let mut store = MemStore::new();
        store.set(&ns_key("shared", "settings"), "not json at all");
        assert_eq!(load_shared(&store), SharedSettings::default());
    }

    #[test]
    fn patterns_defaults() {
        let s = PatternsSettings::default();
        assert_eq!(s.theme_choice, "mix");
        assert_eq!(s.difficulty, "auto");
        assert_eq!(s.mode, "next");
        assert!(!s.show_hint);
    }

    #[test]
    fn patterns_load_empty_is_defaults() {
        let store = MemStore::new();
        assert_eq!(load_patterns(&store), PatternsSettings::default());
    }

    #[test]
    fn patterns_partial_blob_only_overrides_present_fields() {
        let mut store = MemStore::new();
        store.set(
            &ns_key("patterns", "settings"),
            "{\"mode\":\"unit\",\"showHint\":true}",
        );
        let s = load_patterns(&store);
        assert_eq!(s.theme_choice, "mix"); // default kept
        assert_eq!(s.difficulty, "auto"); // default kept
        assert_eq!(s.mode, "unit"); // overridden
        assert!(s.show_hint); // overridden
    }

    #[test]
    fn patterns_roundtrip() {
        let mut store = MemStore::new();
        let s = PatternsSettings {
            theme_choice: "emoji-dinosaurs".to_string(),
            difficulty: "hard".to_string(),
            mode: "unit".to_string(),
            show_hint: true,
        };
        save_patterns(&mut store, &s);
        assert_eq!(load_patterns(&store), s);
    }

    #[test]
    fn patterns_serializes_with_exact_keys() {
        let json = PatternsSettings::default().serialize_json();
        assert!(json.contains("\"themeChoice\""), "json: {json}");
        assert!(json.contains("\"difficulty\""), "json: {json}");
        assert!(json.contains("\"mode\""), "json: {json}");
        assert!(json.contains("\"showHint\""), "json: {json}");
        assert!(!json.contains("theme_choice"), "json: {json}");
        assert!(!json.contains("show_hint"), "json: {json}");
    }

    #[test]
    fn singback_defaults() {
        assert_eq!(SingbackSettings::default().difficulty, "normal");
    }

    #[test]
    fn singback_load_empty_is_defaults() {
        let store = MemStore::new();
        assert_eq!(load_singback(&store), SingbackSettings::default());
    }

    #[test]
    fn singback_partial_blob_only_overrides_present_fields() {
        let mut store = MemStore::new();
        store.set(&ns_key("singback", "settings"), "{}"); // no keys
        assert_eq!(load_singback(&store).difficulty, "normal"); // default kept
        store.set(&ns_key("singback", "settings"), "{\"difficulty\":\"speedy\"}");
        assert_eq!(load_singback(&store).difficulty, "speedy"); // overridden
    }

    #[test]
    fn singback_roundtrip() {
        let mut store = MemStore::new();
        let s = SingbackSettings {
            difficulty: "gentle".to_string(),
        };
        save_singback(&mut store, &s);
        assert_eq!(load_singback(&store), s);
    }

    #[test]
    fn singback_serializes_with_exact_keys() {
        let json = SingbackSettings::default().serialize_json();
        assert!(json.contains("\"difficulty\""), "json: {json}");
    }

    #[test]
    fn clock_defaults() {
        assert_eq!(ClockSettings::default().difficulty, "match");
    }

    #[test]
    fn clock_load_empty_is_defaults() {
        let store = MemStore::new();
        assert_eq!(load_clock(&store), ClockSettings::default());
    }

    #[test]
    fn clock_partial_blob_only_overrides_present_fields() {
        let mut store = MemStore::new();
        store.set(&ns_key("clock", "settings"), "{}"); // no keys
        assert_eq!(load_clock(&store).difficulty, "match"); // default kept
        store.set(&ns_key("clock", "settings"), "{\"difficulty\":\"halfpast\"}");
        assert_eq!(load_clock(&store).difficulty, "halfpast"); // overridden
    }

    #[test]
    fn clock_roundtrip() {
        let mut store = MemStore::new();
        let s = ClockSettings { difficulty: "clock".to_string(), last_seen: 1748600100000 };
        save_clock(&mut store, &s);
        assert_eq!(load_clock(&store), s);
    }

    #[test]
    fn clock_serializes_with_exact_keys() {
        let json = ClockSettings { difficulty: "clock".to_string(), last_seen: 42 }.serialize_json();
        assert!(json.contains("\"difficulty\""), "json: {json}");
        assert!(json.contains("\"lastSeen\":42"), "json: {json}");
        assert!(!json.contains("last_seen"), "json: {json}");
    }

    #[test]
    fn clock_load_tolerates_absent_last_seen() {
        let mut store = MemStore::new();
        store.set(&ns_key("clock", "settings"), "{\"difficulty\":\"clock\"}");
        let s = load_clock(&store);
        assert_eq!((s.difficulty.as_str(), s.last_seen), ("clock", 0));
    }

    #[test]
    fn clock_merge_is_last_write_wins_by_timestamp() {
        let local = ClockSettings { difficulty: "match".to_string(), last_seen: 100 };
        let remote = ClockSettings { difficulty: "halfpast".to_string(), last_seen: 200 };
        // The newer timestamp wins, either merge order.
        assert_eq!(merge_clock(&local, &remote), remote);
        assert_eq!(merge_clock(&remote, &local), remote);
    }

    #[test]
    fn clock_merge_tie_break_is_commutative_and_idempotent() {
        let a = ClockSettings { difficulty: "clock".to_string(), last_seen: 50 };
        let b = ClockSettings { difficulty: "routine".to_string(), last_seen: 50 };
        // Equal timestamps → deterministic, order-independent pick.
        assert_eq!(merge_clock(&a, &b), merge_clock(&b, &a));
        // Idempotent.
        assert_eq!(merge_clock(&a, &a), a);
        let m = merge_clock(&a, &b);
        assert_eq!(merge_clock(&m, &a), m);
        assert_eq!(merge_clock(&m, &b), m);
    }

    #[test]
    fn clock_parse_garbage_is_defaults() {
        assert_eq!(parse_clock("not json"), ClockSettings::default());
        assert_eq!(parse_clock("{}"), ClockSettings::default());
    }

    #[test]
    fn token_length_and_charset() {
        let mut rng = Mulberry32::new(12345);
        let tok = generate_token(&mut rng);
        assert_eq!(tok.len(), 16);
        assert!(
            tok.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "token has out-of-charset char: {tok}"
        );
    }

    #[test]
    fn token_is_deterministic_per_seed() {
        let mut a = Mulberry32::new(42);
        let mut b = Mulberry32::new(42);
        assert_eq!(generate_token(&mut a), generate_token(&mut b));
    }

    #[test]
    fn token_matches_server_regex_bound() {
        // Server accepts [a-z0-9]{8,64}; 16 lowercase-alnum chars is in-bounds.
        let mut rng = Mulberry32::new(7);
        let tok = generate_token(&mut rng);
        assert!((8..=64).contains(&tok.len()));
    }
}
