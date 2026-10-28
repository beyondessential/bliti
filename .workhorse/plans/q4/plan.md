# Q4: provisioning client page

The page at `/provision` (PWEB), plus what the provisioning stream (PRS) gains for it: the revision, the image's date and the record. The device side of those members lands here too, since K4 shipped without them.

## Notes

- The client is tightly coupled to the device's steps: provisioning is run from the latest deployed app, and the PRS revision and image date are what refuse a medium written for other steps. The device side of the revision, the image date and the `record` result shape land in this card too (`bliti-core` report types and the provisioning stream), since K4 shipped without them.
- The step ids `hardware` and `eeprom`, and the prompt ids `case` and `id-wp`, are fixed by PWEB for the pictures. The card that builds those steps uses them.
- The device's step registry is still empty, so the page is built and tested in Playwright against a fake client at the message layer. Hands-on testing waits until enough of provisioning exists, and then runs on the prototype.
- The board photos move from `.workhorse/design/assets/` into the app's assets. The uncropped original stays in `.workhorse/design/assets/` for re-cropping.
- The link to and from the assembly manual is left to G4.
- A step-specific card may override PWEB's generic handling. For example, the Tailscale card may keep the auth key for the session, where PWEB discards `secret` answers once they are sent.
- The device has to be told the image's build date by means outside the protocol. Here that is a required `--image-built` flag on `bliti provision`. The unit reads it from an environment file at a provisional path, and the image-build card settles where that file really lives on the medium.
- `wire-breaks.toml` records nothing before the first release, so the new required members need no entry there.

## Protocol (`bliti-core`)

- [ ] `provisioning::REVISION` (1), and the range of revisions a client built from this crate implements (1 to 1), in `provisioning.rs`
- [ ] `Message::ProvisionState` gains `revision` and `built`, with `built` as a `jiff::civil::Date` (already a dependency of `bliti`; `bliti-core` takes it too, which has to build for wasm), read and written as an RFC 3339 full date
- [ ] A `Record` type in `provisioning/report.rs` (`suffix`, `version`, `disk`, `passphrase`, `at`), whose `Debug` hides the passphrase, plus a helper that finds the record among the steps' results
- [ ] Update the wire generators in `channel/generate.rs` and the message tests for `revision`, `built` and a step result carrying `record`

## Device (`bliti`)

- [ ] `Snapshot` in `provision/run.rs` carries the revision and the image date into `to_message`
- [ ] `--image-built` on the `provision` subcommand in `main.rs`, through `daemon::Options` to the run
- [ ] `services/bliti-provision.service` passes `--image-built` from an `EnvironmentFile`, at a provisional path noted in `services/README.md`
- [ ] The console command shows the revision and the image date with the state (PCON shows every part of the state a client shows)
- [ ] Session and stream tests: every `provision-state` carries the revision and the date it was given

## wasm (`bliti-web`)

- [ ] A provisioning constructor for `Channel` that holds no QR payload: the provisioning token's pre-shared key, and the handshake as an initiator that compares no fingerprint (the variant already in `channel/noise.rs`). `Inner` holds either a payload or nothing, in place of a payload alone
- [ ] Exports for finding a board: the name prefixes to filter the chooser on, one for each version marker implemented, and reading a picked name into its marker, whether the marker is supported, and the suffix or `FAULT`
- [ ] `Channel::provision` in `streams.rs`: opens the stream with `provision`, and returns a handle with `act(act, steps, prompt, value)` and `close`
- [ ] Export the client's revision range
- [ ] Rust tests in `tests.rs` for the prefixes, name reading and the provisioning handshake against a device in provisioning mode

## Shared client pieces

- [ ] Move the reaching-a-device step dots (`Steps`) and `Problem` out of `Connect.jsx` into a shared module both pages import. The page reuses `Title`, `Activity` and dev mode from `Screen.jsx` as they are
- [ ] Move the styles the page shares with the Status screen (the `.health` rows and marks, the bar-state panels) where both pages load them, and add the new pieces: the board picture with its caption, a prompt beneath a step row, the walk-away line, and the faded last-seen list

