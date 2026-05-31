//! Sync client — pure parity with the TS app's `sync.ts` (wire side only).
//!
//! One opaque **family token** spans all games; request path is
//! `<endpoint>/<token>/<game>`. Everything network/timer-shaped is left to the
//! host: real HTTP behind [`SyncTransport`], real timers driving [`Debouncer`]
//! with a host-supplied `now`. This module only computes URLs, interprets pull
//! bodies, and coalesces pending writes (500 ms debounce per game) — no wall
//! clock, no sockets.

use std::collections::HashMap;

/// Default Cloudflare Worker endpoint. Overridable per-device via the shared
/// `syncEndpoint` setting (empty / None → this default).
pub const DEFAULT_ENDPOINT: &str = "https://fountouki-sync.fountouki.workers.dev";

/// Debounce window for per-game pushes (ms). Coalesces bursts of grades into
/// one PUT and keeps write volume under the KV free-plan cap.
pub const DEBOUNCE_MS: i64 = 500;

/// Build the request URL for `(endpoint, token, game)`:
/// `{endpoint || DEFAULT_ENDPOINT}/{token}/{game}`. An empty endpoint override
/// also falls back to the default (matches TS `s.syncEndpoint || DEFAULT`).
pub fn sync_url(endpoint: Option<&str>, token: &str, game: &str) -> String {
    let base = match endpoint {
        Some(e) if !e.is_empty() => e,
        _ => DEFAULT_ENDPOINT,
    };
    format!("{}/{}/{}", base, token, game)
}

/// Interpret a GET body from `pull`. The server returns `"{}"` for an empty
/// key; the client maps that — and an empty / whitespace-only body — to "no
/// data" (`None`). Any other body is returned verbatim as `Some` (caller
/// JSON-parses it; a parse failure there is its own `None`).
pub fn interpret_pull(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        None
    } else {
        Some(body.to_string())
    }
}

/// Host-provided HTTP transport. Both methods are best-effort: an offline /
/// erroring host should return `None` (GET) or silently drop (PUT) — never
/// crash gameplay. The PUT response body is ignored entirely.
pub trait SyncTransport {
    fn get(&self, url: &str) -> Option<String>;
    fn put(&self, url: &str, body: &str);
}

/// One pending push: the latest blob for a game and the time it becomes due.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    blob: String,
    due_at_ms: i64,
}

/// Per-game debounce/coalesce layer for outbound pushes.
///
/// Each [`push`](Debouncer::push) for a game replaces that game's pending blob
/// and resets its due time to `now + DEBOUNCE_MS`, so a burst within the window
/// collapses to a single entry carrying the last blob. The host drains it via
/// [`take_due`](Debouncer::take_due) on a timer tick and [`take_all`](Debouncer::take_all)
/// on background / unmount (flush). Draining removes the returned entries.
#[derive(Debug, Default)]
pub struct Debouncer {
    pending: HashMap<String, Pending>,
}

impl Debouncer {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Queue (or replace) the pending blob for `game`, due at `now + DEBOUNCE_MS`.
    pub fn push(&mut self, game: &str, blob: &str, now: i64) {
        self.pending.insert(
            game.to_string(),
            Pending {
                blob: blob.to_string(),
                due_at_ms: now + DEBOUNCE_MS,
            },
        );
    }

    /// Remove and return every entry whose due time has arrived (`due_at <= now`),
    /// as `(game, blob)`. Sorted by game id for deterministic flush order.
    pub fn take_due(&mut self, now: i64) -> Vec<(String, String)> {
        let mut due: Vec<(String, String)> = self
            .pending
            .iter()
            .filter(|(_, p)| p.due_at_ms <= now)
            .map(|(g, p)| (g.clone(), p.blob.clone()))
            .collect();
        due.sort_by(|a, b| a.0.cmp(&b.0));
        for (g, _) in &due {
            self.pending.remove(g);
        }
        due
    }

