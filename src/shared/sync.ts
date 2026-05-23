// Cross-device sync client. Talks to the CF Worker in server/.
//
// API: pull<T>(game) -> latest blob or null; push<T>(game, blob) -> debounced.
// One opaque family token spans all games. No auth header.
//
// Filled in fully in the next commit once the Worker URL is known. For now
// a noop stand-in keeps the rest of the app importing a stable shape.

export interface SyncClient {
  pull<T>(game: string): Promise<T | null>;
  push<T>(game: string, blob: T): void;
  /** Force-flush any pending pushes (e.g. on visibilitychange). */
  flush(): Promise<void>;
}

const NOOP: SyncClient = {
  async pull() {
    return null;
  },
  push() {
    /* noop */
  },
  async flush() {
    /* noop */
  },
};

export function createSyncClient(_endpoint: string | null, _token: string | null): SyncClient {
  // TODO next commit: real client with debounced PUT + GET on demand.
  return NOOP;
}
