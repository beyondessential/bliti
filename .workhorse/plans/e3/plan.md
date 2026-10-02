# A second client leaving ends the first client's session

## Cause

BlueZ (5.85, and master as of 2026-10-02), `src/gatt-database.c`.
When a client that is not bonded disconnects, `att_disconnected` runs `clear_ccc_state` for each CCC that client had enabled, which calls the characteristic's `ccc_write_cb` with no operation and so no ATT.
That reaches `queue_remove_if(chrc->notify_ios, match_client_att, NULL)`, and `match_client_att` matches anything when ATT is unset, so it frees the head of the characteristic's AcquireNotify queue: the earliest subscriber still there, not the one leaving.
The leaver's own socket then closes through `att_disconnect_cb` → `sock_hup`, which matches by socket and is correct.
The victim keeps its link and its CCC, and BlueZ only calls AcquireNotify again on a fresh CCC write, so nothing on the device side can give it a downlink back.

Ending the victim's connection is not enough: its CCC is still on, so its own disconnect frees the next head, and with three clients one leave runs through every session.

## Fix: a device transmit characteristic per client

Each characteristic has its own AcquireNotify queue.
With one subscriber per characteristic, "the head" can only be the leaver's own socket.

- The device serves eight device transmit characteristics, slot UUIDs being the device transmit base with the last byte the slot index.
- A slot allocation characteristic: a read answers one byte, the slot the reading client is to subscribe to, or an empty value where none is free.
  The slot is keyed by the client's address and held while it is connected; a client reading again gets the one it holds.
  Allocation is serialised, and before allocating it frees slots whose holder is no longer connected.
