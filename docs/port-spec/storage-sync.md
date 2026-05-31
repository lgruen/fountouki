# Storage & Sync — port spec

Goal: a Rust client that talks to the **same** Cloudflare Worker and
reads/writes the **same** localStorage (or equivalent) keys + blob
shapes as the current TS app, with zero server changes.

Source of truth: `src/shared/storage.ts`, `src/shared/settings.ts`,
`src/shared/sync.ts`, `src/games/*/`, `server/worker.ts`,
`server/wrangler.toml`, `server/README.md`.

---

## 1. Local storage

### 1.1 Key scheme

All keys are namespaced strings:

```
fountouki.<area>.<name>.<version>
```

- `NS` is the literal `fountouki`.
- `<version>` is always `v1` today (the code defaults `version = 'v1'`).
- `<area>` is `shared` for app-wide settings, or a game id otherwise.
- Values are **JSON**, stringified with `JSON.stringify`. Read with
  `JSON.parse`; any parse error or missing key → treat as `null`
  (errorless: fall back to defaults, never throw).
- Writes are best-effort: if the storage backend is blocked/full, the
  TS app swallows the error and continues. Mirror that — never crash
  gameplay on a storage failure.

Helper semantics to replicate:
- `load(area, name)` → parsed value or `null`.
- `save(area, name, value)` → `JSON.stringify` then set.
- `remove(area, name)` → delete the key.

### 1.2 Keys in use

| Key | Area | Name | Owner | Synced? |
|-----|------|------|-------|---------|
| `fountouki.shared.settings.v1` | `shared` | `settings` | shared settings (mute + sync config) | no (local only) |
| `fountouki.patterns.settings.v1` | `patterns` | `settings` | patterns game prefs | no (local only) |
| `fountouki.phonics.state.v1` | `phonics` | `state` | phonics SRS state | **yes** (sync game id `phonics`) |

Notes:
- Game ids come from `src/games/registry.ts`: `patterns`, `phonics`.
- The sync **game segment** equals the storage **area** for synced
  games (phonics: area `phonics`, sync path `/<token>/phonics`).
- Scores / session counters (stars, streak, level) are **never
  persisted** — they live in memory only and reset each mount.

### 1.3 Shared settings blob

Key `fountouki.shared.settings.v1`. Shape (`SharedSettings`):

```jsonc
{
  "muted": false,          // bool — single app-wide mute toggle
  "syncToken": null,       // string | null — family sync namespace
  "syncEndpoint": null     // string | null — endpoint override; null = default
}
```

- Load = `{ ...DEFAULTS, ...stored }` so missing fields fall back to
  defaults `{ muted:false, syncToken:null, syncEndpoint:null }`.
- Save = merge patch over current, then write the whole object.
- `muted` is the **only** mute toggle in the whole app (shared across
  games).
- `syncToken` is the **family-level** sync token (one token spans all
  games). `null`/empty = sync disabled.
- `syncEndpoint` overrides the default Worker URL; `null`/empty = use
  the default endpoint (see §3). Empty-string is also treated as "use
  default" (`s.syncEndpoint || DEFAULT_ENDPOINT`).

### 1.4 Sync token generation

When the parent UI mints a token (`generateToken`): 16 chars drawn from
`abcdefghijklmnopqrstuvwxyz0123456789` (lowercase + digits), via
`crypto.getRandomValues` over 16 bytes, each byte mapped `byte % 36`.
~82 bits. Must satisfy the server token regex (§3.3). A Rust client
should generate equivalent tokens (16 lowercase-alnum chars) so they
stay within the server's `[a-z0-9]{8,64}` bound.

### 1.5 Patterns settings blob

Key `fountouki.patterns.settings.v1`. All fields optional on load
(only applied if present / right type):

```jsonc
{
  "themeChoice": "mix",   // see enum below
  "difficulty": "auto",   // 'easy' | 'hard' | 'auto'
  "mode": "next",         // 'next' | 'unit'
  "showHint": false       // bool
}
```

`themeChoice` enum:
`'mix' | 'emoji-animals' | 'emoji-fruit' | 'emoji-vehicles' |
'emoji-construction' | 'emoji-dinosaurs' | 'shapes' | 'letters-upper'
| 'letters-lower' | 'numbers'`.

Defaults if absent: `themeChoice='mix'`, `difficulty='auto'`,
`mode='next'`, `showHint=false`. Load only overwrites a default when
the field is present (and, for `showHint`, a boolean).

### 1.6 Phonics state blob (also the sync blob — see §2)

Key `fountouki.phonics.state.v1`. Shape (`PhonicsState`):

```jsonc
{
  "schemaVersion": 1,      // must equal SCHEMA_VERSION (=1) or whole blob is discarded
  "version": 0,            // monotonic counter, +1 on every grade
  "letters": {
    "a": { "box": 0, "due": 1748600000000, "lastSeen": 0 },
    "s": { "box": 2, "due": 1748600900000, "lastSeen": 1748600100000 }
    // ... one entry per letter that has state
  }
}
```

