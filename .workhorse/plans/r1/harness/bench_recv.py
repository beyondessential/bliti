#!/usr/bin/env python3
"""R1 receiver: a counting central that records every notification's arrival.

Scans for the R1BENCH peripheral, subscribes to bliti's device-transmit characteristic
via AcquireNotify (a SEQPACKET socket, so one read is one notification), and records the
arrival time and sequence number of everything that turns up.

Drop rate and delivered rate are receiver-local, so they need no clock sync with the
sender. The sender's timestamp is recorded but not relied on.
"""

import argparse, json, struct, select, socket, sys, time

import dbus, dbus.mainloop.glib

BLUEZ = "org.bluez"
SERVICE_UUID = "63c7f3bc-0599-4a66-bdcd-f28ec571c118"
DEVICE_TX_UUID = "a7aabad6-3fc2-4c9b-953b-03a70a193ec4"
HEADER = struct.Struct(">IQ")


def managed(bus):
    om = dbus.Interface(bus.get_object(BLUEZ, "/"), "org.freedesktop.DBus.ObjectManager")
    return om.GetManagedObjects()


def find_device(bus, adapter, timeout):
    """Scan until the bench peripheral shows up, matching on its service UUID."""
    ad = dbus.Interface(bus.get_object(BLUEZ, f"/org/bluez/{adapter}"), "org.bluez.Adapter1")
    try:
        ad.SetDiscoveryFilter({"UUIDs": dbus.Array([SERVICE_UUID], signature="s"),
                               "Transport": "le"})
        ad.StartDiscovery()
    except dbus.DBusException as exc:
        print(f"[scan] discovery start: {exc}", flush=True)
    deadline = time.monotonic() + timeout
    try:
        while time.monotonic() < deadline:
            for path, ifaces in managed(bus).items():
                dev = ifaces.get("org.bluez.Device1")
                if not dev:
                    continue
                uuids = [str(u).lower() for u in dev.get("UUIDs", [])]
                name = str(dev.get("Name", ""))
                if SERVICE_UUID in uuids or name == "R1BENCH":
                    print(f"[scan] found {path} name={name!r} rssi={dev.get('RSSI')}", flush=True)
                    return path
            time.sleep(0.25)
    finally:
        try:
            ad.StopDiscovery()
        except dbus.DBusException:
            pass
    return None


def prop(bus, path, iface, name):
    p = dbus.Interface(bus.get_object(BLUEZ, path), "org.freedesktop.DBus.Properties")
    return p.Get(iface, name)


def connect(bus, path, timeout):
    dev = dbus.Interface(bus.get_object(BLUEZ, path), "org.bluez.Device1")
    if not bool(prop(bus, path, "org.bluez.Device1", "Connected")):
        dev.Connect()
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if bool(prop(bus, path, "org.bluez.Device1", "ServicesResolved")):
            return True
        time.sleep(0.2)
    return False


