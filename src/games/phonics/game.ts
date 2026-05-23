// Phonics game: parent-graded lowercase-letter → sound flashcards.
// Errorless, monotonic, no time pressure. See docs/IDEAS.md.

import { pickExemplar } from './deck.js';
import {
  type PhonicsState,
  emptyState,
  ensureLetters,
  gotIt,
  missed,
  buildQueue,
  merge,
  validate,
} from './srs.js';
import { makeHomeButton, makeMuteButton } from '../../shared/chrome.js';
import { load, save } from '../../shared/storage.js';
import { sync } from '../../shared/sync.js';
import { burst } from '../../shared/confetti.js';
import { playCorrect, playLevelUp, playTap } from '../../shared/sounds.js';
import type { MountOpts } from '../registry.js';

const GAME_ID = 'phonics';
const STORAGE_NAME = 'state';
const SESSION_GOAL = 7; // full rainbow — keep in sync with .arc-N CSS in style.css.
const REQUEUE_GAP = 4; // how many cards after a miss before the same card re-appears.
const ADVANCE_DELAY_MS = 700; // delay between "got it" and the next card.
const BURST_BASE = 22;
const BURST_STREAK_STEP = 8; // extra particles per consecutive correct
const BURST_AT_DONE = 140;

// SVG geometry for the rainbow arcs. Each arc is a 150° chord (75° each
// side from top), all sharing the chord baseline y=yHorizon so they fan
// out like a real rainbow seen from the ground. Arc-0 is the outermost,
// arc-(SESSION_GOAL-1) is the innermost (and the first to light up).
const SVG_NS = 'http://www.w3.org/2000/svg';
const ARC_CX = 120;
const ARC_Y_HORIZON = 70;
const ARC_HALF_ANGLE = (75 * Math.PI) / 180;
const ARC_SIN = Math.sin(ARC_HALF_ANGLE); // ≈ 0.966
const ARC_COS = Math.cos(ARC_HALF_ANGLE); // ≈ 0.259
const ARC_SAGITTA_OUTER = 65;
const ARC_SAGITTA_INNER = 25;

interface DebugView {
  letter: string | null;
  stars: number;
  inMissReveal: boolean;
  queueLength: number;
  state: PhonicsState;
}

declare global {
  interface Window {
    __phonics?: DebugView;
  }
}