`LetterState`:
- `box`: int 0..4 (`MAX_BOX = 4`). 0 = new/missed, 4 = mastered.
- `due`: epoch **ms**; letter is ready when `due <= now`.
- `lastSeen`: epoch ms of last grade (0 = never graded).

Validation (`validate`) — return `null` (→ fresh empty state) unless
**all** hold:
- value is a non-null object,
- `schemaVersion === 1`,
- `version` is a number,
- `letters` is a non-null object.
On success, keep only `{ schemaVersion:1, letters, version }` (drops
any extra fields).

`emptyState()` = `{ schemaVersion:1, letters:{}, version:0 }`.

`ensureLetters(state)`: for every letter in the deck `LETTERS` (the 26
lowercase a–z), if missing, add `{ box:0, due:now, lastSeen:0 }`.
Called right after load and after every merge.

SRS mechanics (needed only if the Rust client also runs phonics game
logic, not for raw sync passthrough):
- `intervalFor(box)`: 0→`0`, 1→`2 min`, 2→`15 min`, 3→`6 h`,
  else→`24 h` (ms).
- `gotIt`: `box = min(4, box+1)`, `due = now + intervalFor(box)`,
  `lastSeen = now`, `version += 1`.
- `missed`: `box = max(0, box-1)`, `due = now + intervalFor(box)`,
  `lastSeen = now`, `version += 1`.
- Intro order (`INTRO_ORDER`, gates which letters are active):
  `s a t i p n c k e h r m d g o u l f b j z w v y x q`.

---

## 2. Sync wire protocol (client side)

Implemented in `src/shared/sync.ts`. One opaque **family token** spans
all games; the path is `/<token>/<game>`. Config (endpoint + token) is
re-read from shared settings on **every** call, so token/endpoint
changes mid-session take effect immediately. If `syncToken` is empty,
sync is a no-op (`pull`→`null`, `push`→nothing, `flush`→nothing,
`configured`→false).

`base = syncEndpoint || DEFAULT_ENDPOINT`; URL = `${base}/${token}/${game}`.

### 2.1 pull(game) → blob | null

- `GET ${base}/${token}/${game}` (no special headers).
- If `!response.ok` → `null`.
- Read body as text. If body is empty `""` **or** exactly `"{}"` →
  treat as "no data" → `null` (the server returns `{}` for an empty
  key; the client maps that to null).
- Otherwise `JSON.parse(text)` and return it. Parse error → `null`.
- Any network error → `null` (never throws).

### 2.2 push(game, blob) — debounced

- No-op if not configured.
- **Debounce 500 ms per game** (`DEBOUNCE_MS = 500`): each `push` for a
  game cancels that game's pending timer and schedules a new one; only
  the **last** blob within the 500 ms window is sent. Pending pushes are
  keyed by game (a `Map<game, {blob, timer}>`).
- On fire: `PUT ${base}/${token}/${game}` with header
  `content-type: application/json` and body `JSON.stringify(blob)`.