def find_char(bus, dev_path):
    for path, ifaces in managed(bus).items():
        ch = ifaces.get("org.bluez.GattCharacteristic1")
        if ch and path.startswith(dev_path) and str(ch.get("UUID", "")).lower() == DEVICE_TX_UUID:
            return path
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--adapter", default="hci0")
    ap.add_argument("--scan-timeout", type=float, default=30.0)
    ap.add_argument("--idle-stop", type=float, default=12.0,
                    help="stop after this many seconds with no packet")
    ap.add_argument("--max-run", type=float, default=600.0)
    ap.add_argument("--out", default="/tmp/r1_recv.json")
    ap.add_argument("--label", default="")
    args = ap.parse_args()

    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SystemBus()

    dev_path = find_device(bus, args.adapter, args.scan_timeout)
    if not dev_path:
        print("[fail] peripheral not found", flush=True)
        sys.exit(2)

    if not connect(bus, dev_path, 30.0):
        print("[fail] services did not resolve", flush=True)
        sys.exit(3)
    print("[conn] connected, services resolved", flush=True)

    char_path = find_char(bus, dev_path)
    if not char_path:
        print("[fail] device-transmit characteristic not found", flush=True)
        sys.exit(4)

    ch = dbus.Interface(bus.get_object(BLUEZ, char_path), "org.bluez.GattCharacteristic1")
    fd_obj, mtu = ch.AcquireNotify({})
    fd = fd_obj.take()
    sock = socket.socket(fileno=fd)
    att_mtu = int(mtu)
    print(f"[sub] subscribed, notify mtu={att_mtu}", flush=True)

    # Keep the read loop as cheap as possible: python overhead here shows up as
    # receiver-side backlog rather than link behaviour.
    from array import array
    t_arr, seq_arr, sz_arr = array("Q"), array("L"), array("H")
    sock.settimeout(0.5)
    t_start = time.monotonic()
    last_packet = t_start
    disconnected_at = None
    unpack = HEADER.unpack_from
    clock = time.monotonic_ns

    while True:
        try:
            data = sock.recv(4096)
        except socket.timeout:
            now = time.monotonic()
            if now - last_packet > args.idle_stop:
                print("[stop] idle", flush=True)
                break
            if now - t_start > args.max_run:
                print("[stop] max run reached", flush=True)
                break
            try:
                if not bool(prop(bus, dev_path, "org.bluez.Device1", "Connected")):
                    disconnected_at = time.monotonic() - t_start
                    print(f"[drop] LINK DOWN at t={disconnected_at:.2f}s", flush=True)
                    break
            except dbus.DBusException:
                disconnected_at = time.monotonic() - t_start
                print(f"[drop] device vanished at t={disconnected_at:.2f}s", flush=True)
                break
            continue
        except OSError as exc:
            disconnected_at = time.monotonic() - t_start
            print(f"[drop] socket error {exc} at t={disconnected_at:.2f}s", flush=True)
            break
        if not data:
            disconnected_at = time.monotonic() - t_start
            print(f"[drop] notify socket closed at t={disconnected_at:.2f}s", flush=True)
            break
        t_arr.append(clock())
        if len(data) >= 12:
            sq, _sn = unpack(data, 0)
        else:
            sq = 0xFFFFFFFF
        seq_arr.append(sq)
        sz_arr.append(len(data))
        last_packet = time.monotonic()

    arrivals = list(zip(t_arr, seq_arr, sz_arr))

    try:
        still = bool(prop(bus, dev_path, "org.bluez.Device1", "Connected"))
    except dbus.DBusException:
        still = False

    # Summarise: a long soak produces too many arrivals to dump verbatim.
    import collections
    per_sec = collections.Counter()
    gaps = []
    if arrivals:
        base = arrivals[0][0]
        prev = None
        for t, q, _z in arrivals:
            per_sec[int((t - base) / 1e9)] += 1
            if prev is not None and q != prev + 1:
                gaps.append([prev, q, q - prev - 1])
            prev = q
        span = (arrivals[-1][0] - base) / 1e9
        seqs = [a[1] for a in arrivals]
        lo, hi = min(seqs), max(seqs)
        expected = hi - lo + 1
        lost = expected - len(arrivals)
    else:
        span, lo, hi, expected, lost = 0, 0, 0, 0, 0

    out = {
        "label": args.label,
        "att_mtu": att_mtu,
        "count": len(arrivals),
        "span_s": span,
        "mean_rate": len(arrivals) / span if span else 0,
        "seq_lo": lo, "seq_hi": hi, "expected": expected, "lost": lost,
        "loss_pct": (100.0 * lost / expected) if expected else 0,
        "gap_events": len(gaps),
        "first_gap": gaps[0] if gaps else None,
        "per_second": [per_sec.get(i, 0) for i in range(int(span) + 1)],
        "disconnected_at": disconnected_at,
        "still_connected": still,
        "arrivals": [[int(a), int(q), int(z)] for a, q, z in arrivals] if len(arrivals) < 200000 else [],
    }
    with open(args.out, "w") as fh:
        json.dump(out, fh)
    print(f"[done] {len(arrivals)} notifications over {span:.1f}s = {out['mean_rate']:.0f}/s", flush=True)
    print(f"[loss] {lost} lost of {expected} expected ({out['loss_pct']:.3f}%), "
          f"{len(gaps)} gap events", flush=True)
    print(f"[link] still_connected={still} disconnected_at={disconnected_at}", flush=True)


if __name__ == "__main__":
    main()
