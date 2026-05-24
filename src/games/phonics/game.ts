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
import { playCorrect, playFrog, playLevelUp, playTap } from '../../shared/sounds.js';
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
  frogTaps: number;
}

declare global {
  interface Window {
    __phonics?: DebugView;
  }
}

// Garden palette for the rainbow-done scene. Emoji-only so no asset
// pipeline. Kept to recognisable preschool-friendly plants/veggies —
// the *variety per session* is what carries the freshness.
const GARDEN_POOL = ['🌻', '🌷', '🌹', '🌼', '🍄', '🌵', '🍓', '🌽', '🥕', '🌺', '🌸'];

// Build the d= attribute for one rainbow arc at the given step (0 =
// outermost). Same geometry as the in-game arcs; used both by the play
// card and the scene-wide done splash so the kid's earned rainbow
// visibly "grows up" rather than getting replaced by a new graphic.
function buildArcPath(index: number, totalArcs: number): string {
  const t = totalArcs === 1 ? 0 : index / (totalArcs - 1);
  const sagitta = ARC_SAGITTA_OUTER - t * (ARC_SAGITTA_OUTER - ARC_SAGITTA_INNER);
  const r = sagitta / (1 - ARC_COS);
  const w = r * ARC_SIN;
  return `M ${(ARC_CX - w).toFixed(2)} ${ARC_Y_HORIZON} A ${r.toFixed(2)} ${r.toFixed(2)} 0 0 1 ${(ARC_CX + w).toFixed(2)} ${ARC_Y_HORIZON}`;
}

