// Main game loop and UI wiring.

import { ALL_THEME_IDS, getTheme, type ThemeId, type Item, type Theme } from './themes.js';
import { generateRound, buildChoices, type PatternRound } from './patterns.js';
import { playCorrect, playIncorrect, playLevelUp, playTap, setMuted } from './sounds.js';
import { burst } from './confetti.js';
import { makeCell, makeChoiceButton } from './render.js';

type ThemeChoice = ThemeId | 'mix';
type Difficulty = 'easy' | 'hard' | 'auto';
type GameMode = 'next' | 'unit';

interface State {
  level: number;
  stars: number;
  /** Consecutive correct answers since last level change. */
  streak: number;
  themeChoice: ThemeChoice;
  difficulty: Difficulty;
  mode: GameMode;
  /** Highlight the repeating unit visually. */
  showHint: boolean;
  /** Active round; null only before first generation. */
  round: PatternRound | null;
  /** Active theme used for the current round. */
  activeTheme: Theme | null;
  /** Whether the player has already answered the current round. */
  locked: boolean;
}

const state: State = {
  level: 1,
  stars: 0,
  streak: 0,
  themeChoice: 'mix',
  difficulty: 'auto',
  mode: 'next',
  showHint: true,
  round: null,
  activeTheme: null,
  locked: false,
};

// ---------- DOM lookups ----------

function el<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing element #${id}`);
  return node as T;
}

const seqEl = el<HTMLElement>('sequence');
const choicesEl = el<HTMLElement>('choices');
const starsEl = el<HTMLElement>('stars');
const starCountEl = el<HTMLElement>('star-count');
const levelPipsEl = el<HTMLElement>('level-pips');
const muteBtn = el<HTMLButtonElement>('mute-btn');
const settingsBtn = el<HTMLButtonElement>('settings-btn');
const settingsPanel = el<HTMLElement>('settings-panel');
const closeSettingsBtn = el<HTMLButtonElement>('close-settings');
const resetBtn = el<HTMLButtonElement>('reset-btn');
const themeSelect = el<HTMLSelectElement>('theme-select');
const difficultySelect = el<HTMLSelectElement>('difficulty-select');
const modeSelect = el<HTMLSelectElement>('mode-select');
const hintToggle = el<HTMLInputElement>('hint-toggle');

// ---------- Theme picking ----------

function pickTheme(): Theme {
  if (state.themeChoice === 'mix') {
    const idx = Math.floor(Math.random() * ALL_THEME_IDS.length);
    const id = ALL_THEME_IDS[idx] ?? 'emoji-animals';
    return getTheme(id);
  }
  return getTheme(state.themeChoice);
}

// ---------- Difficulty resolution ----------

function effectiveAnswerMode(): 'easy' | 'hard' {
  if (state.difficulty === 'easy') return 'easy';
  if (state.difficulty === 'hard') return 'hard';
  // Auto: keep choices pulled from the visible row through level 4 so the
  // choice count grows naturally (2 for AB, 3 for ABC). Switch to the full
  // palette of distractors from level 5 once the player is solid.
  return state.level >= 5 ? 'hard' : 'easy';
}

// ---------- Rendering ----------

function renderSequence(round: PatternRound): void {
  seqEl.innerHTML = '';
  const unitLen = round.template.length;
  const showHint = state.showHint && state.mode === 'next';

  round.visible.forEach((item, i) => {
    const groupIdx = Math.floor(i / unitLen);
    const classes: string[] = [];
    if (showHint) {
      classes.push(groupIdx % 2 === 0 ? 'group-a' : 'group-b');
    }
    seqEl.append(makeCell(item, { classes }));
  });

  // The missing-slot cell at the end.
  const slot = makeCell(null, { classes: ['slot'], text: '?' });
  slot.setAttribute('aria-label', 'missing item');
  seqEl.append(slot);
}

function renderChoices(round: PatternRound, pool: Item[]): void {
  choicesEl.innerHTML = '';
  const mode = effectiveAnswerMode();
  const choices = buildChoices(round, mode, pool);
  for (const item of choices) {
    const btn = makeChoiceButton(item);
    btn.addEventListener('click', () => onChoice(btn, item));
    choicesEl.append(btn);
  }
}

const MAX_LEVEL = 6;

/** Re-render the HUD. If `justEarnedStar` is true, pop the star count
 *  briefly. If `justLeveledUp` is true, pulse the newly-filled pip. */
