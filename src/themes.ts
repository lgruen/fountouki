// Item themes — each theme is a small palette of distinct items.
// Items are either text glyphs (emoji/letters/digits) or "shape" tokens that
// the renderer draws as colored shapes via CSS custom properties.

export type ItemKind = 'glyph' | 'shape';

export interface Item {
  /** Stable id; used for equality and as the answer key. */
  id: string;
  kind: ItemKind;
  /** For glyph items: the character(s) to render. */
  glyph?: string;
  /** For shape items: CSS values for color and clip-path. */
  shape?: { color: string; radius?: string; clip?: string };
  /** Accessible name (for aria-labels and debugging). */
  label: string;
}

export type ThemeId =
  | 'emoji-animals'
  | 'emoji-fruit'
  | 'emoji-vehicles'
  | 'emoji-construction'
  | 'emoji-dinosaurs'
  | 'shapes'
  | 'letters-upper'
  | 'letters-lower'
  | 'numbers';

export interface Theme {
  id: ThemeId;
  /** Friendly name for the picker. */
  label: string;
  /** Pool of items to draw from. */
  items: Item[];
}

const glyph = (id: string, char: string, label: string): Item => ({
  id,
  kind: 'glyph',
  glyph: char,
  label,
});

const shape = (
  id: string,
  color: string,
  label: string,
  opts: { radius?: string; clip?: string } = {},
): Item => ({
  id,
  kind: 'shape',
  shape: { color, ...opts },
  label,
});

export const THEMES: Record<ThemeId, Theme> = {
  'emoji-animals': {
    id: 'emoji-animals',
    label: 'Animals',
    items: [
      glyph('dog', '🐶', 'dog'),
      glyph('cat', '🐱', 'cat'),
      glyph('rabbit', '🐰', 'rabbit'),
      glyph('bear', '🐻', 'bear'),
      glyph('panda', '🐼', 'panda'),
      glyph('tiger', '🐯', 'tiger'),
      glyph('frog', '🐸', 'frog'),
      glyph('monkey', '🐵', 'monkey'),
    ],
  },
  'emoji-fruit': {
    id: 'emoji-fruit',
    label: 'Fruit',
    items: [
      glyph('apple', '🍎', 'apple'),
      glyph('banana', '🍌', 'banana'),
      glyph('grapes', '🍇', 'grapes'),
      glyph('strawberry', '🍓', 'strawberry'),
      glyph('orange', '🍊', 'orange'),
      glyph('kiwi', '🥝', 'kiwi'),
      glyph('pear', '🍐', 'pear'),
      glyph('watermelon', '🍉', 'watermelon'),
    ],
  },
  'emoji-vehicles': {
    id: 'emoji-vehicles',
    label: 'Vehicles',
    items: [
      glyph('car', '🚗', 'car'),
      glyph('bus', '🚌', 'bus'),
      glyph('train', '🚂', 'train'),
      glyph('plane', '✈️', 'plane'),
      glyph('rocket', '🚀', 'rocket'),
      glyph('bike', '🚲', 'bike'),
      glyph('boat', '⛵', 'boat'),
      glyph('tractor', '🚜', 'tractor'),
    ],
  },
  'emoji-construction': {
    id: 'emoji-construction',
    label: 'Construction',
    items: [
      glyph('crane', '🏗️', 'crane'),
      glyph('truck', '🚛', 'truck'),
      glyph('digger', '🚜', 'digger'),
      glyph('cone', '🚧', 'traffic cone'),
      glyph('hammer', '🔨', 'hammer'),
      glyph('wrench', '🔧', 'wrench'),
      glyph('saw', '🪚', 'saw'),
      glyph('toolbox', '🧰', 'toolbox'),
    ],
  },
  'emoji-dinosaurs': {
    id: 'emoji-dinosaurs',
    label: 'Dinosaurs',
    items: [
      glyph('trex', '🦖', 'T-rex'),
      glyph('sauropod', '🦕', 'long-neck dino'),
      glyph('croc', '🐊', 'crocodile'),
      glyph('turtle', '🐢', 'turtle'),
      glyph('lizard', '🦎', 'lizard'),
      glyph('dragon', '🐉', 'dragon'),
      glyph('egg', '🥚', 'egg'),
      glyph('bone', '🦴', 'bone'),
    ],
  },
  shapes: {
    id: 'shapes',
    label: 'Shapes',
    items: [
      shape('red-circle', '#ef476f', 'red circle', { radius: '50%' }),
      shape('blue-square', '#118ab2', 'blue square', { radius: '6px' }),
      shape('yellow-triangle', '#ffd166', 'yellow triangle', {
        clip: 'polygon(50% 0, 100% 100%, 0 100%)',
      }),
      shape('green-circle', '#06d6a0', 'green circle', { radius: '50%' }),
      shape('purple-square', '#9b5de5', 'purple square', { radius: '6px' }),
      shape('orange-triangle', '#ff8c42', 'orange triangle', {
        clip: 'polygon(50% 0, 100% 100%, 0 100%)',
      }),
    ],
  },
  'letters-upper': {
    id: 'letters-upper',
    label: 'Letters (ABC)',
    items: [
      glyph('A', 'A', 'A'),
      glyph('B', 'B', 'B'),
      glyph('C', 'C', 'C'),
      glyph('D', 'D', 'D'),
      glyph('E', 'E', 'E'),
      glyph('F', 'F', 'F'),
    ],
  },
  'letters-lower': {
    id: 'letters-lower',
    label: 'letters (abc)',
    items: [
      glyph('a', 'a', 'a'),
      glyph('b', 'b', 'b'),
      glyph('c', 'c', 'c'),
      glyph('d', 'd', 'd'),
      glyph('e', 'e', 'e'),
      glyph('f', 'f', 'f'),
    ],
  },
  numbers: {
    id: 'numbers',
    label: 'Numbers',
    items: [
      glyph('1', '1', 'one'),
      glyph('2', '2', 'two'),
      glyph('3', '3', 'three'),
      glyph('4', '4', 'four'),
      glyph('5', '5', 'five'),
      glyph('6', '6', 'six'),
    ],
  },
};

export const ALL_THEME_IDS: ThemeId[] = Object.keys(THEMES) as ThemeId[];

export function getTheme(id: ThemeId): Theme {
  return THEMES[id];
}
