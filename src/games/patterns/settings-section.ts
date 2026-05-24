// Patterns game settings (theme / difficulty / mode / hint / reset) as
// a section in the parent-settings panel. The game owns its state and
// passes callbacks for live updates + the start-over flow.

import type { ParentSection } from '../../shared/parent-settings.js';

export type ThemeChoice =
  | 'mix'
  | 'emoji-animals'
  | 'emoji-fruit'
  | 'emoji-vehicles'
  | 'emoji-construction'
  | 'emoji-dinosaurs'
  | 'shapes'
  | 'letters-upper'
  | 'letters-lower'
  | 'numbers';
export type Difficulty = 'easy' | 'hard' | 'auto';
export type GameMode = 'next' | 'unit';

export interface PatternsSectionState {
  themeChoice: ThemeChoice;
  difficulty: Difficulty;
  mode: GameMode;
  showHint: boolean;
}

export interface PatternsSectionHooks {
  getState: () => PatternsSectionState;
  onThemeChange: (v: ThemeChoice) => void;
  onDifficultyChange: (v: Difficulty) => void;
  onModeChange: (v: GameMode) => void;
  onHintToggle: (v: boolean) => void;
  onReset: () => void;
}

export function buildPatternsSettingsSection(hooks: PatternsSectionHooks): ParentSection {
  const section = document.createElement('section');
  section.className = 'parent-section';
  section.innerHTML = `
    <h3>Patterns</h3>
    <div class="setting-row">
      <label for="ptn-theme">Pictures</label>
      <select id="ptn-theme">
        <option value="mix">Mix (auto)</option>
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
        <option value="auto">Auto — gets harder</option>
        <option value="easy">Easy — pick from the row</option>
        <option value="hard">Hard — pick from all</option>
      </select>
    </div>
    <div class="setting-row">
      <label for="ptn-mode">Game</label>
      <select id="ptn-mode">
        <option value="next">What comes next?</option>
        <option value="unit">Find the repeating piece</option>
      </select>
    </div>
    <div class="setting-row checkbox-row">
      <label><input type="checkbox" id="ptn-hint" /> Highlight the repeating piece</label>
    </div>
    <div class="parent-token-actions">
      <button class="secondary ptn-reset">Start over</button>
    </div>`;

  const themeSelect = section.querySelector<HTMLSelectElement>('#ptn-theme')!;
  const difficultySelect = section.querySelector<HTMLSelectElement>('#ptn-difficulty')!;
  const modeSelect = section.querySelector<HTMLSelectElement>('#ptn-mode')!;
  const hintToggle = section.querySelector<HTMLInputElement>('#ptn-hint')!;
  const resetBtn = section.querySelector<HTMLButtonElement>('.ptn-reset')!;

  const s = hooks.getState();
  themeSelect.value = s.themeChoice;
  difficultySelect.value = s.difficulty;
  modeSelect.value = s.mode;
  hintToggle.checked = s.showHint;

  themeSelect.addEventListener('change', () =>
    hooks.onThemeChange(themeSelect.value as ThemeChoice),
  );
  difficultySelect.addEventListener('change', () =>
    hooks.onDifficultyChange(difficultySelect.value as Difficulty),
  );
  modeSelect.addEventListener('change', () =>
    hooks.onModeChange(modeSelect.value as GameMode),
  );
  hintToggle.addEventListener('change', () => hooks.onHintToggle(hintToggle.checked));

  return {
    element: section,
    onMount: ({ close }) => {
      resetBtn.addEventListener('click', () => {
        hooks.onReset();
        close();
      });
    },
  };
}