function renderHud(justEarnedStar = false, justLeveledUp = false): void {
  // Level pips: rainbow, one color per slot, lit per current level.
  levelPipsEl.innerHTML = '';
  for (let i = 1; i <= MAX_LEVEL; i++) {
    const pip = document.createElement('span');
    pip.className = 'level-pip';
    if (i <= state.level) pip.classList.add('filled');
    if (justLeveledUp && i === state.level) pip.classList.add('just-filled');
    levelPipsEl.append(pip);
  }
  // Numeric star count, brief bump on each new star.
  starCountEl.textContent = String(state.stars);
  if (justEarnedStar) {
    // Toggle the animation class off then on so it can replay.
    starsEl.classList.remove('bump');
    // Force a reflow so the next add re-triggers the animation.
    void starsEl.offsetWidth;
    starsEl.classList.add('bump');
    setTimeout(() => starsEl.classList.remove('bump'), 500);
  }
}

// ---------- Round flow ----------

function nextRound(): void {
  const theme = pickTheme();
  state.activeTheme = theme;
  state.locked = false;
  const round = generateRound({ pool: theme.items, level: state.level });
  state.round = round;
  if (state.mode === 'next') {
    renderSequence(round);
    renderChoices(round, theme.items);
  } else {
    renderUnitMode(round);
  }
  exposeDebug();
}

// Expose a small read-only snapshot for automated play-testing. No PII,
// no scores, just the round shape + level so tests can pick a choice.
interface DebugView {
  level: number;
  stars: number;
  streak: number;
  mode: GameMode;
  themeId: string | null;
  answerId: string | null;
  template: string | null;
  visibleIds: string[];
}
declare global {
  interface Window {
    __patternplay?: DebugView;
  }
}
function exposeDebug(): void {
  window.__patternplay = {
    level: state.level,
    stars: state.stars,
    streak: state.streak,
    mode: state.mode,
    themeId: state.activeTheme?.id ?? null,
    answerId: state.round?.answer.id ?? null,
    template: state.round?.template ?? null,
    visibleIds: state.round?.visible.map((it) => it.id) ?? [],
  };
}

function onChoice(btn: HTMLButtonElement, item: Item): void {
  if (state.locked || !state.round) return;
  playTap();
  if (item.id === state.round.answer.id) {
    state.locked = true;
    btn.classList.add('correct');
    state.stars += 1;
    state.streak += 1;
    renderHud(true /* justEarnedStar */);
    playCorrect();
    burst(70);
    // Level up every 4 correct in a row, max level 6.
    if (state.streak >= 4 && state.level < MAX_LEVEL) {
      state.level += 1;
      state.streak = 0;
      // Slight delay so the star animation finishes first.
      setTimeout(() => {
        renderHud(false, true /* justLeveledUp */);
        playLevelUp();
        burst(50);
      }, 480);
    }
    // Disable remaining choices.
    choicesEl.querySelectorAll<HTMLButtonElement>('.choice').forEach((b) => {
      b.disabled = true;
    });
    setTimeout(nextRound, 1100);
  } else {
    btn.classList.add('wrong');
    playIncorrect();
    state.streak = 0;
    setTimeout(() => btn.classList.remove('wrong'), 350);
  }
}

// ---------- "Find the repeating piece" mode ----------

/**
 * Unit-mode interaction:
 *  - Every cell starts tappable.
 *  - First tap selects that single cell.
 *  - Subsequent taps:
 *      • adjacent to the left/right edge of the selection -> extend
 *      • on the current left or right edge cell -> shrink that end
 *      • on any other cell -> ignore (cell wiggles "no")
 *  - A pulsing green ✓ appears under the row as soon as at least one
 *    cell is selected. Tapping it submits.
 *  - Correctness check is *length only*: did the kid pick a contiguous
 *    span equal to the period? Starting position doesn't matter.
 */
