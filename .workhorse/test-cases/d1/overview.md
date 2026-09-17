# Diagnostic and informational data display: test cases

Coverage this card owes. The envelope cases are E1's and are not repeated here; these are the readings, the buffer, and the view.

Three levels own different things, as E1 established: Rust owns the device and the wire shapes, Playwright owns the view fed decoded messages, and the Bluetooth path is manual against real hardware. Each case names its level.

## The reading format (verifies spec: BLI-SYS)

An independently written client should pass the client-side cases from the spec alone.

- [ ] Every value kind round trips: `fraction`, `quantity` with and without `max`, `duration`, `text` (Rust).
- [ ] Absent optional members are omitted on the wire rather than sent as null (Rust).
- [ ] A value of an unrecognised `kind` leaves the reading readable as a label, and does not fail the message (Rust, Playwright).
- [ ] A reading carrying `error` carries no `value` and has `state` of `fault` (Rust).
- [ ] A reading carrying neither `value` nor `error` is rejected (Rust).
- [ ] `at` is monotonic across samples and independent of wall time; moving the device clock does not disturb a graph (Rust).

## What the device reports (verifies spec: BLI-SYS)

- [ ] Two mount points on one block device produce one `disk` reading, not two (Rust).
- [ ] Hardware that is not fitted omits its reading entirely (Rust).
- [ ] Hardware that is fitted but unreadable produces a reading with `error` and a reason (Rust).
- [ ] Those two are distinguishable: a test that would pass if the code conflated them fails (Rust).
- [ ] Loopback is never reported; other virtual interfaces are never reported; the overlay interface is (Rust).
- [ ] `cpu` is computed from deltas, so a first sample does not report a figure derived from time since boot (Rust).
- [ ] Network counter wrap does not produce a negative or absurd rate (Rust).
- [ ] `temperature` carries the board's declared trip points in `limits` (Rust).
- [ ] `temperature` carries its note, and the note names the processor core and gives the ordinary range (Rust).
- [ ] A device running warm but healthy reports `state` of `ok`, not `warn` (Rust). This is the regression guard for the behaviour that prompted the note.
- [ ] `throttling` reports undervoltage and frequency capping independently, and reports neither when both are clear (Rust).
- [ ] `power-source` reports external power when the power-loss line is high (Rust).
- [ ] The line low with the charge falling reports on battery (Rust).
- [ ] The line low with the charge steady reports the backup supply bypassed, as `warn`, with a note saying a power cut will stop the device (Rust).
- [ ] Before there is enough history to tell steady from falling, on battery is reported rather than a bypass (Rust). Asserting a bypass early would send someone to move a plug that is already correct.
- [ ] The GPIO chip is resolved by line name, so the reading works on a kernel that numbers the chips differently (Rust).
- [ ] Battery direction comes from `power-source` where that reading exists, and from history where it does not; the `note` says which (Rust).
- [ ] A derived direction is withheld until there is enough history to be steady (Rust).
- [ ] External power reported present while charge falls steadily sets the battery reading to `warn` with a note (Rust). This is the poor-pogo-pin case, which is a documented failure on this board.
- [ ] Every reading degrades to something on a machine that is not a Pi (Rust, plus a manual run on the development laptop).
- [ ] A machine with no UPS fitted reports neither a battery nor a `power-source` reading (Rust, plus a manual run on the development laptop). The pull-up on GPIO 6 makes an unconnected pin read as external power present, so this is the guard against reporting a UPS that is not there.
- [ ] The gauge reading works unchanged on both board models, since both carry the same part at the same address (Rust; manual on each of v3 and v4).

## Sampling and the buffer (verifies spec: BLI-SYS)

- [ ] The window holds about five minutes and evicts oldest first (Rust).
- [ ] Nothing is coarsened: a sample in the window has the resolution it was taken at (Rust).
- [ ] Sampling starts at daemon start, and again on session open (Rust).
- [ ] Sampling stops after thirty minutes with no session open (Rust).
- [ ] A session opening after that restarts it (Rust).
- [ ] `system-history` on subscribing carries the buffer, and an empty buffer is a valid history rather than an error (Rust).
- [ ] Fast and slow readings arrive at different rates, and a sample carrying only some readings is valid (Rust).

## The view (verifies spec: BLI-SYS)

- [ ] A reading the client has never heard of renders from `label`, `value`, `detail`, `note`, `state` and `limits` alone (Playwright).
- [ ] A recognised reading gets its bespoke treatment, and the same reading with recognition removed still renders (Playwright). Both halves matter: the second is what keeps generic rendering the floor.
- [ ] The face carries the label and the headline value and nothing else (Playwright).
- [ ] `detail`, `note`, `limits`, bars and graphs appear only after the tap (Playwright).
- [ ] `warn` and `fault` colour the face and add no element to it (Playwright).
- [ ] Readings sharing a `group` render as one tile; with grouping ignored they render as several and are still correct (Playwright).
- [ ] A bar is drawn for a `fraction` and for a `quantity` with `max`, and not for a `quantity` without one (Playwright).
- [ ] A reading with `error` is shown as failing with its reason, and is distinguishable from a reading that never arrived (Playwright).
- [ ] The client never reports a reading as missing (Playwright).

## Graphs (verifies spec: BLI-SYS)

- [ ] History from `system-history` is drawn immediately, so a graph is populated on first appearance rather than filling from empty (Playwright).
- [ ] Samples are spaced by `at`, not evenly: an irregular gap shows as a gap (Playwright).
- [ ] Two grouped readings with opposed `direction` draw as one mirrored graph (Playwright).
- [ ] Each direction is scaled to its own peak and each peak is stated (Playwright).
- [ ] The quieter direction stays readable when the other is an order of magnitude larger (Playwright). This is the case the scaling decision exists for.
- [ ] Hiding the page drops the subscription; showing it resubscribes, and history covers the gap so the graph is continuous (Playwright).

## Against real hardware

Manual, or agentic over ssh to `tamanu-iti-v4-prototype`. Web Bluetooth cannot run in Chrome on the development laptop, so these go through a phone.

The test device is a v4. Anything board-specific needs running again on a v3, whose power-loss pin is not documented and has to be established on the hardware.

- [ ] Every reading shows a plausible value on the test device, checked against the same figure read over ssh.
- [ ] The fuel gauge reading matches what the gauge reports directly.
- [ ] With the supply in the UPS's own input, pulling it keeps the device running on battery, `power-source` flips to battery, and replugging flips it back.
- [ ] With the supply in the Pi's own socket instead, `power-source` reports the bypass and warns.
- [ ] Battery direction follows `power-source` rather than waiting for history to establish it.
- [ ] The device survives a power cut in the supported configuration and does not in the bypassed one. This is the behaviour the warning exists for, and confirming it is what makes the warning honest.
- [ ] Loading the device shows `cpu` moving and the graph following.
- [ ] Pulling the network cable shows the interface reading change and throughput fall to nothing.
- [ ] The view is legible at arm's length on a phone, held at the distance an operator actually stands from the device.
