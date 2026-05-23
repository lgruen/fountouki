# sync worker

CF Worker fronting Workers KV. Per-family namespace, all games under one
token. See `worker.ts`.

Live URL: `https://fountouki-sync.fountouki.workers.dev`

## Deploy

```
cd server
npx wrangler kv namespace create fountouki_sync   # one-time; copy id into wrangler.toml
npx wrangler deploy
```

`wrangler login` first if not authed. Workers.dev subdomain registration
is also one-time per account — if it's missing, set it via
`PUT /accounts/<id>/workers/subdomain` (or the dashboard).

## API

- `GET /<token>/<game>` → JSON blob (or `{}`).
- `PUT /<token>/<game>` → store JSON. Max 64 KB. Validates JSON.
- `OPTIONS` → CORS preflight.

Token: 8–64 alphanumeric. Game: lowercase + hyphen, ≤32. No auth header
— token in path is namespace + "password".

## Smoke test

```
URL=https://fountouki-sync.fountouki.workers.dev
T=abcd1234
curl $URL/$T/test                               # {}
curl -X PUT $URL/$T/test -H 'content-type: application/json' -d '{"x":1}'
curl $URL/$T/test                               # {"x":1}
```

## Cost / abuse

Stay on the **Workers free plan**. It has a hard daily cap (100K req/day,
1K KV writes/day) and once you hit it requests start failing with 429 —
no surprise bill. Only opt into paid Workers if you've decided you want
to allow more traffic.

Optional belt-and-braces: add a CF dashboard Rate-Limit rule on the
worker route (Security → Rate Limit, e.g. 10 req / 10s per IP) — free
tier covers it. Path validation + body cap + method allow-list in
`worker.ts` already block most scanning.
