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

Where a reading is routinely misread, its detail carries a short plain-language note saying what the figure is and what range is normal. Temperature and battery both need one. The note belongs in the reveal rather than on the face: it answers a question the reader has already asked by tapping.

The face is held to a label and a single number. Bars, history, raw readings, per-interface breakdowns and any caveat about how a figure was arrived at all sit behind the tap. Where a reading is in trouble the number itself changes colour, so alarm costs the face no extra element and a device in trouble is legible at arm's length.

The rule is one reading per face, not one number. Throughput is a single reading with two directions, so it keeps one tile carrying both rates rather than splitting into two tiles that would have to be read together anyway.

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

### Board identity

The board's model and revision are reported wherever the machine can answer for them, falling back to the general identity any machine exposes for its vendor, product and version. The field is therefore populated on a development laptop as well as on the target board, rather than existing only on the hardware we ship.

### Disk

One entry per block device, not per mount point, so a device carrying several mounts is not counted once per mount.

### Temperature and throttling

The temperature reading is shown against the board's own declared thresholds rather than an invented scale, so "hot" means what the board means by it.

The detail says in plain words that the figure is the processor core, not the case and not the room, and that a reading in the seventies is normal under load rather than a sign of anything failing. This is not decoration: it is the question we actually get asked, and a number with no frame of reference invites the wrong conclusion from someone standing over a device deciding whether to unplug it.

For the same reason the reading is coloured as a fault only when the board is genuinely in trouble, not merely warm. A device running hot and working is not a device in trouble, and colouring warmth as failure is what teaches an operator to distrust a healthy reading.

Throttling is reported as the two conditions that can be established: the supply voltage being low, and the processor running below the speed it is capable of. A full throttle bitmask is not available on the target platform without a tool that is not installed, and these two cover the faults an operator in the field is looking for. Fan speed is available on the same board and tells an operator whether a hot device is hot because its cooling has stopped.

### Battery

The tile carries state of charge, because that is the number a non-technician reads. Under the tap sit the voltage and the direction of travel.

Direction is derived from how the state of charge moves across the buffered history, not read from the hardware: the gauge fitted reports voltage and charge but no current. It is presented as an observed trend rather than as a hardware reading, and it needs enough history behind it to be steady.

### Network

Physical interfaces are reported, wired and wireless, plus the overlay the fleet is reached over. Loopback and other virtual interfaces are left out.

Throughput is one reading with two directions. Both rates sit on the one tile face, and the tap reveals the graph and the per-interface breakdown.

The graph is mirrored about a shared time axis, with one direction above it and the other reflected below, so the two are read together rather than as separate charts.

The two directions routinely differ by an order of magnitude, so each is scaled to its own peak and the peaks are printed beside them. A shared scale would be truer to the geometry but would flatten the quieter direction to a line, which loses the shape that makes a graph worth showing; stating both peaks keeps the asymmetry visible as a number instead.

### History

Everything live carries history, not only network throughput. A gauge pinned at its limit means something different depending on whether it just got there or has been there for ten minutes, and that distinction is the diagnosis.

The device samples into a ring buffer and sends the window when a client subscribes, so a graph is populated the moment it appears rather than filling from empty while the operator waits.

Sampling starts when the device starts, and starts again when a session opens. It stops after half an hour with no session, so a device sitting unattended is not sampling forever.

The window is about five minutes at full resolution, with no coarsening. That is what a graph on a phone can legibly show, and it answers what the device is doing now rather than what it did overnight. A longer window would not pay for itself while sampling stops after half an hour idle anyway.

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

### The client-to-device direction

Diagnostics is read-only. The only things a client sends are its subscription, its unsubscription, and its own name and version.

The prototype's send-text affordance is removed with the rest of the prototype rather than kept as a debug channel. Deliberate actions on a device are the subject of later features, and each will bring the messages it needs.

### Both ends name themselves, and neither branches on it

The device reports its software version among its static data, and the client displays it, because "what is this thing running" is a question an operator in the field has.

The client names itself and its version to the device, which logs it. This is how we find out what is actually in the field talking to these devices.

Neither end changes its behaviour based on what the other reported. The exchange is for humans and for logs; the moment code branches on it, it becomes the version gate this design rejects.

## Implementation options

### The application is served remotely and installable

