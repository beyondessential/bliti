# R1 — Measure the real notification rate ceiling

Establish the notification rate at which CHN's link actually degrades, rather than the
conservative `200/s` guess (a cautious step back from a once-seen crash at ~3400 notifications
at once). Determine whether the ceiling varies by peer / adapter / ATT_MTU, and whether a live
backoff signal serves better than a fixed number.

## Apparatus

- **Sender:** the Pi prototype `tamanu-iti-v4-prototype`, reachable over ssh (`ssh ubuntu@tamanu-iti-v4-prototype`). Peripheral / GATT server. Pi 4 built-in Cypress controller, BT 5.0, BlueZ 5.85.
- **Receiver (workhorse):** this machine as a bluer central (`hci0`). Precise, scriptable arrival timestamps and sequence tracking. Carries the reproducible sweep.
- **Receiver (real target, final):** a phone running Chrome / Web Bluetooth, driven by the user against a page served locally over HTTPS. Run last, to check the number holds on the real deployment peer.

The real throttle is what we are setting the number for, so runs use a purpose-built harness that bypasses it — not the real `serve`/`connect` path. Each notification carries a sequence number and a send-timestamp; the receiver records arrival time, sequence gaps (drops), reordering, and connection drops.

## Decisions (locked)

- **Layer:** raw notifications on `DEVICE_TX` for the sweep (isolate the controller ceiling), then one full-stack CHN run at the chosen number to confirm.
- **Sweep shape:** stepped hold — hold offered rate R for a few seconds, record, step up, repeat until it breaks. Separates sustainable rate from burst, and a gradual knee from a cliff.
- **Fixed vs adaptive:** decided after the sweep, from the data — does one number fit all peers/MTUs, or does a live signal fit better.

## The crux: the notify path and backpressure

The device sends via `CharacteristicNotifyMethod::Fun` + `notifier.notify(chunk).await` (bluer 0.17). That await backpressures when the client acquired notifications over a socket (`AcquireNotify`, modern BlueZ), and is fire-and-forget when it falls back to D-Bus `PropertiesChanged`. Which path is in play decides whether the Pi can overrun the controller at all, and is the same signal the adaptive question turns on. `btmon` on the Pi shows the path, the controller's Number-Of-Completed-Packets flow control, and any disconnect reason code.

## Experiment sequence

0. **Reproduce the crash / find the blast ceiling.** Unthrottled, send as fast as the socket allows, `btmon` watching HCI on the Pi. If it crashes, capture the disconnect reason and the rate at failure. If it will not crash (backpressure holds it), that reframes the card toward the adaptive answer.
1. **Stepped-hold sustained sweep** across offered rate. Record delivered vs *achieved-offered* rate (measure what the sender actually managed; do not assume R), drop rate, latency, disconnects.
2. **Vary payload size and ATT_MTU.** Today's chunk is a conservative 20 bytes; a larger MTU allows more per notification. Check whether the ceiling is a notification count or moves with bytes/MTU.
3. **Burst tolerance.** Instantaneous N-at-once, since CHN's window is per-second and the original failure was a burst.
4. **Full-stack confirmation** at the candidate number over the real CHN path.
5. **Phone Web Bluetooth run**, user-driven via the local HTTPS page.

## Instrumentation notes

- `btmon` on the Pi is the gold standard for controller overrun: HCI flow control (NOCP), ACL buffer, disconnect reason codes.
- Drop rate and delivered rate need no cross-machine clock sync (receiver-local). One-way latency does — either sync clocks (chrony/ptp) or measure round-trip via an echo on the client-transmit characteristic instead.

## Deliverable

Written-up measurements, then the CHN "Send rate" section and the `device.rs` constants updated to match. Note: this branch's `device.rs` still carries both `NOTIFY_BYTES_A_SECOND` and `NOTIFY_PACKETS_A_SECOND`, though L1 already removed the byte ceiling from the CHN spec as redundant — R1's code change subsumes that cleanup.

## Measurements (2026-09-22)

Pi 4 prototype (Cypress controller, BlueZ 5.85) as peripheral, Linux/Intel central (BlueZ 5.87) as
receiver, same room. Negotiated ATT_MTU 517, connection interval 45 ms, supervision timeout 420 ms.
Raw notifications on the real `DEVICE_TX` characteristic, no Noise/zlib/yamux, throttle bypassed.

### The notify path has no backpressure

`device.rs` uses `CharacteristicNotifyMethod::Fun`. In bluer 0.17 that is served under `StartNotify`
and each notification is a fire-and-forget `PropertiesChanged` D-Bus message; for a notification
(as against an indication) there is no confirmation to await. The alternative,
`CharacteristicNotifyMethod::Io`, is served under `AcquireNotify` and gets a SEQPACKET socket that
does push back.