function pickGardenPlants(): string[] {
  // 5-6 plants per session, no duplicates within one garden. Variety
  // within a single modal carries freshness since we don't persist
  // a sticker book across sessions.
  const count = 5 + Math.floor(Math.random() * 2);
  const pool = [...GARDEN_POOL];
  const picked: string[] = [];
  for (let i = 0; i < count && pool.length; i++) {
    const j = Math.floor(Math.random() * pool.length);
    picked.push(pool.splice(j, 1)[0]!);
  }
  return picked;
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
    const path = document.createElementNS(SVG_NS, 'path');
    path.setAttribute('d', buildArcPath(i, SESSION_GOAL));
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

  // Rainbow-done scene: not a modal card, a full-viewport scene. Sky +
  // big rainbow that draws itself + rain → garden growth + frog mascot
  // the kid can poke. Buttons sit at the bottom corners so anyone in a
  // rush can leave without waiting for the choreography to finish.
  // Mastery dots intentionally removed — a 4yo with language delays
  // doesn't parse 26 colored squares; the data lives in parent settings
  // where it belongs.
  const done = document.createElement('div');
  done.className = 'phonics-done';
  done.hidden = true;
  done.innerHTML = `
    <div class="phonics-done-sky" aria-hidden="true">
      <div class="phonics-done-sun"></div>
      <div class="phonics-done-cloud cloud-a"></div>
      <div class="phonics-done-cloud cloud-b"></div>
      <div class="phonics-done-cloud cloud-c"></div>
      <div class="phonics-done-cloud cloud-d"></div>
      <div class="phonics-done-cloud cloud-e"></div>
    </div>
    <svg class="phonics-done-arcs" viewBox="0 0 240 80" aria-label="Rainbow"></svg>
    <div class="phonics-done-rain" aria-hidden="true"></div>
    <div class="phonics-done-ground" aria-hidden="true">
      <div class="phonics-done-garden"></div>
      <button class="phonics-frog" type="button" aria-label="Frog">🐸</button>
      <div class="phonics-done-critter" aria-hidden="true"></div>
    </div>
    <div class="phonics-done-actions">
      <button type="button" class="phonics-done-again" aria-label="Play again">↻</button>
      <button type="button" class="phonics-done-home" aria-label="Home">⌂</button>
    </div>`;
  root.append(done);

  // Build the big done-scene rainbow once (reuses the in-game geometry).
  const doneArcSvg = done.querySelector<SVGSVGElement>('.phonics-done-arcs')!;
  const doneArcPaths: SVGPathElement[] = [];
  for (let i = 0; i < SESSION_GOAL; i++) {
    const p = document.createElementNS(SVG_NS, 'path');
    p.setAttribute('d', buildArcPath(i, SESSION_GOAL));
    p.setAttribute('stroke-width', '8');
    p.setAttribute('stroke-linecap', 'round');
    p.setAttribute('fill', 'none');
    p.setAttribute('class', `phonics-arc-path filled arc-${i}`);
    doneArcSvg.append(p);
    doneArcPaths.push(p);
  }

  const gardenEl = done.querySelector<HTMLDivElement>('.phonics-done-garden')!;
  const rainEl = done.querySelector<HTMLDivElement>('.phonics-done-rain')!;
  const critterEl = done.querySelector<HTMLDivElement>('.phonics-done-critter')!;
  const frogEl = done.querySelector<HTMLButtonElement>('.phonics-frog')!;
  let frogTaps = 0;
  let plantEls: HTMLElement[] = [];

  // Frog reactions cycle through 4 short moves. No new payoff after the
  // cycle ends — the kid plays a few times, sees it loop, naturally moves
  // on. We never want to invent a side game in the reward modal.
  const FROG_REACTIONS = ['react-hop', 'react-big', 'react-ribbit', 'react-blink'] as const;

  // Sprout delays for plants. Drops fall ~SPROUT_BASE_DELAY_MS before the
  // matching plant lands so the visual story is "rain → growth". Kept
  // as a const so paintGarden + spawnRaindrops can't drift apart silently.
  const SPROUT_BASE_DELAY_MS = 600;
  const SPROUT_STAGGER_MS = 90;

  function paintGarden(): void {
    gardenEl.innerHTML = '';
    plantEls = [];
    const plants = pickGardenPlants();
    // Plants are DECOR — non-interactive spans. The frog is the only
    // tappable focal in the scene; if plants also looked tappable
    // (round emoji, focus ring, wiggle response) a 4yo would expect
    // frog-level reactions and the modal would drift into "side game in
    // the reward" territory. Errorless interaction means one focal.
    // Split left/right of center so the frog at 50% sits in a visible
    // gap. Corner gaps (14%..40% on left, mirrored on right) keep
    // runway for the bottom-corner play-again / home buttons.
    const n = plants.length;
    const half = Math.ceil(n / 2);
    plants.forEach((emoji, i) => {
      const el = document.createElement('span');
      el.className = 'phonics-plant';
      el.setAttribute('aria-hidden', 'true');
      el.textContent = emoji;
      const onLeft = i < half;
      const localIdx = onLeft ? i : i - half;
      const localCount = onLeft ? half : n - half;
      const span = localCount > 1 ? localIdx / (localCount - 1) : 0.5;
      const leftPct = onLeft ? 20 + span * 22 : 58 + span * 22;
      const yJitter = ((i * 53) % 7) - 3; // deterministic small offset
      el.style.left = `${leftPct.toFixed(1)}%`;
      el.style.setProperty('--plant-y', `${yJitter}px`);
      el.style.setProperty('--sprout-delay', `${SPROUT_BASE_DELAY_MS + i * SPROUT_STAGGER_MS}ms`);
      gardenEl.append(el);
      plantEls.push(el);
    });
  }

  function spawnRaindrops(targets: HTMLElement[]): void {
    rainEl.innerHTML = '';
    // One drop per plant, anchored to the same column. Drop animation
    // is ~700ms; plant sprout starts at SPROUT_BASE_DELAY_MS + i*stagger
    // — start the drop ~SPROUT_BASE earlier so it visibly lands just as
    // the plant scales in. "Rainbow → rain → growth" without words.
    targets.forEach((t, i) => {
      const drop = document.createElement('div');
      drop.className = 'phonics-raindrop';
      drop.style.left = t.style.left || '50%';
      drop.style.setProperty('--drop-delay', `${i * SPROUT_STAGGER_MS}ms`);
      rainEl.append(drop);
    });
  }

  function maybeSpawnCritter(): void {
    // 60% of the time, send a butterfly across after the garden settles.
    // Surprise > entitlement — the kid shouldn't *expect* it every session.
    critterEl.innerHTML = '';
    if (Math.random() > 0.6) return;
    const bug = document.createElement('span');
    const choices = ['🦋', '🐝', '🐞'];
    bug.textContent = choices[Math.floor(Math.random() * choices.length)] ?? '🦋';
    bug.className = 'phonics-critter-bug';
    critterEl.append(bug);
  }

  // Force a CSS animation to restart even when the same class is already
  // present — toggling + reading offsetWidth flushes layout so the
  // browser treats the re-add as a fresh animation. Used for both the
  // frog reactions and the in-game letter hop.
  function restartAnim(el: HTMLElement, cls: string, clearMs: number): void {
    el.classList.remove(cls);
    void el.offsetWidth;
    el.classList.add(cls);
    setT(() => el.classList.remove(cls), clearMs);
  }

  function onFrogTap(): void {
    frogTaps += 1;
    const reaction = FROG_REACTIONS[(frogTaps - 1) % FROG_REACTIONS.length]!;
    FROG_REACTIONS.forEach((r) => frogEl.classList.remove(r));
    restartAnim(frogEl, reaction, 700);
    playFrog();
    exposeDebug();
  }

  frogEl.addEventListener('click', onFrogTap, { signal: abort.signal });

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
      frogTaps,
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
    hintWord.textContent = ex.word;
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
    // Reset scene state for a fresh splash (kid hits "play again" and
    // earns another rainbow → garden re-randomises, frog tap-cycle starts
    // over, critter may or may not appear again).
    frogTaps = 0;
    FROG_REACTIONS.forEach((r) => frogEl.classList.remove(r));
    paintGarden();
    spawnRaindrops(plantEls);
    maybeSpawnCritter();
    done.hidden = false;
    // Stagger the big done-scene arcs so the rainbow visibly "draws
    // itself" across the viewport instead of just appearing.
    doneArcPaths.forEach((p, i) => {
      p.classList.remove('just-drawing');
      setT(() => {
        p.classList.add('just-drawing');
        setT(() => p.classList.remove('just-drawing'), 700);
      }, i * 110);
    });
    playLevelUp();
    burst(BURST_AT_DONE);
    setT(() => burst(80), 380);
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
      frogTaps = 0;
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
