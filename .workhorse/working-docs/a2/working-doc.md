---
status: draft
---

# Gate sensitive actions behind authentication mechanisms

Protect control actions and features behind one or more stackable gates, such as physical presence, a static passphrase, or an offline-verifiable signed challenge issued by BES.

## Threats

The presence token is the only credential today, and a photograph of the QR code yields it (SEC).
Anyone who can open a session can power a device off or re-network it.

The gate is aimed at two attackers:

- **Photo, used later.** Someone captures the QR code and acts from BLE range at another time, out of sight.
- **Overreaching staff.** People with a legitimate reason to be at the device and read its status, who should not be able to control it.

Someone at the device with a phone can usually pull its plug too, so defending against presence in the moment is not the aim.
Stealthy network changes are the most damaging case: quieter than unplugging, and durable once confirmed.

A required use case: someone on the ground, possibly there for a single day, is given access, uses it, and leaves.
They must not still hold access months later without BES knowing.

## Behaviour

- Everything on the Control screen is gated: the power acts (restart, reboot, power off) and network configuration.
  Reading status is not.
- Control needs the presence token **and** a ticket issued by BES. Neither alone is enough.
- The ticket is a bearer ticket: whoever holds it can use it.
- The ticket covers every device. Time is its only scope; the presence token still limits which devices someone can reach.
- The ticket lapses on its own, on a scale of a day to a week. Expiry is the revocation mechanism; there is no revocation list.
- The device refuses a ticket whose lifetime, from issue to expiry, is longer than a week.
- The ticket works with neither end online. The visitor's phone often has no signal at the site, so the device's own sense of time is the main defence against a ticket used after it expires.
- The ticket names who it was issued to, so the device's log can say who held it.
- The device logs, for every act asked for, who held the ticket, alongside the client `name` and `version` it already logs (CTL).
- The client presents the ticket once per channel. The control stream and the configuration session both honour a channel that holds a valid ticket.
- The web app keeps a ticket until it expires, across closing the tab, so a day visitor opens the link once.
- Without a ticket, the Control screen is still offered, showing what exists, with its controls locked and saying a ticket is needed.

## Implementation options

### Ticket shape

BES signs a ticket carrying who it was issued to, its issue time and its expiry.
The device ships with the BES public key in its image and checks the signature offline.
With bearer, fleet-wide scope, a ticket is in effect a fleet secret with a timer on it: its lifetime is the whole exposure if it leaks, and a leak still needs a device's presence token to be of any use.

- **Maximum lifetime enforced by the device.** The device refuses a ticket whose expiry is further from its issue time than some maximum, so a ticket issued for a year by mistake is refused rather than honoured.
- **Delivery.** A link with the ticket in the fragment, as the presence token travels, keeps it out of any server's request log. Chat or email carrying the link is the main leak surface.

### Where the ticket is presented

Chosen: **once per channel**. The client sends the ticket on its own stream after the handshake, and the device records that the channel holds it.

- Rejected: on each gated stream, riding on the first message of `control` and `configure`. Stateless, but sends it twice.
- Rejected: in the handshake payload. Mixes authorisation into the channel layer, which is otherwise the same whether or not anything is gated.

A ticket that expires while its channel is open: undecided whether the channel loses control at that moment or keeps it until it closes.

### Device time

The device needs to know the date to enforce expiry, and a Pi has no battery-backed clock unless one is fitted.

- **Lower bound that only moves forward.** Starts no earlier than the build date of the device's image. Persisted across reboots, advanced by uptime, raised to the issue time of every valid ticket seen and to any authenticated time received. A ticket that has expired is accepted only if the device has not been running since the ticket expired, and has received no authenticated time since.
- **Authenticated time from the client.** When the phone is online, it fetches a signed timestamp for a nonce the device supplies (Roughtime is built for this), which raises the lower bound. Opportunistic; the lower bound covers the offline case.
- **Online challenge.** The device issues a nonce and BES signs it at use time, relayed by the phone. Removes the clock problem entirely, but needs phone connectivity at the site, which is often absent. Not worth building first.

Two consequences of the lower bound:

