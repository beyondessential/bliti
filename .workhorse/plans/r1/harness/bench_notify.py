#!/usr/bin/env python3
"""R1 sender: unthrottled notification source for measuring the real BLE ceiling.

Registers a GATT peripheral carrying bliti's real service and device-transmit
characteristic UUIDs. When a central subscribes, BlueZ hands us a SEQPACKET socket via
AcquireNotify; one write on it is one notification. We then push sequence-numbered
packets either as fast as the socket accepts them (blast) or through a stepped-hold
rate ladder, and record what we actually achieved.

No throttle: this measures the thing CHN's 200/s is a guess at.
"""

import argparse, json, os, select, socket, struct, sys, time

import dbus, dbus.mainloop.glib, dbus.service
from gi.repository import GLib

BLUEZ = "org.bluez"
SERVICE_UUID = "63c7f3bc-0599-4a66-bdcd-f28ec571c118"
DEVICE_TX_UUID = "a7aabad6-3fc2-4c9b-953b-03a70a193ec4"
APP_PATH = "/org/bes/r1bench"

# seq (u32 BE) + monotonic send time in ns (u64 BE), then filler.
HEADER = struct.Struct(">IQ")

args = None
result = {"steps": [], "acquired": None}
CHAR = None
RUN = 1


class Characteristic(dbus.service.Object):
    def __init__(self, bus, index, service_path):
        self.path = f"{service_path}/char{index}"
        self.service_path = service_path
        self.sock = None
        self.notifying = False
        super().__init__(bus, self.path)

    def props(self):
        return {
            "UUID": DEVICE_TX_UUID,
            "Service": dbus.ObjectPath(self.service_path),
            "Flags": dbus.Array(["notify"], signature="s"),
            "Notifying": dbus.Boolean(self.notifying),
            "NotifyAcquired": dbus.Boolean(self.sock is not None),
        }

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return self.props()

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface, name):
        return self.props()[str(name)]

    @dbus.service.signal("org.freedesktop.DBus.Properties", signature="sa{sv}as")
    def PropertiesChanged(self, interface, changed, invalidated):
        pass

    @dbus.service.method("org.bluez.GattCharacteristic1", in_signature="a{sv}", out_signature="hq")
    def AcquireNotify(self, options):
        """Socket path: BlueZ hands back a SEQPACKET fd, one write per notification."""
        if args.path == "signal":
            print("[acquire] refusing (forced signal path)", flush=True)
            raise dbus.exceptions.DBusException("org.bluez.Error.NotSupported")
        mtu = int(options.get("mtu", 0))
        device = str(options.get("device", "?"))
        ours, theirs = socket.socketpair(socket.AF_UNIX, socket.SOCK_SEQPACKET)
        self.sock = ours
        self.notifying = True
        result["acquired"] = {"path": "socket", "mtu": mtu, "device": device, "at": time.time()}
        print(f"[acquire] AcquireNotify: socket path, mtu={mtu} device={device}", flush=True)
        fd = dbus.types.UnixFd(theirs.fileno())
        GLib.idle_add(lambda: (run_sweep(SocketSink(ours, mtu)), False)[1])
        ret = (fd, dbus.UInt16(mtu))
        theirs.close()
        return ret

    @dbus.service.method("org.bluez.GattCharacteristic1")
    def StartNotify(self):
        """Signal path: each notification is a PropertiesChanged on Value. No backpressure."""
        if self.notifying:
            return
        self.notifying = True
        result["acquired"] = {"path": "signal", "mtu": None, "at": time.time()}
        print("[acquire] StartNotify: signal path (PropertiesChanged)", flush=True)
        GLib.idle_add(lambda: (run_sweep(SignalSink(self)), False)[1])

    @dbus.service.method("org.bluez.GattCharacteristic1")
    def StopNotify(self):
        print("[acquire] StopNotify", flush=True)
        self.notifying = False


class SocketSink:
    """One write is one notification. A full socket is BlueZ applying backpressure."""
    kind = "socket"

    def __init__(self, sock, mtu):
        self.sock = sock
        self.mtu = mtu
        sock.setblocking(False)

    def send(self, pkt):
        self.sock.send(pkt)

    def wait_writable(self, timeout):
        select.select([], [self.sock], [], max(0.0, timeout))


