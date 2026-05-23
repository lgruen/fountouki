// Registry of available games. Adding a game = add an import + entry here.

import { mount as mountPatterns } from './patterns/game.js';

export interface MountOpts {
  /** Called when the in-game home button is tapped. */
  onHome: () => void;
}

export interface GameDef {
  id: string;
  /** Single-word label (incidental reading; navigation must work without it). */
  label: string;
  /** Big emoji shown on the picker card. */
  emoji: string;
  mount: (container: HTMLElement, opts: MountOpts) => () => void;
}

export const GAMES: GameDef[] = [
  { id: 'patterns', label: 'patterns', emoji: '🧩', mount: mountPatterns },
];
