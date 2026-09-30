# Offline-capable service worker in dev and production

## Design

One hand-written worker, `src/sw.js`, with no dependencies.
What differs between dev and a build is what the plugin injects into it: the precache list, and a `networkFirst` flag, `false` in a build and `true` in dev.

- A precached URL is answered from the precache. With `networkFirst`, it is fetched from the network while online, the fresh response written back to the precache, and the precache answers only when the network fails.
- A navigation is answered as `/index.html`, under the same rule.
- Any other same-origin GET goes to the network, a copy is stored in the runtime cache, and the stored copy answers when the network fails.

In a build the list is the whole bundle (JS, CSS, HTML, the wasm module, `public/` icons, the manifest), so the third rule rarely runs.
In dev the list is the whole module graph, so the precache path (install, carry-forward, activate cleanup, answering offline) runs in dev too, and `networkFirst` keeps live reload working.
Cache keys drop Vite's `t` query parameter in both caches, so an HMR-stamped request finds its module's entry and a day of edits does not pile up one entry per save. A build has no `t` parameter, so this is inert there.

Update behaviour is unchanged: no `skipWaiting`, so a new version waits until no page of the old one is open; `clients.claim()` on activate.
Activation deletes precache caches of other versions and keeps the runtime cache, so a dev worker update does not lose offline.

The precache list stays a literal list of paths in the emitted `sw.js`: `offline.spec.js` reads the wasm path out of it and rewrites it to fake the next version.

## Decisions

- **vite-plugin-pwa is dropped.** Its `generateSW` has no dev path, and its `injectManifest` dev mode is a separate worker at `/dev-sw.js`. A small local Vite plugin writes `sw.js` with the list at build time and serves the same source at `/sw.js` in dev. The web app manifest becomes a static `public/manifest.webmanifest` linked from `index.html`, which also makes the dev origin installable.
- **Dev precaches the module graph, answered network-first.** The precache path is exercised in dev in full. Precaching dev modules cache-first would pin stale code or turn every save into an update that waits, so dev answers precached entries from the network while online. This is the one behavioural difference between dev and a build, and it is a single flag.
- **Install fills the first load in dev as it does in a build.** A first load makes every request before any worker exists (31 on a probe of the dev server). Precaching the graph at install covers them, so the page does not report its resource-timing entries to the worker, and registration is one `register` call in both.
- **A production worker left on the dev origin waits like any update.** The dev worker installs and waits until its tabs close; clearing site data is the escape. Dev and a build keep one update path.
- **An update carries unchanged entries forward.** Each list entry is `{ url, revision }`, the revision a content hash computed by the plugin in a build and `null` in dev, where `networkFirst` refreshes entries anyway. Install copies an entry from an older precache where both match and fetches the rest, so a UI-only release does not re-download the wasm module (570 KB raw). The same behaviour Workbox gives today.

## Worker

- Classic script, no imports, so dev can serve it as-is and registration is identical in both.
- The plugin replaces one placeholder token with the JSON list, and fails the build if the token is missing.
- The precache cache name is derived in the worker from a hash of the list itself, so any change to the list, including the test's rewritten next version, gets a cache of its own.
- Install fetches with `cache: 'no-cache'`, so an unhashed entry (`index.html`, icons, the manifest) is never filled from a stale HTTP cache.
- Non-GET, cross-origin, and the worker's own script pass straight through.
- Rule 3 stores only `ok` responses.

## Plugin

- A local `sw-plugin.js` beside `vite.config.js`.
- Build: in `generateBundle`, list the bundle's JS, CSS, HTML and wasm plus every file in `public/`, hash each, and emit `sw.js`. `precompress.js` then compresses it like any other file.
- Dev: a `configureServer` middleware answers `/sw.js` with the same source, `networkFirst: true`, and the dev list. Registered directly, so it runs ahead of Vite's own middleware and SPA fallback.
- The dev list is walked from `index.html` through the client environment's module graph, transforming each module first so the walk does not depend on what a browser has already requested. It lists `/index.html`, every module URL as the browser requests it (`/@vite/client`, `/@react-refresh`, pre-bundled deps with their `?v=` stamp), the file behind each `?url` import (the wasm module), and `public/`. Sorted, so the worker's bytes change only when the set of URLs does: a new module, or deps re-optimised. The app has no dynamic imports, so a static walk reaches everything.
- A dev worker that changes installs and waits like any update. Harmless, because the active one is network-first.

## Registration

- `main.jsx` registers `/sw.js` with scope `/`, in the test build too, as today.

## Manifest

- `public/manifest.webmanifest` carries what `vite.config.js` declares today. JSON takes no comments, so the note on why a 192px and a 512px icon are needed stays with the `icons` recipe in the justfile, where it already is.
- `includeAssets` and its note go: the list takes all of `public/`.

## Tests

- `playwright.config.js` gains a second web server, `vite --mode test` on its own port, and a `chromium-dev` project for `tests/offline-dev.spec.js`. The existing project ignores that file.
- The dev spec runs serially, because one of its tests edits a real module.
- Offline after one online load: load, wait for control, go offline, reload, and the app renders and the wasm module fetches. Control follows activation, which follows install filling the precache, so no wait on the cache is needed.
- The dev worker lists the wasm module and `/index.html`, read from `/sw.js` as the build test reads it.
- Network-first, precached: rewrite a listed module that nothing renders visibly, reload online, and the page receives the new content. The edit is restored in `finally`.
- Network-first, runtime: write a probe module under the dev root that nothing imports, fetch it, rewrite it, and fetch again. The second fetch returns the new content. Offline, `?t=` of another value returns the latest. The probe is removed in `finally`.
- HMR under the worker: edit a string in a rendered component, the page shows the new string without a reload, and the edit is restored in `finally`.
- `offline.spec.js` keeps passing against the build unchanged. `installable.spec.js` keeps reading `/manifest.webmanifest`.

## Checked

- Vite 7.3's client reconnects over a `vite-ping` WebSocket, not a fetch, so the worker cannot answer it from cache and cause an offline reload loop.
- HMR runs over a WebSocket, which service workers do not see.

## Outside this repo

- Retire the `/sw.js` override in `bliti-web-serve` and the reset worker in `~/.local/share/bliti-web/`, from their source in `~/code/this-laptop/bliti-prototype.md`, and update the skill's known-noise entry about stale clients to the clear-site-data escape.
