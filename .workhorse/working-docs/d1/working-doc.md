---
status: draft
---

# Diagnostic and informational data display

The first real bliti-web feature: a diagnostics view surfacing device health and system data (board, OS, resource gauges, battery, uptime, network), which also settles bliti-web's architecture and RPC framing as the template for all future features.

## Audience and intent

Two audiences, both physically present at the device:

- **Provisioning-time check.** An installer, just after setup, confirming the device came up: did the network attach, is the disk sane, is it on battery or mains.
- **Field troubleshooting.** Someone at a deployed device that is misbehaving, with no network path to it.

The devices are semi-portable. A realistic arrival story: the device turns up at a field camp and a non-technician sets it up as a wifi AP and connects to it over that. Wifi provisioning is a later card, but it is the context this view is read in — the operator may be mid-setup with nothing working yet.

### The operator is within a metre of the device

This is the governing design principle for what the view shows. The operator can see the hardware, so the view should not spend space telling them what is in front of them. Concretely: where a data source is absent because the hardware is absent, omit it rather than reporting its absence. A device with no battery fitted shows no battery tile — the operator can see there is no battery.

Worth noting where this principle stops: it justifies omitting *hardware presence*, not omitting *failures*. A battery that is fitted but unreadable is a fault the operator cannot see, and needs saying.

### The tile is the headline, the tap is the detail

The second governing principle, and what lets one surface serve both audiences. Each thing the view reports shows as a tile carrying the single number or state a non-technician needs. Tapping it reveals the technical detail behind that headline: raw readings, per-unit breakdowns, the source the figure came from.

Nothing is hidden that a non-technician needs, and nothing a technician needs is missing — it is one level down rather than absent.

## Behaviour

### What is reported

Static, sent once on connect:

- Hostname
- Board model and revision
- OS version

Live, while subscribed:

- CPU, RAM, disk
- Battery
- Temperature, and whether the board is throttling
- Uptime
- Network throughput up and down

Network addresses sit between the two: they change rarely but they do change, and they are among the most important things an installer is waiting to see.

### Disk

One entry per block device, not per mount point, so a device carrying several mounts is not counted once per mount.

### Temperature and throttling

The temperature reading is shown against the board's own declared thresholds rather than an invented scale, so "hot" means what the board means by it.

Throttling is reported as the two conditions that can be established: the supply voltage being low, and the processor running below the speed it is capable of. A full throttle bitmask is not available on the target platform without a tool that is not installed, and these two cover the faults an operator in the field is looking for. Fan speed is available on the same board and tells an operator whether a hot device is hot because its cooling has stopped.

### Battery

The tile carries state of charge, because that is the number a non-technician reads. Under the tap sit the voltage and the direction of travel.

Direction is derived from how the state of charge moves across the buffered history, not read from the hardware: the gauge fitted reports voltage and charge but no current. It is presented as an observed trend rather than as a hardware reading, and it needs enough history behind it to be steady.

### Network

Physical interfaces are reported, wired and wireless, plus the overlay the fleet is reached over. Loopback and other virtual interfaces are left out.

Throughput is shown as up and down over time.

### History

Everything live carries history, not only network throughput. A gauge pinned at its limit means something different depending on whether it just got there or has been there for ten minutes, and that distinction is the diagnosis.

The device samples into a ring buffer and sends the window when a client subscribes, so a graph is populated the moment it appears rather than filling from empty while the operator waits.

Sampling starts when the device starts, and starts again when a session opens. It stops after half an hour with no session, so a device sitting unattended is not sampling forever.

### Cadence

Live data updates at a rate that suits what it measures: the fast-moving things often enough to read as live, the slow-moving ones every few seconds. The device holds this policy.

## Wire contract

This is the part of the card that outlives it: whatever shape is settled here is the shape every later feature is framed in.

### Push and subscribe

Static data is pushed once on connect; live data is subscribed to.

The subscription is coarse: one subscription for all live system information, rather than a stream per metric. The client is not expected to want CPU without memory, so per-metric subscription would buy nothing for the complexity it costs.

The client unsubscribes proactively when the operator is not looking, using the browser's page visibility signal, and resubscribes when they come back. This is what keeps a phone in a pocket from pulling samples over BLE.

On subscribing, the client receives the buffered history before the live samples, so a graph is populated the moment it appears.

### Version skew is the normal case

A device runs software months behind the web application, because the application is served fresh and the device is not. A future native application inverts this: installed once, against a device that has since been updated. Both directions are ordinary, and neither may reduce the operator to an error message. The whole point of the system is that someone standing in front of a broken device can reach it.

