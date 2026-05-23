// Hash routing. #/ -> picker, #/<game-id> -> game.

export type Route = { name: 'picker' } | { name: 'game'; id: string };

export function parseHash(hash: string): Route {
  const m = /^#\/([a-z0-9-]+)/i.exec(hash);
  if (m && m[1]) return { name: 'game', id: m[1].toLowerCase() };
  return { name: 'picker' };
}

export function hashFor(route: Route): string {
  return route.name === 'picker' ? '#/' : `#/${route.id}`;
}

export function navigate(route: Route): void {
  const next = hashFor(route);
  if (location.hash === next) {
    window.dispatchEvent(new HashChangeEvent('hashchange'));
  } else {
    location.hash = next;
  }
}
