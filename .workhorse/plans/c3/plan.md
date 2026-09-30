# Offline-capable service worker in dev and production

## Design

One hand-written worker, `src/sw.js`, with no dependencies and no mode flag.
The only difference between dev and a build is the precache list injected into it.

- A precached URL is answered from the precache.
- A navigation is answered with the precached `index.html` where the list has it.
- Any other same-origin GET goes to the network, a copy is stored, and the stored copy answers when the network fails.

In a build the list is the whole bundle (JS, CSS, HTML, the wasm module, `public/` icons, the manifest), so the third rule rarely runs.
In dev the list is empty or `public/` only, so everything is network-first: live reload keeps working, and offline replays the last load.

Update behaviour is unchanged: no `skipWaiting`, so a new version waits until no page of the old one is open; `clients.claim()` on activate.
Activation deletes precache caches of other versions and keeps the runtime cache, so a dev worker update does not lose offline.

The precache list stays a literal list of paths in the emitted `sw.js`: `offline.spec.js` reads the wasm path out of it and rewrites it to fake the next version.

## Decisions

- **vite-plugin-pwa is dropped.** Its `generateSW` has no dev path, and its `injectManifest` dev mode is a separate worker at `/dev-sw.js`. A small local Vite plugin writes `sw.js` with the list at build time and serves the same source at `/sw.js` in dev. The web app manifest becomes a static `public/manifest.webmanifest` linked from `index.html`, which also makes the dev origin installable.
- **The page reports what it loaded before the worker controlled it.** A first load makes every request before any worker exists (31 on a probe of the dev server), so a worker that only caches what it sees leaves the first load uncached. Once controlled, the page posts its resource-timing entries (30 of the 31: all but the document itself) plus its own URL, and the worker fetches and stores whatever is neither precached nor already stored. A no-op in a build.
- **The runtime cache keys ignore Vite's `t` query parameter,** so each dev module keeps only its latest version rather than one entry per HMR edit. Offline, a request for `?t=N` is answered with the latest stored version of that path.
- **A production worker left on the dev origin waits like any update.** The dev worker installs and waits until its tabs close; clearing site data is the escape. Dev and a build keep one update path.

## Checked

- Vite 7.3's client reconnects over a `vite-ping` WebSocket, not a fetch, so the worker cannot answer it from cache and cause an offline reload loop.
- HMR runs over a WebSocket, which service workers do not see.

## Outside this repo

- Retire the `/sw.js` override in `bliti-web-serve` and the reset worker in `~/.local/share/bliti-web/`, from their source in `~/code/this-laptop/bliti-prototype.md`, and update the skill's known-noise entry about stale clients to the clear-site-data escape.
