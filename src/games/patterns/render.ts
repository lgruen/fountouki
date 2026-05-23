// DOM rendering helpers for items, sequence cells, and choice buttons.

import type { Item } from './themes.js';

type CellExtras = {
  /** CSS classes to add (e.g. 'group-a', 'slot'). */
  classes?: string[];
  /** Override text content (for the '?' slot). */
  text?: string;
};

export function renderItemInto(el: HTMLElement, item: Item, extras: CellExtras = {}): void {
  el.className = 'cell';
  if (extras.classes) el.classList.add(...extras.classes);
  el.style.removeProperty('--shape-color');
  el.style.removeProperty('--shape-radius');
  el.style.removeProperty('--shape-clip');
  el.textContent = '';

  if (extras.text !== undefined) {
    el.textContent = extras.text;
    el.setAttribute('aria-label', extras.text);
    return;
  }

  if (item.kind === 'glyph') {
    el.textContent = item.glyph ?? '';
    el.setAttribute('aria-label', item.label);
  } else if (item.kind === 'shape' && item.shape) {
    el.classList.add('shape');
    el.style.setProperty('--shape-color', item.shape.color);
    if (item.shape.radius) el.style.setProperty('--shape-radius', item.shape.radius);
    if (item.shape.clip) el.style.setProperty('--shape-clip', item.shape.clip);
    el.setAttribute('aria-label', item.label);
  }
}

export function makeCell(item: Item | null, extras: CellExtras = {}): HTMLDivElement {
  const el = document.createElement('div');
  if (item) renderItemInto(el, item, extras);
  else {
    el.className = 'cell';
    if (extras.classes) el.classList.add(...extras.classes);
    if (extras.text !== undefined) el.textContent = extras.text;
  }
  return el;
}

export function makeChoiceButton(item: Item): HTMLButtonElement {
  const btn = document.createElement('button');
  btn.className = 'choice';
  btn.setAttribute('data-id', item.id);
  if (item.kind === 'glyph') {
    btn.textContent = item.glyph ?? '';
  } else if (item.kind === 'shape' && item.shape) {
    btn.classList.add('shape');
    btn.style.setProperty('--shape-color', item.shape.color);
    if (item.shape.radius) btn.style.setProperty('--shape-radius', item.shape.radius);
    if (item.shape.clip) btn.style.setProperty('--shape-clip', item.shape.clip);
  }
  btn.setAttribute('aria-label', item.label);
  return btn;
}
