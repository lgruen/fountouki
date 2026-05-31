//! Hash routing — pure parity with the TS app's `router.ts`.
//!
//! Two routes only: `Picker` (home grid) and `Game(id)` (a mounted game).
//! Grammar (web parity): `^#/([a-z0-9-]+)` case-insensitive. On match the
//! captured id is lowercased → `Game(id)`. Anything else (bare `#/`, `#`,
//! empty, garbage) → `Picker` (silent unknown-route fallback).
//!
//! `parse_hash` / `hash_for` are kept pure so native ports (no URL bar) can
//! reuse them for deep-link / state-restore, and golden tests can assert the
//! grammar bit-for-bit.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Picker,
    Game(String),
}

/// Parse a location hash into a `Route`.
///
/// Mirrors the regex `^#/([a-z0-9-]+)` (case-insensitive). The id is the
/// longest run of `[A-Za-z0-9-]` immediately after a leading `#/`, lowercased.
/// Note: the original regex is *not* end-anchored, so trailing junk (query
/// params, extra path segments) after a valid id is ignored — only the first
/// run is captured. Anything that doesn't match → `Picker`.
pub fn parse_hash(hash: &str) -> Route {
    // Must start with the literal "#/".
    let rest = match hash.strip_prefix("#/") {
        Some(r) => r,
        None => return Route::Picker,
    };

    // Capture the leading [A-Za-z0-9-]+ run (regex group 1, un-anchored tail).
    let mut end = 0;
    for c in rest.chars() {
        if is_id_char(c) {
            end += c.len_utf8();
        } else {
            break;
        }
    }

    if end == 0 {
        // Empty capture group (e.g. bare "#/", or "#/?foo") → no match.
        return Route::Picker;
    }

    Route::Game(rest[..end].to_lowercase())
}

/// Format a `Route` back into a location hash. Inverse of `parse_hash` for
/// any already-lowercased id.
pub fn hash_for(route: &Route) -> String {
    match route {
        Route::Picker => "#/".to_string(),
        Route::Game(id) => format!("#/{}", id),
    }
}

/// Regex char class `[a-z0-9-]` with the case-insensitive flag.
#[inline]
fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_game_id() {
        assert_eq!(parse_hash("#/patterns"), Route::Game("patterns".into()));
        assert_eq!(parse_hash("#/phonics"), Route::Game("phonics".into()));
    }

    #[test]
    fn lowercases_the_id() {
        assert_eq!(parse_hash("#/Patterns"), Route::Game("patterns".into()));
        assert_eq!(parse_hash("#/PHONICS"), Route::Game("phonics".into()));
        assert_eq!(parse_hash("#/PaTtErNs"), Route::Game("patterns".into()));
    }

    #[test]
    fn accepts_digits_and_hyphens() {
        assert_eq!(parse_hash("#/game-2"), Route::Game("game-2".into()));
        assert_eq!(parse_hash("#/abc-123-xyz"), Route::Game("abc-123-xyz".into()));
        assert_eq!(parse_hash("#/123"), Route::Game("123".into()));
    }

    #[test]
    fn bare_and_empty_hashes_fall_back_to_picker() {
        assert_eq!(parse_hash("#/"), Route::Picker);
        assert_eq!(parse_hash("#"), Route::Picker);
        assert_eq!(parse_hash(""), Route::Picker);
    }

    #[test]
    fn garbage_falls_back_to_picker() {
        // No leading "#/".
        assert_eq!(parse_hash("/patterns"), Route::Picker);
        assert_eq!(parse_hash("patterns"), Route::Picker);
        assert_eq!(parse_hash("##/patterns"), Route::Picker);
        // Leading char not in the id class → empty capture.
        assert_eq!(parse_hash("#/?foo=bar"), Route::Picker);
        assert_eq!(parse_hash("#/ space"), Route::Picker);
        assert_eq!(parse_hash("#/_underscore"), Route::Picker);
    }

    #[test]
    fn trailing_junk_after_id_is_ignored() {
        // The TS regex is not end-anchored: only the first id run is captured.
        assert_eq!(parse_hash("#/patterns/extra"), Route::Game("patterns".into()));
        assert_eq!(parse_hash("#/patterns?q=1"), Route::Game("patterns".into()));
        assert_eq!(parse_hash("#/phonics#again"), Route::Game("phonics".into()));
    }

    #[test]
    fn hash_for_picker_and_game() {
        assert_eq!(hash_for(&Route::Picker), "#/");
        assert_eq!(hash_for(&Route::Game("patterns".into())), "#/patterns");
        assert_eq!(hash_for(&Route::Game("game-2".into())), "#/game-2");
    }

    #[test]
    fn roundtrip_picker() {
        assert_eq!(parse_hash(&hash_for(&Route::Picker)), Route::Picker);
    }

    #[test]
    fn roundtrip_game_ids() {
        for id in ["patterns", "phonics", "game-2", "abc-123"] {
            let route = Route::Game(id.to_string());
            assert_eq!(parse_hash(&hash_for(&route)), route);
        }
    }
}
