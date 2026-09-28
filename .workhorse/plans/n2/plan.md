# N2: device control

## Tech notes

- Reconnecting without the chooser relies on the `BluetoothDevice` that `client.js` already holds from `requestDevice`: calling `device.gatt.connect()` on it again needs no user gesture. It only works while the device comes back on the same Bluetooth address. BlueZ uses the adapter's public address by default, so the same address is the ordinary case. The Find the device fallback covers the rest.
- `restart` can only be listed where something starts bliti again after it exits. `bliti.service` runs with `Restart=always`, so under that unit the daemon can restart by exiting. The device needs a way to tell it is running under a supervisor that will restart it before it lists `restart`.
- `reboot` and `power-off` need the privilege to shut the system down. The daemon runs as root under `bliti.service`, so both are available there.
- The device sends `going-away` on each open `default` feed, then ends every connection, then acts. The feed runs through the send-rate ceiling of CHN, so "sent" means handed to the link, not received by the client.
- The Control screen replaces the device view's Network settings button. Back from the network screen returns to Control, not to the device view.
