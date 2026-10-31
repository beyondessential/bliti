# Fake device in dev mode

## Approach

The fake device sits at the same seam as the Playwright fakes: the client object that `App.jsx` and `Provision.jsx` hold, which takes decoded MSG outcomes. It ships in every bundle, not behind `__TEST_SEAM__`.

- Each page holds a client that dispatches between the real client and the fake one, choosing by the payload it holds (main page) or by which find was asked for (provisioning). The rest of the app is unchanged.
- The fake is canned and not scriptable, unlike `tests/fake-client.js` and `tests/fake-provision-client.js`. Those stay as they are, because tests need to script answers that the fake should never give.
- Without the wasm, the fake has to emit its own `onActivity` lines (`in` / `out` / `note`) in the shape `link.js` gives real traffic.
- On the main page the fake device is a reserved code text, which reads to a fake `qr` without the wasm. That's what lets it be remembered and come back after a reload through the existing paths. `remembered.js` keeps it like any code, and the remembered list filters it out of view when dev mode is off.
- The unsupported screen gets the fake-device entry in dev mode on both pages. After disconnecting from the fake on such a browser, the page returns to that screen.

## Follow-up cards

- B5: move the fake behind the transport so the real wasm runs against it.
- C5: make the main page's fake stateful.
- D5: give the provisioning fake a full run, with failures.

## Build

- [x] Fake device for the page that reaches a device (`src/fake/device.js`), routed to by its code from `createClient`
- [x] Fake device for the provisioning page (`src/fake/provision.js`), reached by `find` with `fake` set
- [x] Dev-mode entry on the scan step, the unsupported screen, and the provisioning find, problem and unsupported screens
- [x] Find step for the fake: Connect in place of the chooser, no chip and no code to save
- [x] Unsupported browsers pass the unsupported screen while the fake is held, and return to it once it is let go of
- [x] Remembered list hides the fake outside dev mode
- [x] Tests: `tests/fake-device.spec.js` and `tests/provision-fake.spec.js`, run against the test build and, once, against `vite build`

The fake provisioning device is `running` for a moment before `finished`, because the page ends choosing steps again on a finished run only when it sees the run leave `finished`.