function renderUnitMode(round: PatternRound): void {
  seqEl.innerHTML = '';
  choicesEl.innerHTML = '';
  const unitLen = round.template.length;

  // Selection is the contiguous half-open range [start, end). end = start
  // means "nothing selected".
  let start = 0;
  let end = 0;

  const cells: HTMLDivElement[] = [];
  round.visible.forEach((item, i) => {
    const cell = makeCell(item, { classes: ['selectable'] });
    cell.setAttribute('role', 'button');
    cell.tabIndex = 0;
    cell.addEventListener('click', () => handleTap(i, cell));
    seqEl.append(cell);
    cells.push(cell);
  });

  const submit = document.createElement('button');
  submit.id = 'unit-submit';
  submit.className = 'unit-submit';
  submit.setAttribute('aria-label', 'Check my answer');
  submit.textContent = '✓';
  submit.hidden = true;
  submit.addEventListener('click', onSubmit);
  choicesEl.append(submit);

  function paint(): void {
    cells.forEach((c, i) => {
      const selected = i >= start && i < end;
      c.classList.toggle('unit-pick', selected);
    });
    submit.hidden = end <= start;
  }

  function bounceNo(cell: HTMLDivElement): void {
    cell.classList.add('bounce-no');
    setTimeout(() => cell.classList.remove('bounce-no'), 300);
  }

  function handleTap(idx: number, cell: HTMLDivElement): void {
    if (state.locked) return;
    playTap();
    if (end <= start) {
      // Nothing selected yet — start fresh at this cell.
      start = idx;
      end = idx + 1;
      paint();
      return;
    }
    // Extend left.
    if (idx === start - 1) { start -= 1; paint(); return; }
    // Extend right.
    if (idx === end)       { end   += 1; paint(); return; }
    // Shrink from left edge if they tap the leftmost selected cell.
    if (idx === start)     { start += 1; paint(); return; }
    // Shrink from right edge.
    if (idx === end - 1)   { end   -= 1; paint(); return; }
    // Non-adjacent: bounce that cell so the tap registers visually but
    // selection is unchanged.
    bounceNo(cell);
  }

  function onSubmit(): void {
    if (state.locked) return;
    const len = end - start;
    if (len <= 0) return;
    if (len === unitLen) {
      state.locked = true;
      for (let k = start; k < end; k++) {
        cells[k]?.classList.remove('unit-pick');
        cells[k]?.classList.add('unit-correct');
      }
      submit.hidden = true;
      state.stars += 1;
      state.streak += 1;
      renderHud(true);
      playCorrect();
      burst(70);
      // Unit mode shares the same level progression as 'next' mode so
      // pattern difficulty escalates regardless of which game type the
      // kid plays.
      if (state.streak >= 4 && state.level < MAX_LEVEL) {
        state.level += 1;
        state.streak = 0;
        setTimeout(() => {
          renderHud(false, true);
          playLevelUp();
          burst(50);
        }, 480);
      }
      setTimeout(nextRound, 1300);
    } else {
      // Wrong length: red flash on the selection, then reset.
      for (let k = start; k < end; k++) cells[k]?.classList.add('unit-wrong');
      playIncorrect();
      state.streak = 0;
      setTimeout(() => {
        cells.forEach((c) =>
          c.classList.remove('unit-wrong', 'unit-pick', 'unit-correct'),
        );
        start = end = 0;
        paint();
      }, 600);
    }
  }
}

// ---------- Settings + persistence (settings only, no scores) ----------

const SETTINGS_KEY = 'patternplay.settings.v1';

interface PersistedSettings {
  themeChoice?: ThemeChoice;
  difficulty?: Difficulty;
  mode?: GameMode;
  showHint?: boolean;
  muted?: boolean;
}

function loadSettings(): void {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return;
    const data = JSON.parse(raw) as PersistedSettings;
    if (data.themeChoice) state.themeChoice = data.themeChoice;
    if (data.difficulty) state.difficulty = data.difficulty;
    if (data.mode) state.mode = data.mode;
    if (typeof data.showHint === 'boolean') state.showHint = data.showHint;
    if (data.muted) {
      setMuted(true);
      muteBtn.setAttribute('aria-pressed', 'true');
      muteBtn.querySelector<HTMLElement>('.icon-sound')?.setAttribute('hidden', '');
      muteBtn.querySelector<HTMLElement>('.icon-muted')?.removeAttribute('hidden');
    }
  } catch {
    /* ignore corrupted settings */
  }
}

function saveSettings(): void {
  const data: PersistedSettings = {
    themeChoice: state.themeChoice,
    difficulty: state.difficulty,
    mode: state.mode,
    showHint: state.showHint,
    muted: muteBtn.getAttribute('aria-pressed') === 'true',
  };
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(data));
  } catch {
    /* storage might be blocked; fine */
  }
}

function applySettingsToControls(): void {
  themeSelect.value = state.themeChoice;
  difficultySelect.value = state.difficulty;
  modeSelect.value = state.mode;
  hintToggle.checked = state.showHint;
}

function openSettings(): void {
  applySettingsToControls();
  settingsPanel.hidden = false;
}
function closeSettings(): void {
  settingsPanel.hidden = true;
}

// ---------- Wire up ----------

