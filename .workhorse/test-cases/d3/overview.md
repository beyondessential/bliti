# D3: a client's stale record hides a device from the chooser

Run from a Linux client against a device running this build. `<addr>` is the device's Bluetooth address, and the reproduction in the plan gives the commands.

## The device

- [x] After a client connects, `bluetoothctl info <addr>` on the client gives the advertised local name, not the device's hostname (verifies spec: ADV)
- [x] With a client connected, `systemctl stop bliti` disconnects it first, and its record still lists the bliti service afterwards (verifies spec: CHN)
- [x] With a client connected, killing bliti and letting systemd restart it leaves the client's record listing the bliti service once the leftover connection ends (verifies spec: CHN)
- [x] The daemon's chooser-facing name survives `systemctl restart bliti`: the adapter's alias is the local name again after the restart (verifies spec: ADV)

## The web client

- [x] The chooser is filtered on the local name alone, and the bliti service is asked for as an optional service (verifies spec: WEB)
- [ ] In Chrome on Linux, a client whose record lacks the bliti service (reproduced as the plan describes, on a build before this one) finds the device in the chooser and opens a channel to it (verifies spec: WEB)
- [ ] In Chrome on Android, a phone that could not find the device before this build finds it in the chooser without its Bluetooth state being cleared (verifies spec: WEB)
- [ ] After a session and a disconnect from the page, the chooser finds the device again, three times running (verifies spec: WEB)
