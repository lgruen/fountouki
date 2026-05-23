// Shared (app-wide) settings: mute, sync token. Per-game settings live in
// the game's own area via shared/storage.

import { load, save } from './storage.js';
import { setMuted } from './sounds.js';

export interface SharedSettings {
  muted: boolean;
  /** Family namespace for cross-device sync. Set by parent UI. */
  syncToken: string | null;
  /** Override of the default sync endpoint. Set by parent UI; null = default. */
  syncEndpoint: string | null;
}

const DEFAULTS: SharedSettings = {
  muted: false,
  syncToken: null,
  syncEndpoint: null,
};

export function loadShared(): SharedSettings {
  return { ...DEFAULTS, ...(load<SharedSettings>('shared', 'settings') ?? {}) };
}

export function saveShared(patch: Partial<SharedSettings>): SharedSettings {
  const next = { ...loadShared(), ...patch };
  save('shared', 'settings', next);
  return next;
}

export function applyOnBoot(): void {
  const s = loadShared();
  setMuted(s.muted);
}

/** Returns the new muted state. */
export function toggleMuted(): boolean {
  const next = !loadShared().muted;
  saveShared({ muted: next });
  setMuted(next);
  return next;
}