Measured, blasting unpaced for 10 s at 20-byte payloads:

- **Signal path (what the device uses):** 402,675 writes accepted, 40,267/s, **zero** would-block,
  **zero** seconds spent blocked. The sender believed every send succeeded while ~91% of it was
  being discarded downstream.
- **Socket path (`Io`):** 1,180,576 writes, 4,914 would-blocks, **13.0 of 15 s spent blocked**.

So the device today cannot detect overrun from its own send path, which is why a fixed ceiling was
needed at all. A live signal is available, but only on the `Io` path.

### Notifications are coalesced, so the count is not what the link spends

Every notification on air arrived inside an ATT **Handle Multiple Value Notification (0x23)** PDU.
At ATT_MTU 517 a PDU holds 504 bytes, and a 20-byte notification costs 24 bytes in it
(2 handle + 2 length + 20 value), so ~21 notifications ride in one PDU.

| payload | notifications per PDU | PDUs/s | notifications/s | KiB/s |
| --- | --- | --- | --- | --- |
| 20 B | ~21 | 170.7 | 3,585 | 84.0 |
| 500 B | 1 | 161.7 | 162 | 79.6 |

PDUs per second and bytes per second are near-constant across a 25× change in payload, while the
notification rate moves by 22×. **The notification count is the one quantity that does not hold
still**, which is what makes a fixed notification ceiling the wrong shape.

### Where it breaks: a cliff, not a knee

Stepped hold, 8 s per step at 20-byte payloads, sender alive throughout:

| offered | delivered | lost | on air |
| --- | --- | --- | --- |
| 200/s | 100.0% | 0 | 4.7 KiB/s |
| 500/s | 100.0% | 0 | 11.7 KiB/s |
| 1000/s | 100.0% | 0 | 23.4 KiB/s |
| 2000/s | 100.0% | 0 | 46.7 KiB/s |
| 3000/s | 81.1% | 4,533 | 70 KiB/s, link lost 6.5 s in |

Delivery tracked the offered rate exactly, with no loss and no rising latency, right up to the point
the link went down. Every failure was `HCI Disconnect Complete, Reason: Connection Timeout (0x08)` —
a supervision timeout, not a graceful close. So it falls over rather than degrading, and no
drop-rate signal appears early enough to back off on.

### What this says about CHN's 200/s

- At the 20-byte chunk the device actually sends, the link carried **2,000/s losslessly**: the
  ceiling is about **10× conservative**.
- At maximum payload the link sustained only ~**162/s**, so the permitted 200/s is *above* what the
  link held. The single number is wrong in both directions depending on payload.
- The quantities that stayed put were ~162–171 PDU/s and ~80–84 KiB/s. The byte ceiling L1 removed
  as redundant was closer to the real invariant than the notification count that replaced it.

### Caveats

- Longest clean hold at 2,000/s was 8 s. That is not a sustained-safety proof; a long soak at the
  candidate number is still owed.
- One peer and one adapter only. Connection interval (45 ms) and supervision timeout (420 ms) are
  peer-negotiated, and both bear directly on the ceiling, so a phone may land elsewhere. The
  Web Bluetooth run on Chrome is the outstanding check.

### The `Io` socket path does not give usable flow control

Measured directly, since it was the promising candidate for replacing the number with a live signal:
`CharacteristicNotifyMethod::Io` (`AcquireNotify`), unpaced, 20-byte payloads, 60 s requested.

| | signal (`Fun`) | socket (`Io`) |
| --- | --- | --- |
| accepted by the sender | 40,267/s | 78,594/s |
| would-blocks | 0 | 5,066 (13.4 s of 15.5 s blocked) |
| carried on air | ~3,585/s | ~3,459/s (163.6 PDU/s, 80.5 KiB/s) |
| delivered / accepted | ~9% | ~4.4% |
| outcome | `Connection Timeout (0x08)` | `Connection Timeout (0x08)`, socket reset at 15.5 s |

The socket does push back, but nowhere near the link: it accepted 1,218,578 writes in 15.5 s while
the air carried 53,130 of them, so it let through about **22× the link's capacity** before the
connection was lost anyway. BlueZ drains the socket into its own queue far faster than the air
retires it, and discards the excess, so a write that blocks is reporting on that queue rather than
on the connection.

So `Io` buys a partial signal, not flow control. It does not remove the need for a ceiling, and
swapping the device onto it would not by itself make the device adaptive. Saturation kills the link
in roughly 15 s on either path, while half of saturation (2,000/s at 20 B, 47 KiB/s) ran clean.

### Correction: there was never any loss