This rules out a version gate. Nothing in the protocol refuses to proceed on the grounds of the other end's version.

Each end skips what it does not recognise, in both directions: unknown message types and unknown fields alike.

### Readings describe themselves

A device does not send readings a client is expected to already know the meaning of. Each reading carries what it is: its name, what it measures, the unit it is in, and where its limits sit where it has any.

A client renders any reading from that description alone. The consequence is the one that matters: a device that gains a reading appears in an application that has never heard of it, with no application release in between.

Where a client does recognise a reading by name, it may treat it specially — temperature drawn against the board's own trip points, the battery's derived direction and the caveat that goes with it. Generic rendering is the floor that guarantees nothing is ever invisible; recognition is an improvement on top of it, never a precondition for display.

A reading the device declares but cannot currently take is reported as failing, with why. This is distinct from a reading the device never declared, which needs no explanation because nothing claimed it existed. An application therefore never reports a reading as missing: it has no list to miss it from.

### Both ends name themselves, and neither branches on it

The device reports its software version among its static data, and the client displays it, because "what is this thing running" is a question an operator in the field has.

The client names itself and its version to the device, which logs it. This is how we find out what is actually in the field talking to these devices.

Neither end changes its behaviour based on what the other reported. The exchange is for humans and for logs; the moment code branches on it, it becomes the version gate this design rejects.

## Implementation notes

### Device data sources

Verified on the `tamanu-iti-v4-prototype` test device (Raspberry Pi 5 Model B Rev 1.1, revision `d04171`, Ubuntu 26.04, kernel 7.0.0-1017-raspi, aarch64):

- Board model and revision: `/proc/device-tree/model` and the `Revision` line of `/proc/cpuinfo`. Both are Pi-specific.
- Thermal: `thermal_zone0` reads 48.5 C, with trip points declared at 50, 60, 67.5 and 75 C active and 110 C critical. A second thermal reading comes from the NVMe the device boots from.
- Throttling: `vcgencmd` is not installed and there is no `*throttled*` node in sysfs, so the Pi firmware's throttle bitmask is not readable. What is readable is `hwmon/rpi_volt/in0_lcrit_alarm` for undervoltage, and `scaling_cur_freq` against `cpuinfo_max_freq` for frequency capping (both 2400000 at rest). Fan speed comes from `hwmon/pwmfan/fan1_input`.
- Battery: no `upower`, and `/sys/class/power_supply/` is empty — no kernel driver is bound. There is a live fuel gauge on I2C bus 1 at address `0x36`, reading about 4.19 V and about 100% state of charge, consistent with the MAX1704x family. That family reports voltage and state of charge but no current, so charge/discharge direction and time remaining are not directly readable from it.
- Disk: `/` and `/var/lib/postgresql` are the same block device (`/dev/mapper/root`), so a naive mount-point listing reports the same 441 G twice.
- Network: `/proc/net/dev` gives cumulative byte and packet counters per interface. Interfaces present are `end0`, `wld0`, `tailscale0`, `lo`.
- CPU count: 4.

### Where the prototype stands

- `crates/bliti/src/facts.rs` gathers hostname and global addresses.
- `DeviceMessage::Identity` carries them; the device opens a reporting stream unsolicited and re-sends on change, polled every 2 s (`crates/bliti/src/session.rs`).
- The browser half is a hand-written `index.html` and `app.js` driving Web Bluetooth and the camera, with all protocol in wasm (`crates/bliti-web/src/lib.rs`). No bundler, no framework, no tests above the Rust unit level.

## Testing notes

### The testing environment is constrained

- Web Bluetooth cannot be exercised in Chrome on the development laptop: a bluez bug blocks it, fixed upstream but not yet packaged. Every real end-to-end test therefore runs through the user's phone against the test device, by hand.
- That makes an app-level harness worth having: Playwright driving the interface against a faked channel, so the view's own behaviour is testable without Bluetooth in the loop.
- The local static server is a transient systemd user unit, `bliti-www`, still pointing at the crate's old path under `bestool`. It needs repointing at this repository, and `tailscale serve` currently has no configuration at all, so the HTTPS origin the phone needs is not up either.

## Open questions

- [ ] What do board model and revision show on a device that is not a Pi?
- [ ] How long a window does the ring buffer hold, and at what resolution?
- [ ] Which architecture does bliti-web take?
- [ ] How is the client-to-device direction framed, beyond subscribe and unsubscribe?
- [ ] Does the prototype's send-text affordance go entirely, or stay as a development aid behind something?