- **A shelved device is exposed until it hears the time.** A spare that last ran at time S accepts every ticket expiring after S when first switched on. The exposure ends at the first ticket used on it (its issue time raises the bound) or the first authenticated time it receives.
- **Only authenticated time may raise the bound.** The bound never moves back, so an unauthenticated source (plain NTP on a hostile network) that pushed it far forward would expire every ticket and lock control out for good. Authenticated sources are tickets, a signed timestamp from a client, and time over an authenticated connection such as NTS.

### Issuance

Who at BES signs, with what tooling, and where the signing key lives are all undecided.
Compromising the signing key grants control of every device only to someone who also has a device's presence token.

## Trade-offs

- **Physical presence (button, or a window after power-on).** Ruled out. The devices have no button, and staff are physically present, so this does nothing against the overreaching-staff threat. A power-on window also makes the operator cause the disruption the gate exists to prevent.
- **Static global passphrase.** Ruled out. It is a fleet-wide secret, which breaks SEC's "compromising one device tells nothing about another". Staff learn it, staff leave, and rotating it needs a fleet update.
- **Second token derived from the root, printed on a site-held sheet.** Ruled out. It derives from the board, so it can never change, and anyone who once saw it keeps access. Letting it change needs device state that the board cannot rebuild.
- **Operator-set PIN on first use.** Ruled out. The site becomes the authority and the day visitor learns the PIN and keeps it. It also adds device state that cannot be rebuilt from the board.

## Open questions

- [x] Is not implementing a gate still on the table? No: a day visitor being given access and losing it on their own is a use case to enable.
- [x] Bearer ticket or key-bound certificate? Bearer.
- [x] Scope? Every device; time is the only scope.
- [x] What maximum lifetime the device enforces? One week.
- [x] Where the ticket is presented? Once per channel.
- [ ] Whether a channel loses control when its ticket expires mid-channel.
- [ ] Issuance: who at BES signs, how a visitor asks for access, and where the signing key lives.
- [ ] Which authenticated time sources raise the device's lower bound.
- [x] How long the web app keeps a ticket? Until it expires, across closing the tab.
- [x] What an operator without a ticket sees? The Control screen, shown with its controls locked.
- [ ] How the ticket link is shaped, and whether the app accepts one pasted or scanned as well as followed.
- [ ] Does a ticket carried on a C1 volume also authorise ordinary channels while the volume is inserted, or only channels opened through it?
- [ ] Does sharing device time machinery with V1 change what V1 decides about letting a client correct the clock?
- [ ] Does a device ship with the BES public key in its image, and how is that key rotated?

## Related cards

- **A1, advertise only in a window after power-on.** Gates *discovery* behind a power cycle, which blunts the photo-used-later threat: a device heard only after someone stood at it and power-cycled it. It does nothing against overreaching staff, who can power-cycle. A1 and A2 divide the work: A1 limits who can reach a device, A2 limits who can change it. Neither makes the other redundant.
- **B1, wake a quiet device with a targeted beacon.** Reopens A1's window from the presence token alone, which hands the photo-used-later attacker their reach back. That is acceptable only because A2 still gates control. If A2 were dropped, B1 would reopen the hole A1 closes.
- **C1, debug mode from a removable volume.** C1 is a way to *reach* a device without its sticker, and its purpose includes provisioning: the full surface, not a restricted one. A2 keeps the two questions apart. A channel opened through C1 is reached with the volume's value in place of the presence token, and is authorised for control by a ticket exactly as any other channel is. So C1 can offer everything, and it doesn't become a way round the gate. The volume can carry the ticket alongside its value, so a factory jig or a tool-less technician doesn't have to present one separately; the device then treats channels opened through the volume as holding it. That stacks physical presence on the BES ticket, which is what the card description had in mind. A volume left in a device leaks its ticket to whoever pulls it out, for up to a week, which is one more reason for C1's idea of taking the device out of service while in debug mode.
- **V1, detect a drifted device clock.** V1 notes that nothing on the device currently expires, so a wrong clock costs little. A2 changes that: the device's time becomes load-bearing for control. A2 also brings V1 an authority signal it found missing: a BES-signed ticket is authenticated evidence the time is at least its issue time. V1's "earlier than the build date is wrong" check is the same floor A2's lower bound starts from. The two should share one notion of device time. The wall clock V1 reports and corrects stays separate from the lower bound A2 enforces, so a client correcting the wall clock never moves the lower bound.

## Testing notes
