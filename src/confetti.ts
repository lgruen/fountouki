// Tiny canvas-based confetti burst. Self-contained, no deps.
// One short burst per call; particles fade after ~1.2s.

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  size: number;
  color: string;
  rot: number;
  vr: number;
  life: number; // remaining seconds
}

const COLORS = ['#f582ae', '#ffd166', '#06d6a0', '#118ab2', '#9b5de5', '#ef476f'];

let canvas: HTMLCanvasElement | null = null;
let ctx2d: CanvasRenderingContext2D | null = null;
let particles: Particle[] = [];
let rafId = 0;
let lastTs = 0;

function ensureCanvas(): HTMLCanvasElement | null {
  if (canvas) return canvas;
  const el = document.getElementById('confetti');
  if (!(el instanceof HTMLCanvasElement)) return null;
  canvas = el;
  ctx2d = canvas.getContext('2d');
  resize();
  window.addEventListener('resize', resize);
  return canvas;
}

function resize(): void {
  if (!canvas) return;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(window.innerWidth * dpr);
  canvas.height = Math.floor(window.innerHeight * dpr);
  canvas.style.width = `${window.innerWidth}px`;
  canvas.style.height = `${window.innerHeight}px`;
  if (ctx2d) ctx2d.setTransform(dpr, 0, 0, dpr, 0, 0);
  // Drop any in-flight particles so a viewport change (e.g. orientation
  // flip) doesn't leave a cloud of confetti stranded at coordinates that
  // were computed for the previous layout.
  particles = [];
}

function rand(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

/** Trigger a confetti burst rising from the sequence row. */
export function burst(count = 80): void {
  if (!ensureCanvas() || !ctx2d) return;
  // Anchor the burst to the live sequence card so the celebration lands
  // where the kid is actually looking, regardless of where the play area
  // ends up sitting on screen (phone landscape vs. tablet portrait vs.
  // tablet landscape — different vertical positions in each).
  let cx = window.innerWidth / 2;
  let emitY = Math.min(window.innerHeight * 0.55, 380);
  let spreadX = 60;
  const seq = document.getElementById('sequence');
  if (seq) {
    const r = seq.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) {
      cx = r.left + r.width / 2;
      // Just below the sequence card so particles rise up *through* it
      // (past the answer the kid just placed) before falling away.
      emitY = r.bottom;
      spreadX = Math.min(r.width / 3, 140);
    }
  }
  for (let i = 0; i < count; i++) {
    particles.push({
      x: cx + rand(-spreadX, spreadX),
      y: emitY + rand(-10, 10),
      vx: rand(-220, 220),
      vy: rand(-360, -180),
      size: rand(6, 10),
      color: COLORS[Math.floor(Math.random() * COLORS.length)] ?? '#f582ae',
      rot: rand(0, Math.PI * 2),
      vr: rand(-6, 6),
      life: rand(1.0, 1.6),
    });
  }
  if (!rafId) {
    lastTs = performance.now();
    rafId = requestAnimationFrame(tick);
  }
}

function tick(ts: number): void {
  const dt = Math.min(0.05, (ts - lastTs) / 1000);
  lastTs = ts;
  if (!ctx2d || !canvas) return;
  ctx2d.clearRect(0, 0, canvas.width, canvas.height);
  const g = 600; // px/s^2
  const remaining: Particle[] = [];
  for (const p of particles) {
    p.life -= dt;
    if (p.life <= 0) continue;
    p.vy += g * dt;
    p.x += p.vx * dt;
    p.y += p.vy * dt;
    p.rot += p.vr * dt;
    const alpha = Math.max(0, Math.min(1, p.life));
    ctx2d.save();
    ctx2d.globalAlpha = alpha;
    ctx2d.translate(p.x, p.y);
    ctx2d.rotate(p.rot);
    ctx2d.fillStyle = p.color;
    ctx2d.fillRect(-p.size / 2, -p.size / 2, p.size, p.size * 0.6);
    ctx2d.restore();
    remaining.push(p);
  }
  particles = remaining;
  if (particles.length > 0) {
    rafId = requestAnimationFrame(tick);
  } else {
    rafId = 0;
  }
}
