# Offline-capable service worker in dev and production

Scenarios for the one worker served both by a build and by the dev server.
The automated ones live in `crates/bliti-web/app/tests/`: build scenarios in `offline.spec.js`, `service-worker.spec.js` and `installable.spec.js` against the test build, dev scenarios in `offline-dev.spec.js` (and `installable.spec.js` again) against `vite --mode test`.

## Build

- [x] Load the built bundle online, wait for control, go offline and reload: the app renders and the wasm module fetches (verifies spec: WEB)
- [x] With service workers blocked, the wasm module is fetched at startup, before any code is read (verifies spec: WEB)
- [x] A first visit is controlled once the worker activates, without a reload
- [x] Register a next version whose list renames the wasm module behind an open page: it stays `installed` rather than taking over, and the open page's wasm module still fetches offline (verifies spec: WEB)
- [x] `dist/sw.js` lists every JS, CSS, HTML and wasm file in `dist/` and every file in `public/`, each with a revision, and `networkFirst` is `false`
- [x] A worker source without the placeholder token is refused when the list is injected, which fails `vite build`
- [x] An update whose list changes only the document's revision fetches only `/` at install; every other entry is copied from the old precache
- [x] An update that changes the worker and not its list fetches nothing at install, and the precache in use is left as it is
- [x] An update whose install hits a missing entry never activates, and the old version keeps answering offline
- [x] Once a next version activates, only its own `bliti-precache-*` cache remains, and `bliti-runtime` is kept
- [x] A POST from the page reaches the server and nothing is stored for it in either cache
- [x] `just build` writes gzip, brotli and zstd encodings beside `dist/sw.js` (verifies spec: WEB)

## Dev client

- [x] The dev server's `/sw.js` is the worker source with `networkFirst: true`, a `null` revision on every entry, and a list holding `/`, `/@vite/client`, `/src/main.jsx`, `/src/wasm/bliti_web_bg.wasm` and every file in `public/`
- [x] Every URL a browser requests from the dev server on loading `/` is in the dev list, the document itself as `/`
- [x] Fetching the dev `/sw.js` twice, and again after editing a module's contents without changing its imports, returns identical bytes
- [x] Load the dev client online once, wait for control, go offline and reload: the app renders and the wasm module fetches (verifies spec: WEB)
- [x] Append a marker to a precached module and reload online: the page receives the module with the marker
- [x] Fetch a probe module nothing imports, rewrite it, and fetch again online: the second fetch has the new content; offline, a request with a different `?t=` returns that latest content
- [x] While controlled, change the start screen's title in `src/App.jsx`: the page shows the new title with no navigation
- [x] After an HMR edit, go offline and reload: the page runs the edited module
- [x] The dev origin links `/manifest.webmanifest`, the manifest declares the icons an install needs, and a service worker registers (verifies spec: WEB)

## On a phone

Run through `bliti-web-serve` on this worktree, in Chrome on Android on the tailnet.

- [ ] Load the client once online, turn on airplane mode with Bluetooth left on, reload: the client runs, reads a device's QR code with the camera, and opens a channel to it (verifies spec: WEB)
- [ ] With the dev client open, turn on airplane mode and leave it a while: the page stays put rather than reloading while the dev client looks for its server. Manual only, because emulated offline in the harness reaches neither WebSockets nor the SharedWorker the dev client pings from
- [ ] Back online, edit a component on the laptop: the phone shows the change live
- [ ] Install the client to the home screen from the dev origin, then open it in airplane mode: it runs, with its icon (verifies spec: WEB)
- [ ] On a phone still carrying a production worker on the dev origin, the dev worker waits: the old client runs until every tab of it is closed, and the dev client runs on the next open
- [ ] Clearing the site's data on that phone brings up the dev client at once

## Laptop setup

- [ ] After `bliti-web-serve`, `tailscale serve status` has no `/sw.js` path, and `/sw.js` on the dev origin returns the dev server's worker
- [x] `~/.local/share/bliti-web/sw.js` is gone, and the bliti-prototype skill's known-noise entry names clearing site data for a production worker left on the dev origin