class SignalSink:
    """Emit PropertiesChanged on Value, the way bluer's CharacteristicNotifyMethod::Fun does."""
    kind = "signal"

    def __init__(self, char):
        self.char = char
        self.mtu = None

    def send(self, pkt):
        self.char.PropertiesChanged(
            "org.bluez.GattCharacteristic1",
            {"Value": dbus.Array(pkt, signature="y")},
            [],
        )

    def wait_writable(self, timeout):
        time.sleep(min(timeout, 0.001))


class Service(dbus.service.Object):
    def __init__(self, bus):
        self.path = f"{APP_PATH}/service0"
        super().__init__(bus, self.path)
        self.char = Characteristic(bus, 0, self.path)
        global CHAR
        CHAR = self.char

    def props(self):
        return {"UUID": SERVICE_UUID, "Primary": dbus.Boolean(True)}

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return self.props()


class Application(dbus.service.Object):
    def __init__(self, bus):
        super().__init__(bus, APP_PATH)
        self.service = Service(bus)

    @dbus.service.method("org.freedesktop.DBus.ObjectManager", out_signature="a{oa{sa{sv}}}")
    def GetManagedObjects(self):
        return {
            dbus.ObjectPath(self.service.path): {"org.bluez.GattService1": self.service.props()},
            dbus.ObjectPath(self.service.char.path): {
                "org.bluez.GattCharacteristic1": self.service.char.props()
            },
        }


class Advertisement(dbus.service.Object):
    PATH = f"{APP_PATH}/adv0"

    def __init__(self, bus):
        super().__init__(bus, self.PATH)

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {
            "Type": "peripheral",
            "ServiceUUIDs": dbus.Array([SERVICE_UUID], signature="s"),
            "LocalName": dbus.String("R1BENCH"),
            "Discoverable": dbus.Boolean(True),
        }

    @dbus.service.method("org.bluez.LEAdvertisement1")
    def Release(self):
        pass


def send_packets(sink, target_rate, duration, size, seq_start):
    """Push packets for `duration` seconds. target_rate None means as fast as it takes them.

    Returns per-step stats. EAGAIN counts are the backpressure signal: a socket that
    refuses writes is BlueZ telling us the controller has not drained yet.
    """
    seq = seq_start
    attempted = wrote = eagain = 0
    blocked_ns = 0
    filler = bytes(max(0, size - HEADER.size))
    start = time.monotonic()
    deadline = start + duration
    interval = (1.0 / target_rate) if target_rate else 0.0
    next_send = start

    while True:
        now = time.monotonic()
        if now >= deadline:
            break
        if target_rate:
            if now < next_send:
                time.sleep(min(next_send - now, deadline - now))
                continue
            next_send += interval
            # Do not let a late pacer bank credit and burst.
            if next_send < now:
                next_send = now
        pkt = HEADER.pack(seq & 0xFFFFFFFF, time.monotonic_ns()) + filler
        attempted += 1
        try:
            sink.send(pkt)
            wrote += 1
            seq += 1
        except BlockingIOError:
            eagain += 1
            t0 = time.monotonic_ns()
            # Wait for the socket to drain rather than spinning; this is the backpressure.
            sink.wait_writable(deadline - time.monotonic())
            blocked_ns += time.monotonic_ns() - t0
        except OSError as exc:
            return {
                "error": str(exc), "attempted": attempted, "wrote": wrote,
                "eagain": eagain, "seq_start": seq_start, "seq_end": seq,
                "elapsed": time.monotonic() - start, "blocked_s": blocked_ns / 1e9,
            }

    elapsed = time.monotonic() - start
    return {
        "attempted": attempted, "wrote": wrote, "eagain": eagain,
        "seq_start": seq_start, "seq_end": seq, "elapsed": elapsed,
        "achieved_rate": wrote / elapsed if elapsed else 0,
        "blocked_s": blocked_ns / 1e9,
    }


def out_path():
    base, _, ext = args.out.rpartition(".")
    return f"{base}_run{RUN}.{ext}" if base else f"{args.out}.run{RUN}"


def save():
    """Write results after every step: a long unattended run must survive a link loss."""
    with open(out_path(), "w") as fh:
        json.dump({"args": vars(args), "run": RUN, "result": result}, fh, indent=2)


