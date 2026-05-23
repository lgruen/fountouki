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
const SESSION_GOAL = 7; // full rainbow — keep in sync with .phonics-rainbow .seg-N rules in style.css.
const REQUEUE_GAP = 2; // how many cards after a miss before the same card re-appears.
const ADVANCE_DELAY_MS = 700; // delay between "got it" and the next card.
const BURST_PER_CARD = 25;
const BURST_AT_DONE = 110;

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
  const rainbow = document.createElement('div');
  rainbow.className = 'phonics-rainbow';
  rainbow.setAttribute('aria-label', 'Rainbow progress');
  for (let i = 0; i < SESSION_GOAL; i++) {
    const seg = document.createElement('span');
    seg.className = `phonics-rainbow-seg seg-${i}`;
    rainbow.append(seg);
  }
  const starsEl = document.createElement('div');
  starsEl.className = 'phonics-stars';
  starsEl.setAttribute('aria-label', 'Stars');
  const starGlyph = document.createElement('span');
  starGlyph.className = 'star';
  starGlyph.textContent = '★';
  const starN = document.createElement('span');
  starN.className = 'phonics-star-count';
  starN.textContent = '0';
  starsEl.append(starGlyph, starN);
  const mute = makeMuteButton();
  topbar.append(home, rainbow, starsEl, mute);
  root.append(topbar);

  const play = document.createElement('div');
  play.className = 'phonics-play';

  const card = document.createElement('div');
  card.className = 'phonics-card';
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
      <div class="phonics-done-actions">
        <button class="primary phonics-done-again">Play again</button>
        <button class="secondary phonics-done-home">Home</button>
      </div>
    </div>`;
  root.append(done);

  // ---------- helpers ----------
  function paintRainbow(): void {
    rainbow.querySelectorAll('.phonics-rainbow-seg').forEach((s, i) => {
      s.classList.toggle('filled', i < stars);
    });
    starN.textContent = String(stars);
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
    letterEl.classList.remove('faded');
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
    paintRainbow();
    playCorrect();
    burst(BURST_PER_CARD);
    setT(() => {
      if (stars >= SESSION_GOAL) showDone();
      else showNextCard();
    }, ADVANCE_DELAY_MS);
  }

  function onMissed(): void {
    if (!currentLetter || inMissReveal) return;
    playTap();
    missed(state, currentLetter);
    persist();
    inMissReveal = true;
    // Canonical exemplar — clean anchor, even if the letter has graduated
    // to its variety set in normal play.
    const ex = pickExemplar(currentLetter, 0);
    hintEmoji.textContent = ex.emoji;
    hintWord.textContent = `like ${ex.word}`;
    hint.hidden = false;
    letterEl.classList.add('faded');
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
    done.hidden = false;
    playLevelUp();
    burst(BURST_AT_DONE);
    setT(() => burst(80), 400);
    setT(() => burst(60), 900);
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