An earlier reading of the coarse ladder recorded the 3,000/s step as "81.1% delivered, 4,533 lost".
That was offered-minus-delivered arithmetic, not loss. Checked by sequence number, that step had
**zero gaps**: it ran at the full 3,018/s and was cut short when the link dropped 6.5 s into an 8 s
hold. Every run since has been the same — **0 gap events at every rate, on every step**.

This holds because the BLE link layer retransmits until acknowledged, so a notification that reaches
the air arrives. What was discarded in the unpaced blasts was dropped on the sender side inside
BlueZ's queue and never reached the air at all, which is precisely what a receiver cannot observe.

### Soak and the located cliff

| offered | KiB/s | held | loss |
| --- | --- | --- | --- |
| 2,000/s | 46.9 | 10 min, 1,200,000 notifications, link alive at the end | 0 |
| 2,200/s | 51.6 | clean 30 s | 0 |
| 2,400/s | 56.2 | clean 30 s | 0 |
| 2,600/s | 60.9 | clean 30 s | 0 |
| 2,800/s | 65.6 | link lost ~8 s in | 0 |
| 3,000/s | 70.3 | link lost 6.5 s in | 0 |

The soak is the load-bearing result: 2,000/s at 20 B sustained ten minutes with zero loss and no
disconnect, confirmed on air (57,219 PDUs, no disconnect events). The cliff sits between
**61 and 66 KiB/s**, giving the soaked figure about a 1.3× margin.

### Why client-fed-back loss cannot drive backoff

At 2,800/s the receiver counted full-rate delivery for seven consecutive seconds
(2793, 2772, 2814, 2835, 2772, 2835, 2835 per second) and then the connection was gone. A loss
metric reported by the client would have read 0.000% in every one of those seconds.

So loss is not a leading indicator here; it is constant zero until the link dies. Delivered rate is
no better, since it tracks the offered rate exactly over the same window. Neither quantity moves
before the failure, so a feedback message carrying them would arrive too late to act on, whatever
feed it travelled over.

Web Bluetooth also exposes no delivery or loss accounting of its own: it surfaces
`characteristicvaluechanged` for notifications that arrive, and nothing about ones that did not. A
client can only derive loss where the application protocol numbers its packets, and the channel of
[CHN](../../specs/channel.md) is a byte stream rather than datagrams, so a missing chunk does not
present as a gap. It corrupts the Noise transport message or the zlib stream, which is already
specified to close the connection.

### Second peer: Android Chrome over Web Bluetooth

Same device, same harness, ladder from 500/s to 4,000/s in eight 15 s steps (269,998 notifications
offered). The phone behaved nothing like the Linux central.

| | Linux/BlueZ central | Android Chrome |
| --- | --- | --- |
| negotiated ATT_MTU | 517 | 517 |
| notification PDU | Multiple Value (0x23), ~21 coalesced | single Handle Value (0x1b), one per PDU |
| PDU payload | 504 B | 22 B |
| connection interval | 45 ms | 7.5 ms |
| supervision timeout | 420 ms | 5,000 ms |
| sustained ceiling | ~3,585 notifications/s | ~1,000 notifications/s (peak 1,010, median 840) |
| air throughput | 84 KiB/s | 17.3 KiB/s |
| when over-offered | link lost above 2,600/s | no loss, no disconnect, backlog drains late |

The phone received **269,998 of 269,998 with zero gaps and never dropped the link**, taking 335.8 s
to absorb a ladder the sender delivered in 184 s. Offering beyond its rate cost lateness, not loss.

Three things follow.

**The ceiling is not a notification count.** Two peers at the same ATT_MTU differ 3.6× in
notifications per second, because one coalesces and the other does not. Coalescing is the peer's
choice and the device cannot see it.

**Nor is it a byte count.** Bytes per second held steady across a 25× payload change on one peer,
which is what the earlier reading rested on, but across peers it moves 4.9× (84 vs 17.3 KiB/s). The
quantity that actually held still across both is **ATT PDUs per connection event**: 170/22.2 = 7.7
on the Linux central, 840–1,010/133 = 6.3–7.6 on the phone. That is the device controller's limit,
and it is the one number neither the spec nor the device can usefully state, since the connection
interval is negotiated by the peer and changes mid-session.

**The crash is peer-specific.** The phone never lost the link because its supervision timeout is
5,000 ms against the Linux central's 420 ms. The failure that motivated the conservative ceiling
needs a peer whose supervision timeout is tight enough that a congested link misses enough
connection events to time out.

### The chunk size, not the rate, is what limits the deployment target

On the phone each notification occupies a whole ATT PDU carrying 20 bytes of payload, out of a
negotiated 517. `NOTIFY_CHUNK` in `gatt.rs` is 20, chosen as a conservative size that works on any
peer. On a peer that coalesces this costs little, because the PDU is filled from several
notifications regardless. On a peer that does not coalesce it is the binding constraint: the link
spends a PDU to move 20 bytes.

