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
