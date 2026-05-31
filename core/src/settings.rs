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
#[derive(Debug, Clone, PartialEq, Eq, SerJson, DeJson)]
pub struct SharedSettings {
    #[nserde(rename = "muted")]
    pub muted: bool,
    #[nserde(rename = "syncToken")]
    pub sync_token: Option<String>,
    #[nserde(rename = "syncEndpoint")]
    pub sync_endpoint: Option<String>,
}

impl Default for SharedSettings {
    fn default() -> Self {
        Self {
            muted: false,
            sync_token: None,
            sync_endpoint: None,
        }
    }
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