Raising the chunk toward the negotiated MTU is therefore the larger lever on the real target, and it
is untested. Bigger PDUs occupy more air time, so fewer fit in a connection event and the gain will
be less than the 25× the payload ratio suggests. It needs measuring rather than extrapolating.

### Web Bluetooth and loss, settled

Web Bluetooth exposes no delivery or loss accounting: `characteristicvaluechanged` reports what
arrived and nothing about what did not. The page derives loss only because the harness numbers its
payloads. It read zero throughout, on the peer that queued as well as on the one that died, which
is the same answer the Linux central gave.

### Chunk size is the larger lever on the deployment target

Sequenced run against Android Chrome: four payload sizes offered just above their ceiling, then an
eight-minute soak. 364,500 notifications, **zero lost, zero gaps**, no mid-run disconnect, and the
air capture matches exactly (347,000 PDUs of 22 B, 8,000 of 102 B, 5,500 of 252 B, 4,000 of 502 B,
all single Handle Value Notification).

| payload | notifications/s | KiB/s | bytes per connection event | relative |
| --- | --- | --- | --- | --- |
| 20 B | 917 | 17.9 | 158 | 1.0x |
| 100 B | 444 | 43.4 | 343 | 2.4x |
| 250 B | 212 | 51.6 | 401 | 2.9x |
| 500 B | 118 | 57.4 | 444 | 3.2x |

`NOTIFY_CHUNK` in `gatt.rs` is 20, so on a peer that does not coalesce the link spends a whole PDU
and its per-PDU overhead to move 20 bytes. Raising it is worth **about 3.2x the throughput** on the
peer that matters, which is a larger gain than anything available from moving the rate ceiling.

The return diminishes sharply: 2.4x of the 3.2x arrives by 100 bytes, because bytes carried per
connection event saturate near 450. A chunk around 250 B captures most of the benefit without
depending on a large negotiated MTU, which matters because the chunk must also be safe on a peer
that negotiates the 23-byte minimum.

The phone soak confirmed a sustainable figure on that peer: 336,000 notifications at 700/s over
480 s, zero loss, link alive throughout. Delivered rate ran at ~900/s for the first 23 s while it
caught up on the backlog left by the 500 B phase, then settled to exactly the offered 700/s.

### At range, and on battery

Phone moved to the far side of the house, roughly 10 to 15 m through several walls, running the
same five-phase program twice: once plugged in, once on battery. Both runs delivered
**86,400 of 86,400 with zero loss, zero gaps, and no disconnect**; the only disconnect in the
capture is `Remote User Terminated Connection (0x13)`, the page closing cleanly at the end.

Battery changed the link. Plugged in, the phone requested and held a **7.5 ms** connection
interval, as it had close up. On battery it asked for **50 ms**, 6.7x longer. Supervision timeout
stayed at 5,000 ms throughout.

Air-side throughput, by payload:

| payload | close, plugged | range, plugged | range, battery |
| --- | --- | --- | --- |
| 20 B | 17.9 KiB/s | 4.2 | 3.4 |
| 100 B | 43.4 | 10.1 | 13.4 |
| 250 B | 51.6 | 12.3 | 8.1 |
| 500 B | 57.4 | 10.5 | 7.3 |

Three things hold across both range runs.

**Range costs about 4x, and battery a further 1.2x.** The whole program took 537 s plugged and
665 s on battery against 297 s of sending. Note the modest battery penalty against a 6.7x longer
connection interval: at 7.5 ms the link managed only 1.5 PDUs per connection event at range against
6.9 close up, because retransmissions consume the short event, while at 50 ms it managed 8.0. A
longer interval is more efficient per event, so it nearly compensates.

**Bigger chunks stop helping, and start hurting.** Close up, throughput rose monotonically with
payload. At range it peaks at 250 B plugged and 100 B on battery, and 500 B is worse than 250 B in
both. A larger ATT PDU spans more link-layer packets, and losing any one of them costs the whole
PDU a retransmission, so the gain reverses once the error rate is non-trivial.

**Still no loss, at four times worse conditions.** Six distinct conditions now, zero gap events in
every one. Per-second delivery does swing wildly at range, between 2 and 793, so any controller
sampling a short window would be tracking multipath rather than a trend.

The exact optimum was not measured: the sweep tested 20, 100, 250 and 500 B only. The principled
target is an ATT PDU that fits inside one link-layer packet, which with Data Length Extension is
251 bytes, so a chunk somewhere near 180 to 240 B. That is worth one more sweep before a number is
written down.

RSSI logging produced no samples, so this section has no signal-strength figures to put against the
throughput. Distance and walls are described rather than measured.