## The page

- [ ] `provision.html` at the app's top level, titled "Provision an Iti", with the same icons and manifest links as `index.html`, loading `src/provision/main.jsx`
- [ ] `src/provision/client.js`, the browser half: the chooser filtered by `namePrefix` on the exported prefixes with the ADV service as optional, the open via the provisioning `Channel`, the slot-full error, reconnecting to the picked device without the chooser, and the provisioning stream. The activity record never carries a `secret` answer
- [ ] `src/provision/state.js`, pure and unit-testable: refusing a medium (revision below or above the range, image date 30 days or more before today by the phone's clock), which screen a state shows, the default ticks, finding the record, and the picture for a step and prompt
- [ ] `Provision.jsx`, the finding screens: press USER with its picture, opening the chooser, nothing picked, connecting with cancel, something else connected, the browser unable to open a channel at all
- [ ] Refusal screens: the stick out of date (with the build date when refused by date), and the page out of date with Reload. Neither shows the steps or sends an act
- [ ] Choosing screen: every step in order with its checkbox and reason, ticked unless `done`, and Start disabled with nothing ticked
- [ ] Run screen: every step with its state (`unchosen` as skipped), a progress bar, each open prompt beneath its step by kind (notice text, yes or no, text field and Save, hidden field with Show and Save, text alone for an unknown kind), the answer cleared once a `secret` is sent, a refused answer shown beneath its prompt, the failure reason with Try again and Stop, Stop while `waiting`, and the walk-away line
- [ ] Pictures beneath the `hardware`/`case` and `eeprom`/`id-wp` prompts, with their captions
- [ ] Done screen: the record (code, version, disk, saved time, passphrase with Copy), "Also saved to the USB stick", the power-off instruction, Provision another Iti (disconnects) and Run steps again (back to choosing). It never sends `acknowledge`
- [ ] Lost screen: the last state faded and marked as last seen, no act offered, "Press USER to check back" with Find Iti, and reconnecting on its own while the page is shown, until it reaches the board or the operator opens the chooser
- [ ] Disconnect beside the title while connected
- [ ] `// spec: PWEB` and `// spec: PRS` comments where the code implements them

## Pictures

- [ ] Re-encode the three cropped rev B photos for phone width and move them into the app's assets (imported, so they are hashed and precached). Remove the cropped copies from `.workhorse/design/assets/`, keeping the original there
- [ ] Check that the service worker precaches the page's images, so the pictures show offline once the app is ready offline

## Tests

- [ ] A fake provisioning client at the message layer (`tests/fake-provision-client.js`), supplied through the test seam as `window.__blitiProvisionClient`, which records every act sent and lets a test emit any `provision-state`
- [ ] Finding: the chooser's prefixes, nothing picked, cancel while connecting, something else connected
- [ ] Refusing: a revision below and above the range, an image 30 days old refused and one 29 days old accepted, and no act sent in any of them
- [ ] Choosing: default ticks from done-states, ticking and unticking, Start disabled with nothing ticked, the `start` act's steps
- [ ] The run: each prompt kind answered, an unknown kind not answerable, a refused answer, pictures for the fixed ids only, progress, retry and stop, and a `secret` field empty after Save
- [ ] Done: the record shown, Copy, no `acknowledge` ever sent, Run steps again, Provision another
- [ ] Lost: the faded last state with no acts, and reconnecting on its own
- [ ] `pages.spec.js` covers `/provision` in place of the fixture page; delete `tests/second-page.html` and `tests/second-page.js`, and drop the test-mode entry from `vite.config.js`
- [ ] `state.js` unit tests for the date boundary and revision range, if the harness has a unit runner; otherwise as Playwright evaluations
