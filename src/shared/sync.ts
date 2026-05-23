// Cross-device sync client. Talks to the CF Worker in server/.
// One opaque family token spans all games (path = /<token>/<game>).
// Reads settings from shared/settings on every call so token / endpoint
// changes mid-session take effect immediately.

import { loadShared } from './settings.js';

const DEFAULT_ENDPOINT = 'https://fountouki-sync.fountouki.workers.dev';
const DEBOUNCE_MS = 500;

export interface SyncClient {
  /** Returns the latest blob for `game`, or null if no token or fetch failed. */
  pull<T>(game: string): Promise<T | null>;
  /** Debounced PUT — coalesces multiple pushes for the same game. */
  push<T>(game: string, blob: T): void;
  /** Flush any pending pushes immediately (e.g. on visibilitychange). */
  flush(): Promise<void>;
  /** Whether sync is currently configured (token present). */
  configured(): boolean;
}

interface Cfg {
  endpoint: string;
  token: string;
}

function readCfg(): Cfg | null {
  const s = loadShared();
  if (!s.syncToken) return null;
  return { endpoint: s.syncEndpoint || DEFAULT_ENDPOINT, token: s.syncToken };
}

interface Pending {
  blob: unknown;
  timer: number;
}

export function createSyncClient(): SyncClient {
  const pending = new Map<string, Pending>();

  async function doPush(cfg: Cfg, game: string, blob: unknown): Promise<void> {
    try {
      await fetch(`${cfg.endpoint}/${cfg.token}/${game}`, {
        method: 'PUT',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(blob),
      });
    } catch {
      // Best-effort — don't crash gameplay if offline / endpoint down.
    }
  }

  return {
    async pull<T>(game: string): Promise<T | null> {
      const cfg = readCfg();
      if (!cfg) return null;
      try {
        const r = await fetch(`${cfg.endpoint}/${cfg.token}/${game}`, { method: 'GET' });
        if (!r.ok) return null;
        const text = await r.text();
        if (!text || text === '{}') return null;
        return JSON.parse(text) as T;
      } catch {
        return null;
      }
    },

    push<T>(game: string, blob: T): void {
      const cfg = readCfg();
      if (!cfg) return;
      const prev = pending.get(game);
      if (prev) clearTimeout(prev.timer);
      const timer = window.setTimeout(() => {
        pending.delete(game);
        void doPush(cfg, game, blob);
      }, DEBOUNCE_MS);
      pending.set(game, { blob, timer });
    },

    async flush(): Promise<void> {
      const cfg = readCfg();
      if (!cfg) return;
      const items = [...pending.entries()];
      pending.clear();
      await Promise.all(
        items.map(async ([game, p]) => {
          clearTimeout(p.timer);
          await doPush(cfg, game, p.blob);
        }),
      );
    },

    configured(): boolean {
      return readCfg() !== null;
    },
  };
}

/** Singleton client used across the app. */
export const sync: SyncClient = createSyncClient();

/** Random 16-char family token. ~82 bits of entropy. */
export function generateToken(): string {
  const alpha = 'abcdefghijklmnopqrstuvwxyz0123456789';
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  let out = '';
  for (const b of bytes) out += alpha[b % alpha.length] ?? '';
  return out;
}

export { DEFAULT_ENDPOINT };
