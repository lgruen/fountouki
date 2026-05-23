// Cross-device sync for fountouki games.
//
// GET  /<token>/<game>  -> JSON blob (or {} if none)
// PUT  /<token>/<game>  -> store JSON. Max 64 KB.
//
// Token is the namespace AND the "password". Knowledge of it = read/write.
// No auth header. Leo's call — game state, not identifying info, tampering
// is acceptable.

export interface Env {
  STORE: KVNamespace;
}

const TOKEN_RE = /^[a-z0-9]{8,64}$/i;
const GAME_RE = /^[a-z0-9-]{1,32}$/i;
const MAX_BODY_BYTES = 64 * 1024;

const cors: Record<string, string> = {
  "access-control-allow-origin": "*",
  "access-control-allow-methods": "GET, PUT, OPTIONS",
  "access-control-allow-headers": "content-type",
  "access-control-max-age": "86400",
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...cors, "content-type": "application/json" },
  });
}

function text(body: string, status: number): Response {
  return new Response(body, { status, headers: cors });
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    if (req.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: cors });
    }

    const url = new URL(req.url);
    const parts = url.pathname.split("/").filter(Boolean);
    if (parts.length !== 2) return text("not found", 404);
    const [token, game] = parts;
    if (!TOKEN_RE.test(token) || !GAME_RE.test(game)) {
      return text("bad request", 400);
    }
    const key = `${token}:${game}`;

    if (req.method === "GET") {
      const value = await env.STORE.get(key);
      return new Response(value ?? "{}", {
        headers: { ...cors, "content-type": "application/json" },
      });
    }

    if (req.method === "PUT") {
      const body = await req.text();
      if (body.length > MAX_BODY_BYTES) return text("too large", 413);
      try {
        JSON.parse(body);
      } catch {
        return text("not json", 400);
      }
      await env.STORE.put(key, body);
      return json({ ok: true });
    }

    return text("method not allowed", 405);
  },
};
