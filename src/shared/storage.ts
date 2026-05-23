// Namespaced localStorage. Keys always look like `fountouki.<area>.<name>.v1`.
//
// Areas: `shared` for app-wide settings (mute, sync token);
//        `<game-id>` for per-game state.

const NS = 'fountouki';

export function storageKey(area: string, name: string, version = 'v1'): string {
  return `${NS}.${area}.${name}.${version}`;
}

export function load<T>(area: string, name: string): T | null {
  try {
    const raw = localStorage.getItem(storageKey(area, name));
    return raw === null ? null : (JSON.parse(raw) as T);
  } catch {
    return null;
  }
}

export function save<T>(area: string, name: string, value: T): void {
  try {
    localStorage.setItem(storageKey(area, name), JSON.stringify(value));
  } catch {
    /* storage might be blocked; fine */
  }
}

export function remove(area: string, name: string): void {
  try {
    localStorage.removeItem(storageKey(area, name));
  } catch {
    /* ignore */
  }
}

/** One-time migration of legacy keys from the pattern-game days. */
export function migrateLegacy(): void {
  const moves: Array<[from: string, area: string, name: string]> = [
    ['patternplay.settings.v1', 'patterns', 'settings'],
  ];
  for (const [from, area, name] of moves) {
    const to = storageKey(area, name);
    try {
      if (localStorage.getItem(to) !== null) continue;
      const v = localStorage.getItem(from);
      if (v === null) continue;
      localStorage.setItem(to, v);
      localStorage.removeItem(from);
    } catch {
      /* ignore */
    }
  }
}