The application is always loaded from a hosted origin, and BLE is the only transport to a device. There is no device-side server and no second transport, so there is one code path.

It is installable and works offline once loaded, so a phone that has opened it before is useful at a camp with no connectivity. This is what the offline story needed; a device-side server would have brought a whole second transport and the secure-context problem with it, for the same benefit.

Worth seeing the consequence: a cached installed application is itself a source of version skew, an old client against a device that has since been updated. It is the same case the wire contract already absorbs, arriving by a second route.

### Frontend

A React single-page application built to static files with Vite, on npm.

This brings a node toolchain into a repository that has none, which is the real cost. What it buys is the most ordinary possible client: declarative rendering for a view that is almost entirely live state, and something another person can pick up without learning our conventions first.

Everything protocol-shaped stays in Rust compiled to wasm, as it is now: the key schedule, matching an advertisement, the handshake, the streams, the message framing. The React half drives Web Bluetooth, the camera and the interface, and renders what comes across. The boundary does not move; only what is on the browser side of it does.

The prototype's hand-written `index.html` and `app.js` are removed rather than grown into this.

### Serving and deployment

The sticker encodes `https://bliti.tamanu.app/` as the application's origin, so that is where the application eventually lives. Standing it up there is not this card.

This card puts the built static bundle in reach: local serving works for development against a phone, and the build runs in CI and produces the bundle as an artefact, so deploying later is wiring rather than work.

Local serving needs the stale `bliti-www` unit repointed at this repository and an HTTPS proxy in front of it, because the phone needs a secure origin for Bluetooth and the camera.

### Graphs

Sparklines and the throughput graph are drawn as SVG generated from the sample buffer, with no charting dependency. The shapes this view needs are a small amount of code, and a library can be adopted later if the view outgrows them.

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

### Three levels, and what each one owns

- **Rust tests** own the protocol and the device: framing, handshake, streams, the message shapes, gathering each reading, and the ring buffer's behaviour over time. This is where transport and protocol coverage lives.
- **Playwright** owns the view, fed decoded messages directly with no wasm in the loop. It covers rendering, the tile and tap behaviour, the subscribe and unsubscribe lifecycle around page visibility, and the graphs as the buffer fills.
- **Manual, or agentic against real hardware,** owns Bluetooth. Nothing tries to fake Web Bluetooth.

### The environment this is tested in

- Web Bluetooth cannot be exercised in Chrome on the development laptop: a bluez bug blocks it, fixed upstream but not yet packaged. Every real end-to-end run therefore goes through a phone against the test device, by hand.
- An agent with ssh access to `tamanu-iti-v4-prototype` can drive and inspect the device half of such a run, which is how the readings above were confirmed.
- The local static server is a transient systemd user unit, `bliti-www`, still pointing at the crate's old path under `bestool`. It needs repointing at this repository, and `tailscale serve` currently has no configuration at all, so the HTTPS origin the phone needs is not up either.

### Scenarios worth covering

- A reading the device declares but cannot take is shown as failing, with why; a reading never declared shows nothing at all.
- A reading the client has never heard of renders from its own description.
- A recognised reading gets its bespoke treatment, and the same reading unrecognised still renders.
- Hiding the page unsubscribes; returning to it resubscribes and the graphs carry across the gap.
- Subscribing to a device that has been up for a while shows a populated graph immediately.
- A device with no battery fitted shows no battery tile.
- Two mounts on one block device are counted once.
- The throughput graph is readable in both directions when one is far larger than the other.
- A hot but healthy device is not coloured as a fault, and its detail explains what the reading is.

## Open questions

None outstanding. Every decision the interview opened has been closed; what remains is the shape of the split.

## Notes for the split

- Behaviour and the wire contract fold into specs. The wire contract is not this card's alone: version skew, self-describing readings, and the push-and-subscribe shape are the template every later feature is framed in, so they likely belong in a spec of their own rather than inside a diagnostics spec. `.workhorse/specs/channel.md` already owns the layers beneath them.
- The client half of the feature belongs under its own heading, per the rule at the end of `.workhorse/specs/web-app.md`.
- Implementation options and the architecture decisions become the plan, along with removing the prototype, standing up the React build, and fixing local serving.
- Testing notes become the test cases, split across the three levels.