export function mount(container: HTMLElement, opts: MountOpts): () => void {
  let state: PhonicsState = validate(load<unknown>(GAME_ID, STORAGE_NAME)) ?? emptyState();
  ensureLetters(state);
  save(GAME_ID, STORAGE_NAME, state);

  let stars = 0;
  let streak = 0;
  let queue: string[] = buildQueue(state);
  let currentLetter: string | null = null;
  let inMissReveal = false;

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
  root.className = 'game game-phonics';
  container.append(root);

  const topbar = document.createElement('header');
  topbar.className = 'topbar phonics-topbar';
  const home = makeHomeButton({ onHome: opts.onHome });
  const topSpacer = document.createElement('div');
  topSpacer.className = 'phonics-topbar-spacer';
  const mute = makeMuteButton();
  // No star counter on the topbar — the rainbow IS the progress
  // indicator. Avoids the "quiz score" feel of "★ 0" in the corner.
  topbar.append(home, topSpacer, mute);
  root.append(topbar);

  const play = document.createElement('div');
  play.className = 'phonics-play';

  const card = document.createElement('div');
  card.className = 'phonics-card';

  // Rainbow arcs (SVG) — visual hero, grow from inner-out as stars accumulate.
  const arcSvg = document.createElementNS(SVG_NS, 'svg');
  arcSvg.setAttribute('viewBox', '0 0 240 80');
  arcSvg.setAttribute('class', 'phonics-arcs');
  arcSvg.setAttribute('aria-label', 'Rainbow progress');
  const arcPaths: SVGPathElement[] = [];
  for (let i = 0; i < SESSION_GOAL; i++) {
    const t = i / (SESSION_GOAL - 1);
    const sagitta = ARC_SAGITTA_OUTER - t * (ARC_SAGITTA_OUTER - ARC_SAGITTA_INNER);
    const r = sagitta / (1 - ARC_COS);
    const w = r * ARC_SIN;
    const path = document.createElementNS(SVG_NS, 'path');
    path.setAttribute(
      'd',
      `M ${(ARC_CX - w).toFixed(2)} ${ARC_Y_HORIZON} A ${r.toFixed(2)} ${r.toFixed(2)} 0 0 1 ${(ARC_CX + w).toFixed(2)} ${ARC_Y_HORIZON}`,
    );
    path.setAttribute('stroke-width', '8');
    path.setAttribute('stroke-linecap', 'round');
    path.setAttribute('fill', 'none');
    path.setAttribute('class', `phonics-arc-path arc-${i}`);
    arcSvg.append(path);
    arcPaths.push(path);
  }
  card.append(arcSvg);

  const letterEl = document.createElement('div');
  letterEl.className = 'phonics-letter';
  card.append(letterEl);

  const hint = document.createElement('div');
  hint.className = 'phonics-hint';
  hint.hidden = true;
  const hintEmoji = document.createElement('div');
  hintEmoji.className = 'phonics-hint-emoji';
  const hintWord = document.createElement('div');
  hintWord.className = 'phonics-hint-word';
  hint.append(hintEmoji, hintWord);
  card.append(hint);

  play.append(card);

  const actions = document.createElement('div');
  actions.className = 'phonics-actions';
  const missBtn = document.createElement('button');
  missBtn.className = 'phonics-action phonics-miss';
  missBtn.setAttribute('aria-label', 'Missed');
  missBtn.textContent = '✗';
  const gotBtn = document.createElement('button');
  gotBtn.className = 'phonics-action phonics-got';
  gotBtn.setAttribute('aria-label', 'Got it');
  gotBtn.textContent = '✓';
  const advanceBtn = document.createElement('button');
  advanceBtn.className = 'phonics-action phonics-advance';
  advanceBtn.setAttribute('aria-label', 'Got it now');
  advanceBtn.textContent = '→';
  advanceBtn.hidden = true;
  actions.append(missBtn, gotBtn, advanceBtn);
  play.append(actions);

  root.append(play);

  const done = document.createElement('div');
  done.className = 'phonics-done';
  done.hidden = true;
  done.innerHTML = `
    <div class="phonics-done-card">
      <div class="phonics-done-rainbow">🌈</div>
      <h2>Rainbow!</h2>
      <div class="phonics-mastery" aria-label="Letter mastery"></div>
      <div class="phonics-done-actions">
        <button class="primary phonics-done-again">Play again</button>
        <button class="secondary phonics-done-home">Home</button>
      </div>
    </div>`;
  root.append(done);

  const masteryEl = done.querySelector<HTMLDivElement>('.phonics-mastery')!;
  function paintMastery(): void {
    masteryEl.innerHTML = '';
    // 26 dots, one per letter, colored by Leitner box (0 = gray, 4 = gold).
    // Gives a visible long-term arc across sessions — "look how much
    // you've grown."
    const letters = Object.keys(state.letters).sort();
    letters.forEach((l, i) => {
      const s = state.letters[l];
      const dot = document.createElement('span');
      dot.className = `mastery-dot box-${s?.box ?? 0}`;
      dot.setAttribute('aria-label', `${l}: box ${s?.box ?? 0}`);
      // Stagger so the splash has a final cascade — kid sees their
      // whole alphabet light up after the rainbow lands.
      dot.style.animationDelay = `${800 + i * 22}ms`;
      masteryEl.append(dot);
    });
  }

  // ---------- helpers ----------
  function paintRainbow(justFilledIndex?: number): void {
    // Stars fill outer-to-inner: star 1 lights arc-0 (outermost = red),
    // star GOAL lights arc-(GOAL-1) (innermost = violet). The kid sees a
    // genuine rainbow arc from the very first correct, not just the cool
    // half until ~star 4.
    arcPaths.forEach((p, i) => {
      const isFilled = i < stars;
      p.classList.toggle('filled', isFilled);
      p.classList.toggle('just-filled', justFilledIndex === i);
    });
    // Hide the SVG slot when no arcs are lit yet — the card collapses to
    // just the letter, no dead space above.
    arcSvg.style.visibility = stars === 0 ? 'hidden' : 'visible';
  }

  function hopLetter(hot: boolean): void {
    letterEl.classList.remove('hop', 'hop-hot');
    void letterEl.offsetWidth;
    letterEl.classList.add('hop');
    if (hot) letterEl.classList.add('hop-hot');
    setT(() => letterEl.classList.remove('hop', 'hop-hot'), 700);
  }

  function exposeDebug(): void {
    // Read window.__phonics fresh each time the test inspects; the snapshot
    // here references the live state, so a sync-pull rewrite below is
    // reflected on the next read.
    window.__phonics = {
      letter: currentLetter,
      stars,
      inMissReveal,
      queueLength: queue.length,
      state,
    };
  }

  function showNextCard(): void {
    if (stars >= SESSION_GOAL) {
      showDone();
      return;
    }
    if (queue.length === 0) queue = buildQueue(state);
    const next = queue.shift();
    if (!next) return;
    currentLetter = next;
    inMissReveal = false;
    letterEl.textContent = next;
    card.classList.remove('miss');
    hint.hidden = true;
    missBtn.hidden = false;
    gotBtn.hidden = false;
    advanceBtn.hidden = true;
    exposeDebug();
  }

  function persist(): void {
    save(GAME_ID, STORAGE_NAME, state);
    sync.push(GAME_ID, state);
  }

  function onGotIt(): void {
    if (!currentLetter || inMissReveal) return;
    playTap();
    gotIt(state, currentLetter);
    persist();
    stars += 1;
    streak += 1;
    const newlyLitArcIndex = stars - 1; // outer-to-inner: stars=1 → arc-0
    paintRainbow(newlyLitArcIndex);
    hopLetter(streak >= 3); // hot streak: bigger / tilted hop
    // Pulse the whole rainbow on each fill — the prize itself reacts,
    // not just one arc.
    arcSvg.classList.remove('pulsing');
    void arcSvg.getBoundingClientRect();
    arcSvg.classList.add('pulsing', 'celebrating');
    setT(() => arcSvg.classList.remove('pulsing'), 480);
    setT(() => arcSvg.classList.remove('celebrating'), 650);
    // Streak-aware reward: more confetti + higher pitch as the kid runs hot.
    const streakBoost = Math.min(streak - 1, 5);
    playCorrect(streakBoost);
    burst(BURST_BASE + streakBoost * BURST_STREAK_STEP);
    setT(() => {
      arcPaths.forEach((p) => p.classList.remove('just-filled'));
      if (stars >= SESSION_GOAL) showDone();
      else showNextCard();
    }, ADVANCE_DELAY_MS);
  }

  function onMissed(): void {
    if (!currentLetter || inMissReveal) return;
    playTap();
    missed(state, currentLetter);
    persist();
    streak = 0;
    inMissReveal = true;
    // Canonical exemplar — clean anchor, even if the letter has graduated
    // to its variety set in normal play.
    const ex = pickExemplar(currentLetter, 0);
    hintEmoji.textContent = ex.emoji;
    hintWord.textContent = `like ${ex.word}`;
    hint.hidden = false;
    // Subtle card tint so the recovery moment is visibly distinct from
    // the prompt — without being alarming. Letter stays full strength
    // (errorless).
    card.classList.add('miss');
    missBtn.hidden = true;
    gotBtn.hidden = true;
    advanceBtn.hidden = false;
    queue.splice(REQUEUE_GAP, 0, currentLetter);
    exposeDebug();
  }

  function onAdvance(): void {
    playTap();
    showNextCard();
  }

  function showDone(): void {
    paintMastery();
    done.hidden = false;
    // Stagger the arcs as the splash opens so the rainbow visibly
    // "draws itself" rather than just sitting there.
    arcPaths.forEach((p, i) => {
      p.classList.remove('just-filled');
      setT(() => {
        p.classList.add('just-filled');
        setT(() => p.classList.remove('just-filled'), 700);
      }, i * 120);
    });
    playLevelUp();
    burst(BURST_AT_DONE);
    setT(() => burst(90), 350);
    setT(() => burst(70), 800);
    setT(() => burst(50), 1400);
    void sync.flush();
    exposeDebug();
  }

  const sig = abort.signal;
  gotBtn.addEventListener('click', onGotIt, { signal: sig });
  missBtn.addEventListener('click', onMissed, { signal: sig });
  advanceBtn.addEventListener('click', onAdvance, { signal: sig });
  done.querySelector<HTMLButtonElement>('.phonics-done-again')!.addEventListener(
    'click',
    () => {
      stars = 0;
      paintRainbow();
      queue = buildQueue(state);
      done.hidden = true;
      showNextCard();
    },
    { signal: sig },
  );
  done.querySelector<HTMLButtonElement>('.phonics-done-home')!.addEventListener(
    'click',
    () => opts.onHome(),
    { signal: sig },
  );

  // ---------- cloud pull ----------
  void (async () => {
    const remote = validate(await sync.pull<unknown>(GAME_ID));
    if (!remote) return;
    state = merge(state, remote);
    ensureLetters(state);
    save(GAME_ID, STORAGE_NAME, state);
    // Rebuild queue for the merged view; don't yank the kid mid-card.
    queue = buildQueue(state);
    exposeDebug();
  })();

  // ---------- boot ----------
  paintRainbow();
  showNextCard();
  exposeDebug();

  return () => {
    abort.abort();
    for (const id of timers) clearTimeout(id);
    timers.clear();
    delete window.__phonics;
    container.innerHTML = '';
  };
}
