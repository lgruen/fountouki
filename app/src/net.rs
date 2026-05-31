//! Cross-device sync transport (the Cloudflare Worker KV store). Poll-based HTTP
//! via quad-net so it works in the macroquad WASM build (browser XHR/fetch) and
//! native alike. The wire protocol + last-seen-wins merge live in
//! `fountouki_core::sync` / `srs`; this just drives requests across frames.
//! Everything is best-effort — it never blocks or crashes gameplay if offline.
use crate::store::Db;
use fountouki_core::settings;
use fountouki_core::sync::{self, Debouncer};
use quad_net::http_request::{Method, Request, RequestBuilder};

pub struct SyncClient {
    db: Db,
    game: &'static str,
    pull: Option<Request>,
    pulled: bool,
    pushes: Vec<Request>,
    deb: Debouncer,
}

impl SyncClient {
    /// Create and kick off the initial pull (if a family sync token is set).
    pub fn new(db: Db, game: &'static str) -> SyncClient {
        let mut c = SyncClient {
            db,
            game,
            pull: None,
            pulled: false,
            pushes: Vec::new(),
            deb: Debouncer::new(),
        };
        c.begin_pull();
        c
    }

    /// (endpoint, token) if sync is configured, re-read fresh each call so
    /// token/endpoint edits in the parent menu take effect mid-session.
    fn cfg(&self) -> Option<(String, String)> {
        let s = {
            let kv = self.db.borrow_kv();
            settings::load_shared(&**kv)
        };
        let token = s.sync_token.filter(|t| !t.is_empty())?;
        let endpoint = s
            .sync_endpoint
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| sync::DEFAULT_ENDPOINT.to_string());
        Some((endpoint, token))
    }

    fn begin_pull(&mut self) {
        if let Some((ep, token)) = self.cfg() {
            let url = sync::sync_url(Some(&ep), &token, self.game);
            self.pull = Some(RequestBuilder::new(&url).send());
        } else {
            self.pulled = true; // not configured — nothing to pull
        }
    }

    /// Returns the remote blob exactly once, when the initial pull completes.
    /// The caller merges it into local state.
    pub fn poll_pull(&mut self) -> Option<String> {
        if self.pulled {
            return None;
        }
        if let Some(req) = self.pull.as_mut() {
            if let Some(result) = req.try_recv() {
                self.pulled = true;
                self.pull = None;
                if let Ok(body) = result {
                    return sync::interpret_pull(&body);
                }
            }
        }
        None
    }

    /// Queue a debounced push of the latest blob (coalesced over 500ms).
    pub fn queue_push(&mut self, blob: &str, now: i64) {
        if self.cfg().is_some() {
            self.deb.push(self.game, blob, now);
        }
    }

    /// Fire any due pushes + reap finished requests. Call every frame.
    pub fn drive(&mut self, now: i64) {
        for (_game, blob) in self.deb.take_due(now) {
            self.send_put(&blob);
        }
        self.pushes.retain_mut(|r| r.try_recv().is_none());
    }

    /// Send all pending pushes immediately (on leaving the game).
    pub fn flush(&mut self) {
        for (_game, blob) in self.deb.take_all() {
            self.send_put(&blob);
        }
    }

    fn send_put(&mut self, blob: &str) {
        if let Some((ep, token)) = self.cfg() {
            let url = sync::sync_url(Some(&ep), &token, self.game);
            let req = RequestBuilder::new(&url)
                .method(Method::Put)
                .header("Content-Type", "application/json")
                .body(blob)
                .send();
            self.pushes.push(req);
        }
    }
}
