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
- [ ] On the prototype: the Connected tile reads `1/8` alone and `2/8` with the laptop CLI connected

## Upstream report draft

> Subject: gatt-database: disconnect of an unbonded device frees another device's AcquireNotify socket
>
> When an unbonded device disconnects, att_disconnected() calls clear_ccc_state(), which invokes the external characteristic's ccc_write_cb() with op == NULL.
> ccc_write_cb() then does queue_remove_if(chrc->notify_ios, match_client_att, NULL), and match_client_att() returns true for any entry when att is NULL, so the first client_io in notify_ios is freed: the earliest subscriber's socket, not the disconnecting device's.
> With two unbonded centrals subscribed through AcquireNotify, the later one disconnecting closes the earlier one's notify socket while its link and CCC stay up; BlueZ does not call AcquireNotify again until that central rewrites the CCC.
> Seen on 5.85, and the code is unchanged on master.
> Passing the disconnecting device's bt_att through to the CCC callback (or matching notify_ios entries by that device) would remove only its own entry.
