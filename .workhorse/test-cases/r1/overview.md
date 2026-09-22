# Send rate ceiling

Scenarios verifying that a device holds to the send rate of [CHN](../../specs/channel.md), now a
payload-byte budget rather than a count of notifications.

The measurements behind the figure are in the card's plan. They are not reproducible in CI: they need
the prototype device, a second machine, and a phone, so the cases below cover the pacing logic, and
the link behaviour itself stays a manual check.

## Pacing

- [x] A full second's allowance of small payloads takes about a second to go out (verifies spec: CHN)
- [x] No one-second window anywhere in a sustained run exceeds the ceiling, including across window
      boundaries, which is what a counter cleared each second would allow (verifies spec: CHN)
- [x] A quiet stretch is not banked: the first payload after a lull goes straight out, and the one
      after it is still paced (verifies spec: CHN)
- [x] The same number of bytes takes the same time whatever the payload size, so the ceiling does not
      move with the chunk (verifies spec: CHN)
- [ ] Payload held back by the ceiling is delivered late rather than dropped (verifies spec: CHN)

## On a real link

- [ ] A device sending continuously at the ceiling holds a session for ten minutes against a
      BlueZ-backed central, with no loss and no disconnect
- [ ] A device sending continuously at the ceiling holds a session against Android Chrome over Web
      Bluetooth, at close range and at the far end of a building
- [ ] A client slower than the ceiling paces the device rather than being overrun: everything
      arrives, late, and the link survives
- [ ] A session recovers after the client walks out of range and returns, with the ceiling still
      enforced on the new session