muteBtn.addEventListener('click', () => {
  const pressed = muteBtn.getAttribute('aria-pressed') === 'true';
  const next = !pressed;
  muteBtn.setAttribute('aria-pressed', String(next));
  setMuted(next);
  muteBtn.querySelector<HTMLElement>('.icon-sound')?.toggleAttribute('hidden', next);
  muteBtn.querySelector<HTMLElement>('.icon-muted')?.toggleAttribute('hidden', !next);
  saveSettings();
});

settingsBtn.addEventListener('click', openSettings);
closeSettingsBtn.addEventListener('click', () => {
  closeSettings();
  saveSettings();
});
settingsPanel.addEventListener('click', (e) => {
  if (e.target === settingsPanel) closeSettings();
});

themeSelect.addEventListener('change', () => {
  state.themeChoice = themeSelect.value as ThemeChoice;
  saveSettings();
  nextRound();
});
difficultySelect.addEventListener('change', () => {
  state.difficulty = difficultySelect.value as Difficulty;
  saveSettings();
  nextRound();
});
modeSelect.addEventListener('change', () => {
  state.mode = modeSelect.value as GameMode;
  saveSettings();
  nextRound();
});
hintToggle.addEventListener('change', () => {
  state.showHint = hintToggle.checked;
  saveSettings();
  if (state.round) renderSequence(state.round);
});

resetBtn.addEventListener('click', () => {
  state.level = 1;
  state.stars = 0;
  state.streak = 0;
  renderHud();
  closeSettings();
  nextRound();
});

// Keyboard convenience: Esc closes settings.
window.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') closeSettings();
});

// ---------- Service worker (offline / faster cold-starts) ----------

declare const __BUILD_ID__: string;

function registerServiceWorker(): void {
  if (!('serviceWorker' in navigator)) return;

  const params = new URLSearchParams(location.search);
  // Escape hatch: ?nosw unregisters any installed worker and reloads.
  if (params.has('nosw')) {
    void navigator.serviceWorker.getRegistrations().then(async (regs) => {
      await Promise.all(regs.map((r) => r.unregister()));
      const cs = await caches.keys();
      await Promise.all(cs.map((k) => caches.delete(k)));
      location.replace(location.pathname);
    });
    return;
  }
  // Skip on localhost so dev iteration isn't fighting a stale cache. Use
  // ?sw=force to opt in (e.g. for offline-mode tests against a local
  // server).
  const host = location.hostname;
  const isLocal = host === 'localhost' || host === '127.0.0.1' || host === '';
  if (isLocal && params.get('sw') !== 'force') return;

  window.addEventListener('load', () => {
    // Capture controller state before registration. If a controller was
    // already present, this is an existing-install boot; any subsequent
    // controllerchange means an update was just activated, so reload to
    // pick up the new assets. On first install controller flips
    // null → non-null and the page is already running fine — no reload.
    const hadController = navigator.serviceWorker.controller !== null;
    let reloaded = false;
    if (hadController) {
      navigator.serviceWorker.addEventListener('controllerchange', () => {
        if (reloaded) return;
        reloaded = true;
        location.reload();
      });
    }
    void navigator.serviceWorker
      .register('./sw.js')
      .then((reg) => {
        // Nudge the worker to check for updates on every visit.
        void reg.update();
      })
      .catch(() => {
        /* registration failed; site still works online */
      });
  });
}

// ---------- Orientation lock (best effort) ----------

/**
 * When launched as a standalone PWA, try to lock the screen to
 * landscape. The manifest declares orientation:"landscape" already; this
 * is a runtime belt-and-braces for browsers that respect screen.orientation
 * (Android Chrome) and a no-op on browsers that don't (iOS Safari).
 */
function tryLockLandscape(): void {
  const standalone =
    window.matchMedia?.('(display-mode: standalone)').matches ??
    (window.navigator as unknown as { standalone?: boolean }).standalone ??
    false;
  if (!standalone) return;
  const orient = (screen as unknown as { orientation?: { lock?: (o: string) => Promise<void> } })
    .orientation;
  if (orient?.lock) {
    void orient.lock('landscape').catch(() => {
      /* lock can be rejected on iOS or when no fullscreen — that's fine */
    });
  }
}

// ---------- Boot ----------

loadSettings();
applySettingsToControls();
renderHud();
nextRound();
tryLockLandscape();
registerServiceWorker();
// Surface the build id on window for quick debugging.
(window as unknown as { __patternplay_build?: string }).__patternplay_build = __BUILD_ID__;
