// Registry of available games. Adding a game = add an import + entry here.

import { mount as mountPatterns } from './patterns/game.js';
import { mount as mountPhonics } from './phonics/game.js';

export interface MountOpts {
  /** Called when the in-game home button is tapped. */
  onHome: () => void;
}

export interface GameDef {
  id: string;
  /** Single-word label (incidental reading; navigation must work without it). */
  label: string;
  /** Fallback glyph + aria text. Used when renderIcon isn't supplied. */
  emoji: string;
  /** Optional custom icon renderer for the picker card (e.g. a literal
   *  pattern sequence rather than a single emoji). */
  renderIcon?: (container: HTMLElement) => void;
  mount: (container: HTMLElement, opts: MountOpts) => () => void;
}

function renderPatternsIcon(container: HTMLElement): void {
  const seq = document.createElement('div');
  seq.className = 'picker-icon-sequence';
  // A literal "what comes next?" pattern so the icon teaches the
  // mechanic at a glance — kids should not need to read the label.
  const cells: Array<{ glyph: string; slot?: boolean }> = [
    { glyph: '🐶' },
    { glyph: '🐱' },
    { glyph: '🐶' },
    { glyph: '?', slot: true },
  ];
  for (const c of cells) {
    const el = document.createElement('span');
    el.className = c.slot ? 'picker-icon-cell picker-icon-slot' : 'picker-icon-cell';
    el.textContent = c.glyph;
    seq.append(el);
  }
  container.append(seq);
}

function renderPhonicsIcon(container: HTMLElement): void {
  // Preview the reward scene — rainbow above, frog peeking below — so the
  // picker tile teases what's waiting at the end of a session. Same
  // characters the kid meets on the rainbow splash; sets expectation.
  const scene = document.createElement('div');
  scene.className = 'picker-icon-phonics';
  const rainbow = document.createElement('span');
  rainbow.className = 'picker-phonics-rainbow';
  rainbow.textContent = '🌈';
  const frog = document.createElement('span');
  frog.className = 'picker-phonics-frog';
  frog.textContent = '🐸';
  scene.append(rainbow, frog);
  container.append(scene);
}

export const GAMES: GameDef[] = [
  {
    id: 'patterns',
    label: 'patterns',
    emoji: '🐶🐱🐶?',
    renderIcon: renderPatternsIcon,
    mount: mountPatterns,
  },
  {
    id: 'phonics',
    label: 'phonics',
    // `emoji` is the aria/fallback only — renderIcon owns the visual.
    emoji: '🌈🐸',
    renderIcon: renderPhonicsIcon,
    mount: mountPhonics,
  },
];
