// Patterns game: sequence-completion + find-the-repeating-piece.
// Exported as mount(container) so the picker can swap it in/out.

import { ALL_THEME_IDS, getTheme, type ThemeId, type Item, type Theme } from './themes.js';
import { generateRound, buildChoices, type PatternRound } from './patterns.js';
import { playCorrect, playIncorrect, playLevelUp, playTap } from '../../shared/sounds.js';
import { burst } from '../../shared/confetti.js';
import { makeCell, makeChoiceButton } from './render.js';
import { makeHomeButton, makeMuteButton } from '../../shared/chrome.js';
import { load, save } from '../../shared/storage.js';
import type { MountOpts } from '../registry.js';

type ThemeChoice = ThemeId | 'mix';
type Difficulty = 'easy' | 'hard' | 'auto';
type GameMode = 'next' | 'unit';

interface PersistedSettings {
  themeChoice?: ThemeChoice;
  difficulty?: Difficulty;
  mode?: GameMode;
  showHint?: boolean;
}

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
    __patterns?: DebugView;
  }
}

const MAX_LEVEL = 6;

export function mount(container: HTMLElement, opts: MountOpts): () => void {
  const state = {
    level: 1,
    stars: 0,
    streak: 0,
    themeChoice: 'mix' as ThemeChoice,
    difficulty: 'auto' as Difficulty,
    mode: 'next' as GameMode,
    showHint: false,
    round: null as PatternRound | null,
    activeTheme: null as Theme | null,
    locked: false,
  };

  // Timer + listener cleanup harness.
  const abort = new AbortController();
  const timers = new Set<number>();
  const setT = (fn: () => void, ms: number): number => {
    const id = window.setTimeout(() => {
      timers.delete(id);
      fn();
    }, ms);
    timers.add(id);
    return id;
  };

  // ---------- DOM ----------
  container.innerHTML = '';
  const root = document.createElement('div');
  root.className = 'game game-patterns';
  container.append(root);

  const topbar = document.createElement('header');
  topbar.className = 'topbar';
  const homeBtn = makeHomeButton({ onHome: opts.onHome });
  const starsEl = document.createElement('div');
  starsEl.className = 'stars';
  starsEl.setAttribute('aria-label', 'Stars');
  const starGlyph = document.createElement('span');
  starGlyph.className = 'star';
  starGlyph.textContent = '★';
  const starCountEl = document.createElement('span');
  starCountEl.className = 'star-count';
  starCountEl.textContent = '0';
  starsEl.append(starGlyph, starCountEl);
  const levelPipsEl = document.createElement('div');
  levelPipsEl.className = 'level-pips';
  levelPipsEl.setAttribute('aria-label', 'Level');
  const muteBtn = makeMuteButton();
  const settingsBtn = document.createElement('button');
  settingsBtn.className = 'icon-btn settings-btn';
  settingsBtn.setAttribute('aria-label', 'Settings');
  settingsBtn.textContent = '⚙️';
  topbar.append(homeBtn, starsEl, levelPipsEl, muteBtn, settingsBtn);
  root.append(topbar);

  const playArea = document.createElement('div');
  playArea.className = 'play-area';
  const seqEl = document.createElement('section');
  seqEl.className = 'sequence';
  seqEl.setAttribute('aria-label', 'Pattern sequence');
  const choicesEl = document.createElement('section');
  choicesEl.className = 'choices';
  choicesEl.setAttribute('aria-label', 'Answer choices');
  playArea.append(seqEl, choicesEl);
  root.append(playArea);

  const settingsPanel = document.createElement('div');
  settingsPanel.className = 'settings-panel';
  settingsPanel.hidden = true;
  settingsPanel.innerHTML = `
    <div class="settings-card" role="dialog" aria-label="Settings">
      <h2>Settings</h2>
      <div class="setting-row">
        <label for="ptn-theme">Pictures</label>
        <select id="ptn-theme">
          <option value="mix" selected>Mix (auto)</option>
          <option value="emoji-animals">Animals 🐶</option>
          <option value="emoji-fruit">Fruit 🍎</option>
          <option value="emoji-vehicles">Vehicles 🚗</option>
          <option value="emoji-construction">Construction 🏗️</option>
          <option value="emoji-dinosaurs">Dinosaurs 🦖</option>
          <option value="shapes">Shapes 🟥</option>
          <option value="letters-upper">Letters (ABC)</option>
          <option value="letters-lower">letters (abc)</option>
          <option value="numbers">Numbers (123)</option>
        </select>
      </div>
      <div class="setting-row">
        <label for="ptn-difficulty">Helpers</label>
        <select id="ptn-difficulty">
          <option value="auto" selected>Auto — gets harder</option>
          <option value="easy">Easy — pick from the row</option>
          <option value="hard">Hard — pick from all</option>
        </select>
      </div>
      <div class="setting-row">
        <label for="ptn-mode">Game</label>
        <select id="ptn-mode">
          <option value="next" selected>What comes next?</option>
          <option value="unit">Find the repeating piece</option>
        </select>
      </div>
      <div class="setting-row checkbox-row">
        <label><input type="checkbox" id="ptn-hint" /> Highlight the repeating piece</label>
      </div>
      <div class="setting-actions">
        <button class="secondary ptn-reset">Start over</button>
        <button class="primary ptn-close">Done</button>
      </div>
      <p class="footer-note">No data leaves this device. <a href="https://github.com/lgruen/fountouki" target="_blank" rel="noopener">Open source · MIT</a></p>
    </div>`;
  root.append(settingsPanel);

  const themeSelect = settingsPanel.querySelector<HTMLSelectElement>('#ptn-theme')!;
  const difficultySelect = settingsPanel.querySelector<HTMLSelectElement>('#ptn-difficulty')!;
  const modeSelect = settingsPanel.querySelector<HTMLSelectElement>('#ptn-mode')!;
  const hintToggle = settingsPanel.querySelector<HTMLInputElement>('#ptn-hint')!;
  const resetBtn = settingsPanel.querySelector<HTMLButtonElement>('.ptn-reset')!;
  const closeBtn = settingsPanel.querySelector<HTMLButtonElement>('.ptn-close')!;

  // ---------- helpers ----------
  function pickTheme(): Theme {
    if (state.themeChoice === 'mix') {
      const idx = Math.floor(Math.random() * ALL_THEME_IDS.length);
      return getTheme(ALL_THEME_IDS[idx] ?? 'emoji-animals');
    }
    return getTheme(state.themeChoice);
  }

  function effectiveAnswerMode(): 'easy' | 'hard' {
    if (state.difficulty === 'easy') return 'easy';
    if (state.difficulty === 'hard') return 'hard';
    return state.level >= 4 ? 'hard' : 'easy';
  }

  function renderSequence(round: PatternRound): void {
    seqEl.innerHTML = '';
    const unitLen = round.template.length;
    const showHint = state.showHint && state.mode === 'next';
    round.visible.forEach((item, i) => {
      const groupIdx = Math.floor(i / unitLen);
      const classes: string[] = [];
      if (showHint) classes.push(groupIdx % 2 === 0 ? 'group-a' : 'group-b');
      seqEl.append(makeCell(item, { classes }));
    });
    const slot = makeCell(null, { classes: ['slot'], text: '?' });
    slot.setAttribute('aria-label', 'missing item');
    seqEl.append(slot);
  }

  function renderChoices(round: PatternRound, pool: Item[]): void {
    choicesEl.innerHTML = '';
    const choices = buildChoices(round, effectiveAnswerMode(), pool);
    for (const item of choices) {
      const btn = makeChoiceButton(item);
      btn.addEventListener('click', () => onChoice(btn, item));
      choicesEl.append(btn);
    }
  }

  function renderHud(justEarnedStar = false, justLeveledUp = false): void {
    levelPipsEl.innerHTML = '';
    for (let i = 1; i <= MAX_LEVEL; i++) {
      const pip = document.createElement('span');
      pip.className = 'level-pip';
      if (i <= state.level) pip.classList.add('filled');
      if (justLeveledUp && i === state.level) pip.classList.add('just-filled');
      levelPipsEl.append(pip);
    }
    starCountEl.textContent = String(state.stars);
    if (justEarnedStar) {
      starsEl.classList.remove('bump');
      void starsEl.offsetWidth;
      starsEl.classList.add('bump');
      setT(() => starsEl.classList.remove('bump'), 500);
    }
  }

  function exposeDebug(): void {
    window.__patterns = {
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

  function onChoice(btn: HTMLButtonElement, item: Item): void {
    if (state.locked || !state.round) return;
    playTap();
    if (item.id === state.round.answer.id) {
      state.locked = true;
      btn.classList.add('correct');
      state.stars += 1;
      state.streak += 1;
      renderHud(true);
      playCorrect();
      burst(70);
      if (state.streak >= 4 && state.level < MAX_LEVEL) {
        state.level += 1;
        state.streak = 0;
        setT(() => {
          renderHud(false, true);
          playLevelUp();
          burst(50);
        }, 480);
      }
      choicesEl.querySelectorAll<HTMLButtonElement>('.choice').forEach((b) => {
        b.disabled = true;
      });
      setT(nextRound, 1100);
    } else {
      btn.classList.add('wrong');
      playIncorrect();
      state.streak = 0;
      setT(() => btn.classList.remove('wrong'), 350);
    }
  }

  function renderUnitMode(round: PatternRound): void {
    seqEl.innerHTML = '';
    choicesEl.innerHTML = '';
    const unitLen = round.template.length;

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
    submit.className = 'unit-submit';
    submit.setAttribute('aria-label', 'Check my answer');
    submit.textContent = '✓';
    submit.hidden = true;
    submit.addEventListener('click', onSubmit);
    choicesEl.append(submit);

    function paint(): void {
      cells.forEach((c, i) => {
        c.classList.toggle('unit-pick', i >= start && i < end);
      });
      submit.hidden = end <= start;
    }

    function bounceNo(cell: HTMLDivElement): void {
      cell.classList.add('bounce-no');
      setT(() => cell.classList.remove('bounce-no'), 300);
    }

    function handleTap(idx: number, cell: HTMLDivElement): void {
      if (state.locked) return;
      playTap();
      if (end <= start) {
        start = idx;
        end = idx + 1;
        paint();
        return;
      }
      if (idx === start - 1) {
        start -= 1;
        paint();
        return;
      }
      if (idx === end) {
        end += 1;
        paint();
        return;
      }
      if (idx === start) {
        start += 1;
        paint();
        return;
      }
      if (idx === end - 1) {
        end -= 1;
        paint();
        return;
      }
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
        if (state.streak >= 4 && state.level < MAX_LEVEL) {
          state.level += 1;
          state.streak = 0;
          setT(() => {
            renderHud(false, true);
            playLevelUp();
            burst(50);
          }, 480);
        }
        setT(nextRound, 1300);
      } else {
        for (let k = start; k < end; k++) cells[k]?.classList.add('unit-wrong');
        playIncorrect();
        state.streak = 0;
        setT(() => {
          cells.forEach((c) =>
            c.classList.remove('unit-wrong', 'unit-pick', 'unit-correct'),
          );
          start = end = 0;
          paint();
        }, 600);
      }
    }
  }

  // ---------- persistence ----------
  function loadPersisted(): void {
    const data = load<PersistedSettings>('patterns', 'settings');
    if (!data) return;
    if (data.themeChoice) state.themeChoice = data.themeChoice;
    if (data.difficulty) state.difficulty = data.difficulty;
    if (data.mode) state.mode = data.mode;
    if (typeof data.showHint === 'boolean') state.showHint = data.showHint;
  }

  function persist(): void {
    save<PersistedSettings>('patterns', 'settings', {
      themeChoice: state.themeChoice,
      difficulty: state.difficulty,
      mode: state.mode,
      showHint: state.showHint,
    });
  }

  function applyToControls(): void {
    themeSelect.value = state.themeChoice;
    difficultySelect.value = state.difficulty;
    modeSelect.value = state.mode;
    hintToggle.checked = state.showHint;
  }

  function openSettings(): void {
    applyToControls();
    settingsPanel.hidden = false;
  }
  function closeSettings(): void {
    settingsPanel.hidden = true;
  }

  // ---------- wire ----------
  const sig = abort.signal;
  settingsBtn.addEventListener('click', openSettings, { signal: sig });
  closeBtn.addEventListener(
    'click',
    () => {
      closeSettings();
      persist();
    },
    { signal: sig },
  );
  settingsPanel.addEventListener(
    'click',
    (e) => {
      if (e.target === settingsPanel) closeSettings();
    },
    { signal: sig },
  );

  themeSelect.addEventListener(
    'change',
    () => {
      state.themeChoice = themeSelect.value as ThemeChoice;
      persist();
      nextRound();
    },
    { signal: sig },
  );
  difficultySelect.addEventListener(
    'change',
    () => {
      state.difficulty = difficultySelect.value as Difficulty;
      persist();
      nextRound();
    },
    { signal: sig },
  );
  modeSelect.addEventListener(
    'change',
    () => {
      state.mode = modeSelect.value as GameMode;
      persist();
      nextRound();
    },
    { signal: sig },
  );
  hintToggle.addEventListener(
    'change',
    () => {
      state.showHint = hintToggle.checked;
      persist();
      if (state.round) renderSequence(state.round);
    },
    { signal: sig },
  );

  resetBtn.addEventListener(
    'click',
    () => {
      state.level = 1;
      state.stars = 0;
      state.streak = 0;
      renderHud();
      closeSettings();
      nextRound();
    },
    { signal: sig },
  );

  window.addEventListener(
    'keydown',
    (e) => {
      if (e.key === 'Escape') closeSettings();
    },
    { signal: sig },
  );

  // ---------- boot ----------
  loadPersisted();
  applyToControls();
  renderHud();
  nextRound();

  return () => {
    abort.abort();
    for (const id of timers) clearTimeout(id);
    timers.clear();
    delete window.__patterns;
    container.innerHTML = '';
  };
}
