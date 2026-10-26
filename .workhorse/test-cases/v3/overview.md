# Status screen

Scenarios for the Status screen landing and the `end-of-batch` marker it reads its rows by.

## Device: batches

- [x] The feed opens on one batch of every fact then the current readings, ended by `end-of-batch` (verifies spec: NFO)
- [x] Each sampler tick is a batch of readings of its own (verifies spec: NFO)
- [x] A change of facts is one batch: each fact it ends, then the facts now (verifies spec: NFO)
- [x] `end-of-batch` round trips with nothing beyond `type` (verifies spec: NFO)
- [x] A refusal names the message type it refused, for a critical type and for a critical member (verifies spec: STAT)

## The landing

- [x] A channel opens on the Status screen, titled with the hostname, the software identity beneath and Disconnect beside it (verifies spec: STAT)
- [x] With no hostname, the title is the code's last four characters (verifies spec: STAT)
- [x] Every screen reached from it returns to it, by its own Back and by the browser's (verifies spec: STAT, WEB)
- [x] All readings, Network settings and Power and battery each carry Back alone beside the title (verifies spec: VIEW, NSCR, CSCR)
- [x] A refusal is said once per message type while the channel is open, and again on the next channel (verifies spec: STAT)

## The rows

- [x] Every row is checking until the first batch has ended, with nothing applied before it ends (verifies spec: STAT)
- [x] Tailscale is hidden once the first batch has ended where the device reports no `tailscale-state` (verifies spec: STAT)
- [x] An address moving interfaces in one batch never shows the network as not connected (verifies spec: STAT, NFO)
- [x] A resumed feed's first batch drops what ended while the feed was declined (verifies spec: NFO)
- [x] Network: cable, wifi with the network joined, hotspot with and without a client count, and cable or wifi then hotspot (verifies spec: STAT)
- [x] Network: the way on the default route is named, falling back to cable then wifi by interface name (verifies spec: STAT)
- [x] Network: a link-local address, or one on another interface, counts as none (verifies spec: STAT)
- [x] Network: not connected says a cable with no address, and a network joined with no address (verifies spec: STAT)
- [x] Network: Set up while not connected and Change while connected both open Network settings (verifies spec: STAT)
- [x] Network: Network settings opens read-only where someone else holds the session (verifies spec: STAT, NSCR)
- [x] Tailscale: every state in the table, with the reason only where it calls for it (verifies spec: STAT)
- [x] Tailscale: the device name where `tailscale-name` has a value, and a lapsing sign-in lifting well to warning with when (verifies spec: STAT)
- [x] Tamanu: checking with no `tamanu-state`, every state in the table, and each version only where it has a value (verifies spec: STAT)
- [x] An unrecognised state is marked by its status and says its value with the reason (verifies spec: STAT)

## Advanced and acts

- [x] Advanced is closed when a channel opens and leads to All readings, Network settings and Power and battery (verifies spec: STAT)
- [x] Advanced stays as left across screens and closes on a new channel (verifies spec: STAT)
- [x] A power act shows the Status screen's title and identity with the act in place of the rows and Advanced (verifies spec: STAT, WEB)

## On a real device

- [ ] The screen against the prototype device: rows settle on the first batch, and the network row follows a cable pulled and plugged back in
- [ ] A reboot from Power and battery comes back to the Status screen with its rows checking, then settled
