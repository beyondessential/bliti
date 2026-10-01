# Offline-capable service worker in dev and production

## Design

One hand-written worker, `src/sw.js`, with no dependencies.
What differs between dev and a build is what the plugin injects into it: the precache list, and a `networkFirst` flag, `false` in a build and `true` in dev.

- A precached URL is answered from the precache. With `networkFirst`, it is fetched from the network while online, the fresh response written back to the precache, and the precache answers only when the network fails.
- A navigation is answered as `/`, under the same rule. The document is listed as `/` rather than `/index.html`, because an origin may redirect `/index.html` to `/`, and a redirected response cannot answer a navigation.
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
- A version that changes the worker and not its list has the same precache name as the one in use, so its install finds that precache complete (its list is written last) and does nothing. Otherwise a failed fetch would delete the precache the active version answers from.
- Install fetches with `cache: 'no-cache'`, so an unhashed entry (`index.html`, icons, the manifest) is never filled from a stale HTTP cache.
- Non-GET, cross-origin, and the worker's own script pass straight through.
- Rule 3 stores only `ok` responses.
- Network-first waits a few seconds before a stored copy answers, because a phone in airplane mode with Tailscale connected holds tailnet connections open rather than failing them, which left the installed dev client on its splash screen. Once one request has waited that long, the rest of the load is answered from the cache at once, since imports chain about six deep and each would otherwise wait its turn. Every navigation asks the network again, so the first page loaded once it answers clears that.

## Plugin

- A local `sw-plugin.js` beside `vite.config.js`.
- Build: in `generateBundle`, list the bundle's JS, CSS, HTML and wasm plus every file in `public/`, hash each, and emit `sw.js`. `precompress.js` then compresses it like any other file.
- Dev: a `configureServer` middleware answers `/sw.js` with the same source, `networkFirst: true`, and the dev list. Registered directly, so it runs ahead of Vite's own middleware and SPA fallback.
- The dev list is walked from `index.html` by transforming each module through the client environment and following the specifiers in the transformed code, read with `es-module-lexer`. Not the module graph: it records a module without the query the browser asks for it by (`?import&url` is `?url` there). The walk lists `/`, every module URL as the browser requests it (`/@vite/client`, `/@react-refresh`, pre-bundled deps with their `?v=` stamp), the file behind each `?url` import (the wasm module), and `public/`. Stripped of `t` and sorted, so the worker's bytes change only when the set of URLs does: a new module, or deps re-optimised. The app has no dynamic imports, so a static walk reaches everything.
- A dev worker that changes installs and waits like any update. Harmless, because the active one is network-first.

## Registration

- `main.jsx` calls `keepReady()` from `src/offline.js`, which registers `/sw.js` with scope `/`, in the test build too, and registers again on the browser's `online` event while no worker controls the page. An install cut short leaves nothing behind, so that is how a phone that lost its connection mid-install becomes ready offline once it is back.

## Offline readiness

- A precache is complete or discarded, so a phone either holds everything offline or nothing. Between the first load and the end of install it holds nothing, and a browser offers to install the application before then.
- The start screen says "Not ready offline" in red, right-aligned in the title row, until a worker controls the page, and nothing once one does. `useOfflineReady()` reads `navigator.serviceWorker.controller` and follows `controllerchange`. One label for both waiting and failed, because the retry on reconnecting means the operator does the same thing either way.
- The Web application spec's "Installation and offline use" separates staying usable while the page is open from opening offline, which needs the app to be ready offline.

## Manifest

- `public/manifest.webmanifest` carries what `vite.config.js` declares today. JSON takes no comments, so the note on why a 192px and a 512px icon are needed stays with the `icons` recipe in the justfile, where it already is.
- `includeAssets` and its note go: the list takes all of `public/`.

## Tests

- `playwright.config.js` gains a second web server, `vite --mode test` on its own port, and a `chromium-dev` project for `tests/offline-dev.spec.js`. The existing project ignores that file.
- The dev spec runs serially, because one of its tests edits a real module.
- Offline after one online load: load, wait for control, go offline, reload, and the app renders and the wasm module fetches. Control follows activation, which follows install filling the precache, so no wait on the cache is needed.
- The dev worker lists the wasm module and `/`, read from `/sw.js` as the build test reads it.
- Network-first, precached: rewrite a listed module that nothing renders visibly, reload online, and the page receives the new content. The edit is restored in `finally`.
- Network-first, runtime: write a probe module under the dev root that nothing imports, fetch it, rewrite it, and fetch again. The second fetch returns the new content. Offline, `?t=` of another value returns the latest. The probe is removed in `finally`.
- HMR under the worker: edit a string in a rendered component, the page shows the new string without a reload, and the edit is restored in `finally`.
- `offline.spec.js` keeps passing against the build unchanged. `installable.spec.js` keeps reading `/manifest.webmanifest`.

## Checked

- Vite 7.3's client reconnects over a `vite-ping` WebSocket, not a fetch, so the worker cannot answer it from cache and cause an offline reload loop.
- HMR runs over a WebSocket, which service workers do not see.
- The dev client pings from a SharedWorker. Playwright's emulated offline reaches neither WebSockets nor that worker, so "stays put when the connection drops" is a manual check on the phone.
- `vite preview` answers a missing file with the page and a 200. A real origin answers 404, which is what fails an install; the harness routes the missing entry to a 404 to show it.