- A subscription on a slot not given to that client opens no session.
- When a session ends with its client still connected, the device ends that connection, so the client sees the channel close and the slot comes free.
  With slots that only follows a client that misbehaves (subscribes to another's slot) or the device ending the session itself.
- The CLI and the web client read the allocation, then subscribe to their slot. The web client reports a full device as busy and leaves the operator able to try again.

The client transmit characteristic stays single: BlueZ matches write sockets by socket, so writes are unaffected.

## Other clients, live

The page shows how many clients have a channel open, itself among them, out of how many the device can hold: the `channel-clients` reading over the `channel-clients-max` fact of NFO, as `2/8 clients`.
It counts channels, which the sampler already does to keep sampling open, rather than slots: a slot is given before the handshake, so a stranger in range trying one would count as an operator.
The snapshot a session is sent as it opens counts the channels then rather than at the last tick, so a page never receives a count that leaves itself out.

## Outstanding

- [ ] Find the BCM4345C0 (CYW43455, the Pi 5 controller) concurrent LE connection limit, in case it is tighter than eight slots. The datasheet lookup failed in the session that wrote this; testing needs as many centrals as slots.
- [ ] Report upstream to linux-bluetooth: `clear_ccc_state` frees one arbitrary AcquireNotify socket rather than the disconnecting device's. Draft below; the user sends it.

## Steps

- [x] CHN Transport and Several clients at once (the busy device is CHN's to require; WEB needs nothing of its own)
- [x] `bliti-core`: allocation UUID, slot count, slot UUIDs
- [x] Device: slots, allocation read, one control per slot, end connection on session end
- [x] CLI: allocate then subscribe
- [x] Web: allocate then subscribe, busy error
- [x] On-device check, both orders (earlier client leaves, later client leaves)
- [x] Bring the bliti-prototype skill's second-client note in line
- [x] `channel-clients` and `channel-clients-max`: NFO, VIEW, device, page
- [x] Run the Rust and web suites over the `2/8` change
- [x] On the prototype: the Connected tile reads `1/8` alone and `2/8` with the laptop CLI connected

## Upstream report

For the BlueZ issue form (`.github/ISSUE_TEMPLATE/issue.yml`), or a private draft advisory if it is treated as a vulnerability (`SECURITY.md`): any central in range can trigger it without pairing, and each time it stops another client's notifications.
The user files it; nothing here is sent by an agent (`doc/coding-assistants.rst`).

Still to gather, with the build before E3 deployed (`bec14b56`; a dev deploy runs from `/run`, so the kept build is untouched) and two unbonded centrals:

- [ ] A btmon trace on the peripheral of the later central leaving (`btmon -w btmon.log`): it shows B disconnecting without writing its CCC
- [ ] The bluetoothd debug log over the same run (debug on with `SIGUSR2`, then `journalctl -u bluetooth --boot 0`): `External CCC write received with value: 0x0000` at B's disconnect, and A's socket closing with B's
- [ ] The same run with the earlier central leaving, where only its own socket closes
- [ ] The phone's model and Android version, for Versions
- [ ] Decide whether to write a functional test reproducer (`test/functional`, three hosts on emulated controllers), which the AI policy prefers for a bug that is not trivial

### Description

When an unbonded LE central disconnects, bluetoothd closes the AcquireNotify socket of a different central subscribed to the same external characteristic: the earliest subscriber still in `chrc->notify_ios`, rather than the one that disconnected.

`att_disconnected()` in `src/gatt-database.c` removes the device state of a device that is not bonded and runs `clear_ccc_state()` for each CCC it had enabled.
`clear_ccc_state()` calls the CCC callback with `op == NULL`, so `ccc_write_cb()` runs `queue_remove_if(chrc->notify_ios, match_client_att, NULL)`.
`match_client_att()` matches any entry when `att` is NULL ("used by clear_cc_state to clear all instances"), and `queue_remove_if()` removes only the first match, so the head of `notify_ios` is freed.
Where the disconnecting device was not the first subscriber, that is another device's `client_io`: its socket is closed while its link and its CCC stay up, so its notifications stop, and the application cannot restore them, since bluetoothd calls AcquireNotify again only on a new CCC write.
The disconnecting device's own socket is closed separately, through `att_disconnect_cb()` and `sock_hup()`.

Expected: only the disconnecting device's notify socket is closed.

It came in with 8eb1dee87 ("gatt: Fix not establishing a socket for each device"), first released in 5.69, and the code is unchanged on master at ae69dcddd.

Any central in range can trigger it without pairing: connect, enable notifications on such a characteristic, and disconnect. Each disconnect stops the notifications of the earliest other subscriber.

Untested observation: the leaving device's own `client_io` is already closed through `att_disconnect_cb()`, so the `op == NULL` path may only need to drop the notification count rather than remove an entry.

### To reproduce

1. A peripheral runs bluetoothd 5.69 or later with an external GATT application whose characteristic notifies through AcquireNotify (`NotifyAcquired`), and centrals connect without pairing.
2. Central A connects and enables notifications; the application receives AcquireNotify for A.
3. Central B connects and enables notifications; the application receives AcquireNotify for B.
4. B disconnects.
5. The application sees A's socket close at the same moment as B's. A's link stays up and A receives no further notifications.
6. Where A disconnects instead of B, only A's socket closes.

Seen with a Rust application (bluer) on a Raspberry Pi 5, an Android phone running Chrome (Web Bluetooth) as A and a Linux laptop as B: the application logged both sockets closing within 0.1 ms of B leaving.

### Versions

- BlueZ version: 5.85 (Ubuntu 5.85-4ubuntu0.2), on the peripheral
- Kernel version: 7.0.0-1017-raspi (Ubuntu 26.04 LTS)
- Problematic device: Raspberry Pi 5 Model B Rev 1.1, BCM4345C0 controller on UART, as the peripheral. Centrals: an Android phone with Chrome (model to fill in), and an Arch Linux laptop with BlueZ 5.87, kernel 7.2.6, Intel controller

### AI use

Disclose which tool and model versions were used, and for what: Claude Code (claude-opus-4-8, then claude-opus-5-5) read the gatt-database.c code path, found the introducing commit and drafted this report. Whoever files it has to have read and verified it first.