def run_sweep(sink):
    global RUN, result
    seq = 0
    try:
        if args.program_file:
            program = json.load(open(args.program_file))
            print(f"[program] {len(program)} phases, sink={sink.kind}", flush=True)
            for n, ph in enumerate(program, 1):
                rate, size = ph["rate"], ph["size"]
                hold, gap = ph.get("hold", 15), ph.get("gap", 8)
                label = ph.get("label", f"phase{n}")
                print(f"[step] {label}: {rate}/s for {hold}s, payload {size}B", flush=True)
                step = send_packets(sink, rate, hold, size, seq)
                step.update(target_rate=rate, size=size, label=label)
                seq = step["seq_end"]
                result["steps"].append(step)
                save()
                print(f"[step] {json.dumps(step)}", flush=True)
                if "error" in step:
                    print("[step] link failed; stopping program", flush=True)
                    break
                if gap:
                    time.sleep(gap)
        elif args.mode == "blast":
            print(f"[blast] {args.duration}s, payload {args.size}B, no pacing, sink={sink.kind}", flush=True)
            step = send_packets(sink, None, args.duration, args.size, seq)
            step["target_rate"] = None
            step["size"] = args.size
            result["steps"].append(step)
            save()
            print(f"[blast] {json.dumps(step)}", flush=True)
        else:
            for rate in args.rates:
                print(f"[step] target {rate}/s for {args.hold}s, payload {args.size}B", flush=True)
                step = send_packets(sink, rate, args.hold, args.size, seq)
                step["target_rate"] = rate
                step["size"] = args.size
                seq = step["seq_end"]
                result["steps"].append(step)
                save()
                print(f"[step] {json.dumps(step)}", flush=True)
                if "error" in step:
                    print("[step] link failed; stopping ladder", flush=True)
                    break
                time.sleep(args.gap)
    finally:
        result["sink"] = sink.kind
        result["finished_at"] = time.time()
        save()
        print(f"[done] run {RUN} wrote {out_path()}", flush=True)
        if args.repeat:
            # Re-arm for another run: the next subscription starts the program again.
            RUN += 1
            result = {"steps": [], "acquired": None}
            if CHAR is not None:
                CHAR.notifying = False
                CHAR.sock = None
            print(f"[ready] waiting for a central to subscribe (run {RUN})", flush=True)
        else:
            GLib.idle_add(lambda: (loop.quit(), False)[1])


def main():
    global args, loop
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=["blast", "ladder"], default="blast")
    ap.add_argument("--duration", type=float, default=10.0, help="blast seconds")
    ap.add_argument("--hold", type=float, default=5.0, help="ladder seconds per step")
    ap.add_argument("--gap", type=float, default=1.0, help="idle seconds between steps")
    ap.add_argument("--size", type=int, default=20, help="notification payload bytes")
    ap.add_argument("--rates", type=int, nargs="+",
                    default=[100, 200, 400, 800, 1600, 3200, 6400])
    ap.add_argument("--repeat", action="store_true",
                    help="re-arm after each program so several runs can be done back to back")
    ap.add_argument("--program-file", default=None,
                    help="JSON list of {rate,size,hold,gap} phases, run in order on subscribe")
    ap.add_argument("--path", choices=["auto", "signal"], default="auto",
                    help="auto allows the AcquireNotify socket; signal forces PropertiesChanged")
    ap.add_argument("--adapter", default="hci0")
    ap.add_argument("--out", default="/tmp/r1_sender.json")
    args = ap.parse_args()

    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SystemBus()
    adapter = f"/org/bluez/{args.adapter}"

    app = Application(bus)
    adv = Advertisement(bus)

    gatt = dbus.Interface(bus.get_object(BLUEZ, adapter), "org.bluez.GattManager1")
    le = dbus.Interface(bus.get_object(BLUEZ, adapter), "org.bluez.LEAdvertisingManager1")

    gatt.RegisterApplication(APP_PATH, {},
                             reply_handler=lambda: print("[gatt] registered", flush=True),
                             error_handler=lambda e: (print(f"[gatt] FAILED {e}", flush=True), sys.exit(1)))
    le.RegisterAdvertisement(Advertisement.PATH, {},
                             reply_handler=lambda: print("[adv] advertising as R1BENCH", flush=True),
                             error_handler=lambda e: (print(f"[adv] FAILED {e}", flush=True), sys.exit(1)))

    loop = GLib.MainLoop()
    print("[ready] waiting for a central to subscribe", flush=True)
    try:
        loop.run()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