- The captured config (endpoint+token) is the one read at `push`-call
  time, not at fire time. Network errors are swallowed (best-effort —
  don't crash gameplay if offline).

### 2.3 flush() — send pending now

- No-op if not configured.
- Snapshot all pending entries, clear the pending map, cancel each
  timer, and `PUT` them all immediately (in parallel). Awaitable.
- **Flush-on-hide triggers** (`src/main.ts`): called on
  `document visibilitychange` when `visibilityState === 'hidden'`, and
  on `window pagehide`. Phonics also calls `sync.flush()` from its own
  unmount. A Rust/native client should flush on the equivalent
  app-background / will-resign-active / unmount events so the last grade
  or two isn't dropped when a kid bails to the home screen.

### 2.4 configured() → bool

True iff a non-empty `syncToken` is present in shared settings.

### 2.5 Per-game blob model + conflict/merge

- The server stores **one opaque JSON blob per `(token, game)`**. It is
  last-write-wins at the storage layer; the server does no merging.
- **Merge is entirely client-side** and game-specific. Only phonics
  merges today (`src/games/phonics/srs.ts merge()`), run on the pull
  result against current local state:
  - Per-letter winner = the entry with the larger `lastSeen`
    (ties keep local `a`). Letters present on only one side are kept.
  - Result `version = max(local.version, remote.version)`.
  - `schemaVersion` reset to 1.
  - After merge: `ensureLetters`, persist locally, rebuild the play
    queue (don't yank the kid mid-card).
- Phonics push/persist on every grade: it calls `save(...)` **and**
  `sync.push('phonics', state)` after each `gotIt`/`missed`.
- Patterns settings are **not** synced — local only.
- A Rust client porting phonics must implement this same lastSeen-wins,
  max-version merge to interoperate with TS clients on the same token.

### 2.6 Wire shapes summary

```
GET  /<token>/<game>
  → 200, content-type application/json
  → body: the stored JSON, or "{}" if the key is empty
  (client maps "" or "{}" to null)

PUT  /<token>/<game>
  headers: content-type: application/json
  body:    <the game blob as JSON>
  → 200 {"ok":true} on success
  → 400 "not json"  if body isn't valid JSON
  → 413 "too large" if body > 64 KiB
```

The client ignores the PUT response body entirely (best-effort fire).

---

## 3. Server (Cloudflare Worker)

Source: `server/worker.ts`, `server/wrangler.toml`,
`server/README.md`. **Do not change it** — the Rust client conforms.

### 3.1 Endpoint

- Live URL / `DEFAULT_ENDPOINT`:
  `https://fountouki-sync.fountouki.workers.dev`
  (hardcoded in `sync.ts`; overridable per-device via
  `syncEndpoint`).
- Worker name: `fountouki-sync` (`wrangler.toml`).

### 3.2 Storage backend

- **Workers KV**, binding `STORE`
  (`[[kv_namespaces]] binding = "STORE"`, id
  `9431a456706247dbb0de5e8406d7b8e5`).
- Storage key inside KV: **`${token}:${game}`** (colon-joined — note:
  *not* the slash path, and *not* the localStorage scheme).
- No Durable Objects. KV is eventually consistent — two devices can
  briefly read stale data; the client merge (§2.5) is what reconciles.

### 3.3 Routes & validation

- Path must be exactly two non-empty segments: `/<token>/<game>`
  (`pathname.split('/').filter(Boolean).length === 2`), else `404
  "not found"`.
- `token` must match `/^[a-z0-9]{8,64}$/i` (8–64 alphanumeric,
  case-insensitive), else `400 "bad request"`.
- `game` must match `/^[a-z0-9-]{1,32}$/i` (1–32 lowercase letters,
  digits, hyphen; case-insensitive), else `400 "bad request"`.
- `OPTIONS` (any path) → `204` with CORS headers (preflight).
- `GET` → `200`, `content-type: application/json`, body = stored value
  or `"{}"`.
- `PUT` → reads body text; `413 "too large"` if `> 64*1024` bytes;
  `JSON.parse` validates, `400 "not json"` on failure; else
  `STORE.put(key, body)` and return `200 {"ok":true}`.
- Any other method → `405 "method not allowed"`.

CORS (on every response incl. GET/PUT/OPTIONS):
```
access-control-allow-origin: *
access-control-allow-methods: GET, PUT, OPTIONS
access-control-allow-headers: content-type
access-control-max-age: 86400
```
(A native Rust client doesn't need CORS, but the headers are present.)

### 3.4 Auth model

- **No auth header.** The token in the path *is* the namespace **and**
  the password — anyone who knows the token can read+write that
  family's blobs. Deliberate: it's game state, not PII; tampering is
  acceptable per the maintainer.

### 3.5 Limits / abuse

- Body cap **64 KiB** (`413` over).
- Designed to live on the Workers **free plan**: ~100K req/day, ~1K KV
  writes/day; over cap → requests start failing with 429 (no surprise
  bill). The 500 ms push debounce + flush-on-hide keeps write volume
  low; a Rust client should keep the same debounce to stay under the KV
  write cap.
- Optional CF dashboard rate-limit rule (e.g. 10 req / 10 s per IP) is
  mentioned as belt-and-braces; not enforced in `worker.ts`.

### 3.6 Smoke test (verbatim from README)

```
URL=https://fountouki-sync.fountouki.workers.dev
T=abcd1234
curl $URL/$T/test                                # {}
curl -X PUT $URL/$T/test -H 'content-type: application/json' -d '{"x":1}'
curl $URL/$T/test                                # {"x":1}
```

---

## 4. Legacy migration

`migrateLegacy()` (`storage.ts`), run once at boot **before**
`applyOnBoot` in `src/main.ts`:

- Move list (one entry today): legacy key `patternplay.settings.v1` →
  new key `fountouki.patterns.settings.v1`.
- Per move: if the **new** key already exists, skip. Else if the legacy
  key exists, copy its raw value to the new key and delete the legacy
  key. (Raw value copy — no re-encode.)
- All wrapped in try/catch; failures are ignored.

A Rust client that starts from a fresh install can skip this (no
`patternplay.*` keys will exist), but if it shares a device with an old
TS install it should perform the same one-time move to pick up legacy
patterns prefs.

---

## 5. Boot order (for parity)

From `src/main.ts`:
1. `migrateLegacy()` — legacy key moves (§4).
2. `applyOnBoot()` — load shared settings, apply `muted` to the audio
   layer.
3. Mount the routed game; phonics fires a background `sync.pull` +
   merge after mount.
4. Register flush-on-hide (`visibilitychange` hidden, `pagehide`).
