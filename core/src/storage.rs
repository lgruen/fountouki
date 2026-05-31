//! Namespaced key/value storage — pure parity with the TS app's `storage.ts`.
//!
//! Keys are `fountouki.<area>.<name>.v1`. `<area>` is `shared` for app-wide
//! settings, or a game id otherwise. Values are JSON strings. The host
//! provides the actual backend (localStorage on web; plist / SharedPrefs /
//! file native) behind [`KeyValueStore`]; this module is just the key scheme +
//! the one-time legacy migration table. The exact key strings are load-bearing
//! — installed devices already hold data under them.

use std::collections::HashMap;

/// Host-provided key/value backend. Reads/writes are best-effort: a blocked or
/// full backend should silently no-op (the TS app swallows storage errors and
/// keeps gameplay alive), so implementors must not panic.
pub trait KeyValueStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, val: &str);
    fn remove(&mut self, key: &str);
}

/// Build the namespaced key for an `(area, name)` pair: `fountouki.{area}.{name}.v1`.
///
/// `<version>` is always `v1` today (the TS code defaults `version = 'v1'`).
pub fn ns_key(area: &str, name: &str) -> String {
    format!("fountouki.{}.{}.v1", area, name)
}

/// One-time legacy key moves, run at boot **before** `apply_on_boot`.
///
/// Per entry: if the destination key already exists, skip. Else if the source
/// key exists, copy its raw value (no re-encode) to the destination and remove
/// the source. Failures are silently ignored (the backend's no-op semantics).
///
/// Move table (one entry today): `patternplay.settings.v1` →
/// `fountouki.patterns.settings.v1`.
pub fn migrate_legacy<S: KeyValueStore + ?Sized>(store: &mut S) {
    for (src, dest) in LEGACY_MOVES {
        // Destination already populated → leave it; never clobber newer data.
        if store.get(dest).is_some() {
            continue;
        }
        // Raw value copy, then drop the legacy key.
        if let Some(val) = store.get(src) {
            store.set(dest, &val);
            store.remove(src);
        }
    }
}

/// `(legacy_key, new_key)` moves performed by [`migrate_legacy`].
const LEGACY_MOVES: &[(&str, &str)] = &[("patternplay.settings.v1", "fountouki.patterns.settings.v1")];

/// In-memory [`KeyValueStore`] for tests (and any host that wants a default).
#[derive(Debug, Default, Clone)]
pub struct MemStore(pub HashMap<String, String>);

impl MemStore {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl KeyValueStore for MemStore {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set(&mut self, key: &str, val: &str) {
        self.0.insert(key.to_string(), val.to_string());
    }

    fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ns_key_format() {
        assert_eq!(ns_key("shared", "settings"), "fountouki.shared.settings.v1");
        assert_eq!(ns_key("patterns", "settings"), "fountouki.patterns.settings.v1");
        assert_eq!(ns_key("phonics", "state"), "fountouki.phonics.state.v1");
    }

    #[test]
    fn memstore_roundtrip() {
        let mut s = MemStore::new();
        assert_eq!(s.get("k"), None);
        s.set("k", "v");
        assert_eq!(s.get("k"), Some("v".to_string()));
        s.set("k", "v2");
        assert_eq!(s.get("k"), Some("v2".to_string()));
        s.remove("k");
        assert_eq!(s.get("k"), None);
    }

    #[test]
    fn migrate_moves_legacy_when_dest_absent() {
        let mut s = MemStore::new();
        s.set("patternplay.settings.v1", "{\"mode\":\"unit\"}");
        migrate_legacy(&mut s);
        // Copied to new key, raw value preserved.
        assert_eq!(
            s.get("fountouki.patterns.settings.v1"),
            Some("{\"mode\":\"unit\"}".to_string())
        );
        // Legacy key removed.
        assert_eq!(s.get("patternplay.settings.v1"), None);
    }

    #[test]
    fn migrate_skips_when_dest_exists() {
        let mut s = MemStore::new();
        s.set("patternplay.settings.v1", "LEGACY");
        s.set("fountouki.patterns.settings.v1", "NEW");
        migrate_legacy(&mut s);
        // Destination untouched; legacy key left as-is (no copy, no remove).
        assert_eq!(
            s.get("fountouki.patterns.settings.v1"),
            Some("NEW".to_string())
        );
        assert_eq!(s.get("patternplay.settings.v1"), Some("LEGACY".to_string()));
    }

    #[test]
    fn migrate_is_noop_on_fresh_install() {
        let mut s = MemStore::new();
        migrate_legacy(&mut s);
        assert!(s.0.is_empty());
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut s = MemStore::new();
        s.set("patternplay.settings.v1", "X");
        migrate_legacy(&mut s);
        let after_first = s.clone();
        migrate_legacy(&mut s);
        // Second run sees the destination present → no further change.
        assert_eq!(s.0, after_first.0);
    }
}
