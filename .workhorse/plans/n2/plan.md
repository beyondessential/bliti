# N2: device control

## Tech notes

- Reconnecting without the chooser relies on the `BluetoothDevice` that `client.js` already holds from `requestDevice`: calling `device.gatt.connect()` on it again needs no user gesture. It only works while the device comes back on the same Bluetooth address. BlueZ uses the adapter's public address by default, so the same address is the ordinary case. The Find the device fallback covers the rest.
- `restart` can only be listed where something starts bliti again after it exits. `bliti.service` runs with `Restart=always`, so under that unit the daemon can restart by exiting. The device needs a way to tell it is running under a supervisor that will restart it before it lists `restart`.
- `reboot` and `power-off` need the privilege to shut the system down. The daemon runs as root under `bliti.service`, so both are available there.
- The device sends `going-away` on each open `default` feed, then ends every connection, then acts. The feed runs through the send-rate ceiling of CHN, so "sent" means handed to the link, not received by the client.
- The Control screen replaces the device view's Network settings button. Back from the network screen returns to Control, not to the device view.
- While connected, the device view's `h1` becomes Info, and Control (filled) and Disconnect move into that title row, using the network screen's `heading title` layout. The Device `h2` stays and heads only the identity block: the hostname header from `Readings.jsx` and the software line.
- `HeldBar` renders only on the device view in `App.jsx` today, while NSCR has it on every other connected screen. It moves out of the device view to the app shell, so the Control screen and any later screen carry it without each opting in.
- While an act is under way, the device view stays up: Info title with Disconnect only, the Device identity block, and a `bar-state working` with the network screen's spinner in place of the tiles. Disconnect there stops reconnecting and runs the ordinary `disconnect()`, landing on the QR code read section. So the app needs a state that is neither connected nor disconnected: the device view held with no channel under it.

## Build

- [x] Core: `control`, `acts`, `act` (critical `ACT`), `accepted`, `refused`, `going-away` in `Message`, the generator, and `wire-breaks.toml`
- [x] Daemon: a `control` module holding the acts and one `Controller` shared by every session, with a systemd backend and a recording one for tests
- [x] Daemon: route `control` streams to it, remember each session's `hello` for the log, and send `going-away` on each open `default` feed
- [x] Daemon: after going-away, end every connection through the adapter, then carry the act out; log a failure and take acts again
- [x] Wasm: `Channel::control` and a `ControlHandle` beside `ConfigurationHandle`, sharing the session plumbing
- [x] Web client: `client.control()`, and `reconnect()` reusing the held `BluetoothDevice`
- [x] App: the Info title row, the Control screen, the kept-session bar on every screen but the network one, Back from Network to Control
- [x] App: the going-away state (Info title with Disconnect, identity, progress), reconnecting, giving up, the power-off hold
- [x] Harness: fake `control` and `reconnect` in `fake-client.js`, and Playwright specs for all of it
- [x] Test cases file, `cargo fmt`, `cargo test`, `cargo clippy`, `npm test`