## Checklist

Paths are under `crates/bliti-web/app/` unless stated.

### Worker

- [x] `src/sw.js`: a classic script that parses `{ networkFirst, entries }` from the placeholder token, and names its precache `bliti-precache-` plus a SHA-256 of the list's JSON, and its runtime cache `bliti-runtime`
- [x] Cache key function: same-origin URL with the fragment and the `t` query parameter removed, used for every read and write in both caches
- [x] Install: store the list itself in the precache under a reserved key; for each entry, copy it from an older `bliti-precache-*` whose stored list has the same URL and revision, otherwise fetch with `cache: 'no-cache'`; fail the install if any entry fails, so a half-filled precache never activates
- [x] Activate: delete every `bliti-precache-*` but its own, keep `bliti-runtime`, `clients.claim()`
- [x] Fetch: pass non-GET, cross-origin and `/sw.js` through; map navigations to `/index.html`; answer precached keys cache-first, or network-first with write-back under `networkFirst`; everything else network-first into `bliti-runtime`, storing only `ok` responses
- [x] Carry over the note from `vite.config.js` on why a new version waits (the lazily loaded wasm module a page left running still needs), next to the absence of `skipWaiting`

### Plugin and config

- [x] `sw-plugin.js`: read `src/sw.js`, and throw where the placeholder token is missing
- [x] Build: in `generateBundle`, list every emitted chunk and asset with the extensions `js`, `css`, `html`, `wasm`, plus every file in `public/`, each with a truncated SHA-256 of its bytes as revision, and emit `sw.js` with `networkFirst: false`
- [x] Dev: a `configureServer` middleware for `/sw.js` answering `application/javascript` with `networkFirst: true`, `revision: null` on every entry, and the walked list
- [x] Dev walk: transform `index.html` through `server.transformIndexHtml`, take the entry URLs from its script `src` attributes and inline module imports, then transform each module through the client environment and follow the specifiers in its transformed code (`es-module-lexer`, added as a dev dependency); add the file behind each `?url` import, `/`, and `public/`; strip `t`; sort
- [x] Check the walked list against a recorded load: every URL a browser requests from the dev server on `/` is in it, excepting the document itself (listed as `/index.html`)
- [x] `vite.config.js`: drop `VitePWA`, add the plugin, and move the installable-and-offline note (WEB) to `sw-plugin.js`
- [x] `npm uninstall vite-plugin-pwa`, so `package.json` and `package-lock.json` both lose it

### Manifest and registration

- [x] `public/manifest.webmanifest` with the fields and icons `vite.config.js` declares today
- [x] `index.html`: `<link rel="manifest" href="/manifest.webmanifest">` beside the icon links
- [x] `src/main.jsx`: register `/sw.js` with scope `/` where `navigator.serviceWorker` exists, a failure logged and otherwise ignored

### Tests

- [x] `playwright.config.js`: a second web server, `vite --mode test --port 5174 --strictPort`, and a `chromium-dev` project with that base URL matching `offline-dev.spec.js` and `installable.spec.js`; the `chromium` project ignores `offline-dev.spec.js`
- [x] `tests/offline-dev.spec.js`, serial: the dev `/sw.js` lists `/` and `/src/wasm/bliti_web_bg.wasm` with `networkFirst: true`
- [x] Offline after one online load: load, wait for control, go offline, reload; `#root` renders and the wasm module fetches
- [x] Network-first, precached: append a marker comment to a listed module, reload online, the response for it carries the marker; restored in `finally`
- [x] Network-first, runtime: a probe module under `src/` that nothing imports, fetched, rewritten, fetched again with the new content; offline, a different `?t=` returns the latest; removed in `finally`
- [x] HMR: while controlled, change the start screen's `<h1>` text in `src/App.jsx`, the page shows it without a navigation; restored in `finally`
- [x] `offline.spec.js` and `installable.spec.js` pass against the build with no changes to either
- [x] The rest of the automatable scenarios in the test cases, in `tests/service-worker.spec.js` (first-visit claim, carry-forward fetching only what changed, a failed install never activating, activate cleanup, POST pass-through, the emitted list, a missing token refused by the exported `inject`) and `tests/offline-dev.spec.js` (stable dev worker bytes, offline after an HMR edit). The offline reload loop moved to the manual phone checks

### Laptop (outside this repo)

Before the phone check: `tailscale serve` answers `/sw.js` on the dev origin with the reset worker, so the phone would never see the dev worker.

- [x] In `~/code/this-laptop/bliti-prototype.md`: drop the `--set-path /sw.js` step from `bliti-web-serve` and the `bliti-web-sw-reset.js` source, and reword the skill's known-noise entry on older clients to a production worker waiting until its tabs close, with clearing site data as the escape
- [x] Reinstall per that file's install table, remove `~/.local/share/bliti-web/sw.js`, clear the `/sw.js` path from `tailscale serve`, and commit in `this-laptop`

### Verify

- [x] `just build`: `dist/sw.js` lists the bundle and `public/`, and `precompress.js` writes its encodings
- [x] `npx playwright test` passes both projects
- [ ] `bliti-web-serve` on this worktree, then on a phone: load once online, airplane mode, reload, and the client runs; back online, an edit to a component shows live
