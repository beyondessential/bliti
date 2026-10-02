# A second client leaving ends the first client's session

On the v4 prototype, with the web client on a phone and `bliti connect` from the laptop.
The device's journal names each session's client and slot.

## Several clients at once

- [x] The two clients are given different slots (verifies spec: CHN)
- [x] The later client leaving leaves the earlier one's page live: readings keep arriving, and the device logs only the leaver's session ending (verifies spec: CHN)
- [x] The earlier client leaving leaves the later one's session running (verifies spec: CHN)
- [ ] A client that fails its handshake and leaves, while another is connected, leaves the other's session running (verifies spec: CHN)
- [x] A client reconnecting after leaving is given a slot and opens a session (verifies spec: CHN)

## Slots

- [x] Each connected client is given a slot no other holds, and reading again gives the same one (verifies spec: CHN)
- [x] A device with every slot held by a connected client turns the next one away, and a slot whose holder has gone is given again (verifies spec: CHN)
- [x] The allocation answers one byte for a slot and an empty value for none, and a client refuses a slot the device does not offer (verifies spec: CHN)
- [ ] A subscription to a slot the client was not given opens no session (verifies spec: CHN)
- [ ] The CLI and the web client tell the operator the device is serving as many clients as it can when no slot is free (verifies spec: CHN)

## When a channel ends

- [ ] A session the device ends while its client is connected, such as on a failed handshake, ends that client's connection, and the page reports the channel closed (verifies spec: CHN)

## Other clients

- [x] The snapshot a session is sent as it opens counts that session, and every tick carries the count of open channels (verifies spec: NFO)
- [x] The page shows the count less itself (verifies spec: VIEW)
- [ ] On the prototype, the page's Also connected tile goes from none to one as the laptop CLI opens a channel, and back as it leaves (verifies spec: NFO, VIEW)
- [ ] A client in range that fails its handshake does not show in the count (verifies spec: NFO)
