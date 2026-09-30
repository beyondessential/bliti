# D3: a client's stale record hides a device from the chooser

## Diagnosis

The chooser does not match advertisements. It matches the host's record of a device, and on Chrome over BlueZ that record is `Device1.Name` and `Device1.UUIDs`, both of which a connection rewrites.

- **UUIDs.** Once BlueZ has resolved a device's services, the record's UUIDs are the resolved services, and the service UUIDs it hears advertised are ignored from then on. A record whose cached database lacks the bliti service therefore never matches a `services` filter again. Only a connection can correct it, because BlueZ re-reads the Database Hash on connecting, and the chooser is what stands in the way of that connection.
- **How a record loses the service.** A client connected while bliti withdraws its service gets Service Changed from the device's bluetoothd, re-resolves, and caches a database without bliti. On its next start the daemon ended carried-over connections *before* registering its application, so that client was disconnected before it could learn the service was back. A deploy, `systemctl restart bliti`, or a crash with a client connected all do it.
- **Name.** On connecting, BlueZ's GAP profile reads the Device Name characteristic and stores it as the record's name. The device served its adapter alias, the hostname, so after any connection the record's name is `tamanu-iti-v4-prototype` and not the advertised payload. Advertising puts the payload back only when a report carrying the scan response gets past the host's discovery filter, and with a UUID filter in place a report of the scan response alone does not. That is the second case on the card: bliti cached, and the chooser still empty.

Chrome on Linux reads only these two properties when it matches (`MatchesFilter` in `bluetooth_device_chooser_controller.cc`; `UpdateServiceUUIDs` in `bluetooth_device_bluez.cc`). BlueZ's handling is `device_add_eir_uuids` and `dev_property_get_uuids` in `src/device.c`, and `read_device_name_cb` in `profiles/gap/gas.c`.

## Reproducing it

On a Linux client, with the device advertising and bliti's service registered:

1. `bluetoothctl --timeout 8 scan le`, then `bluetoothctl connect <addr>`. `bluetoothctl info <addr>` now lists the bliti UUID. Before the fix it also shows the name as the device's hostname.
2. On the device, `sudo systemctl stop bliti`, then `sudo systemctl start bliti`.
3. `bluetoothctl info <addr>`: before the fix, the UUIDs are Generic Access, Generic Attribute and Device Information only.
4. A scan filtered as Chrome filters it (`menu scan`, `uuids <bliti service>`, `transport le`, `back`, `scan on`) restores the name but never the UUID, and the web client's chooser stays empty.

`bluetoothctl remove <addr>` clears it on Linux. After the fix, a stale record of the service no longer stands in the chooser's way, so no client has to be cleared.

## Fix

- [x] Device: serve the local name as the adapter's alias, so the GAP Device Name a client reads is the name it filters on (ADV).
- [x] Device: register the GATT application before ending connections carried over from before the start, so a client still connected is told the service is back (CHN).
- [x] Device: on stopping, withdraw the advertisement and end every connection before the application goes, so no client is shown the database without the service (CHN).
- [x] Web client: filter the chooser on the local name alone, with the bliti service in `optionalServices` (WEB).
- [x] Deploy to v4 and rerun the reproduction. With the laptop connected, a clean stop, and a kill followed by systemd's restart, both leave its record listing the bliti service and giving the local name.
- [ ] Check the chooser in Chrome on Linux, from a record made stale on a build before this one.
- [ ] Check on an Android phone that already holds a stale record: whether the name-only chooser finds the device without clearing Bluetooth state.

## Not fixed here

- v4's sparse advertising (about two advertisements heard in twelve seconds) is unexplained. The intervals are set at 100 to 150 ms. Scans from this laptop found v4 at once while reproducing, so it may come from Wi-Fi coexistence on the CYW43455 and not from bliti.