    /// Remove and return all pending entries regardless of due time (flush), as
    /// `(game, blob)`, sorted by game id.
    pub fn take_all(&mut self) -> Vec<(String, String)> {
        let mut all: Vec<(String, String)> = self
            .pending
            .drain()
            .map(|(g, p)| (g, p.blob))
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    }

    /// True if any push is pending.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_uses_default_endpoint_when_none() {
        assert_eq!(
            sync_url(None, "tok", "phonics"),
            "https://fountouki-sync.fountouki.workers.dev/tok/phonics"
        );
    }

    #[test]
    fn url_uses_default_endpoint_when_empty() {
        assert_eq!(
            sync_url(Some(""), "tok", "phonics"),
            "https://fountouki-sync.fountouki.workers.dev/tok/phonics"
        );
    }

    #[test]
    fn url_uses_override_endpoint() {
        assert_eq!(
            sync_url(Some("https://example.test"), "abc123de", "patterns"),
            "https://example.test/abc123de/patterns"
        );
    }

    #[test]
    fn interpret_pull_maps_empty_and_braces_to_none() {
        assert_eq!(interpret_pull(""), None);
        assert_eq!(interpret_pull("   "), None);
        assert_eq!(interpret_pull("{}"), None);
        assert_eq!(interpret_pull("  {}  "), None);
        assert_eq!(interpret_pull("\n{}\n"), None);
    }

    #[test]
    fn interpret_pull_returns_real_body_verbatim() {
        assert_eq!(
            interpret_pull("{\"version\":3}"),
            Some("{\"version\":3}".to_string())
        );
        // Non-empty, non-"{}" payloads are returned as-is (caller parses).
        assert_eq!(interpret_pull("{ }"), Some("{ }".to_string()));
    }

    #[test]
    fn debounce_single_push_due_after_window() {
        let mut d = Debouncer::new();
        d.push("phonics", "blob1", 1000);
        // Not yet due.
        assert!(d.take_due(1000).is_empty());
        assert!(d.take_due(1499).is_empty());
        // Due exactly at now + DEBOUNCE_MS.
        assert_eq!(d.take_due(1500), vec![("phonics".into(), "blob1".into())]);
        // Drained.
        assert!(!d.has_pending());
        assert!(d.take_due(9999).is_empty());
    }

    #[test]
    fn debounce_coalesces_two_pushes_same_game_to_last_blob() {
        let mut d = Debouncer::new();
        d.push("phonics", "first", 1000); // due 1500
        d.push("phonics", "second", 1300); // resets due to 1800, replaces blob
                                            // At 1500 the reset window hasn't elapsed → nothing due.
        assert!(d.take_due(1500).is_empty());
        // One coalesced entry carrying the last blob.
        let due = d.take_due(1800);
        assert_eq!(due, vec![("phonics".into(), "second".into())]);
    }

    #[test]
    fn debounce_keeps_separate_games_separate() {
        let mut d = Debouncer::new();
        d.push("phonics", "p", 1000);
        d.push("patterns", "q", 1000);
        let mut due = d.take_due(1500);
        due.sort();
        assert_eq!(
            due,
            vec![
                ("patterns".into(), "q".into()),
                ("phonics".into(), "p".into())
            ]
        );
    }

    #[test]
    fn take_all_flushes_regardless_of_due_time() {
        let mut d = Debouncer::new();
        d.push("phonics", "p", 1000); // due 1500
        d.push("patterns", "q", 1000); // due 1500
                                       // Flush well before due.
        let all = d.take_all();
        assert_eq!(
            all,
            vec![
                ("patterns".into(), "q".into()),
                ("phonics".into(), "p".into())
            ]
        );
        assert!(!d.has_pending());
    }

    #[test]
    fn take_due_leaves_not_yet_due_entries() {
        let mut d = Debouncer::new();
        d.push("a", "x", 1000); // due 1500
        d.push("b", "y", 1100); // due 1600
        let due = d.take_due(1500);
        assert_eq!(due, vec![("a".into(), "x".into())]);
        // "b" still pending.
        assert!(d.has_pending());
        assert_eq!(d.take_due(1600), vec![("b".into(), "y".into())]);
    }
}
