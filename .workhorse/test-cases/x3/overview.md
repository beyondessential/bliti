# Tailscale and Tamanu acts

Scenarios for the acts on the Status screen's Tailscale and Tamanu rows, the Tailscale screen, the sign-in screen, and the netcheck report the device carries for them.

## Device: netcheck report

- [x] A netcheck result carries `report` as the text `tailscale netcheck --format=json` printed, byte for byte (verifies spec: TSC)
- [x] `report` is present in every netcheck result, with relays and without (verifies spec: TSC)
- [x] The typed members are unchanged by `report` being added (verifies spec: TSC)

## Wasm bridge

- [ ] A Tailscale stream and a Tamanu stream open with `tailscale` and `tamanu` as their first message (verifies spec: TSC, TMU)
- [x] A sign-in by key carries `ACT`, `METHOD`, `KEY` critical, and `NAME` only where given (verifies spec: TSC)
- [x] A ping carries `PEER` only where given (verifies spec: TSC)
- [x] The login link renders as a QR code that decodes back to the link
- [x] The activity record never shows an auth key or a login link

## Status screen: streams

- [x] Showing the Status screen opens a Tailscale stream and a Tamanu stream; leaving it for another screen closes both (verifies spec: STAT)
- [x] A device that never answers `tailscale` or `tamanu` leaves both rows without an act, and the rows still render from the feed (verifies spec: STAT)
- [x] A row offers no act the device has not listed, even where the state's table names one (verifies spec: STAT)
- [x] A new `tailscale-acts` or `tamanu-acts` changes the row's act without the operator doing anything (verifies spec: STAT)

## Status screen: Tamanu row

- [x] Stopped with `start` listed: Start; tapping it shows "Start Tamanu?" in place of the button, the other rows still in view (verifies spec: STAT)
- [x] Cancel on the confirmation puts the button back and sends nothing (verifies spec: STAT)
- [x] Confirming sends `tamanu-act` with `start`, and the button is not offered again until the device answers or lists its acts anew (verifies spec: STAT)
- [x] Running or degraded with `restart` listed: Restart, confirmed with "Restart Tamanu?" and that Tamanu may be unavailable while it restarts (verifies spec: STAT)
- [x] `restarting` and `starting` on the feed show the row in progress with no act (verifies spec: STAT)
- [x] A `cannot-act` shows its reason in the row's detail as the device wrote it, until the operator asks again or leaves the screen (verifies spec: STAT)
- [x] A `tamanu-failed` shows its reason the same way (verifies spec: STAT)
- [x] Absent, broken or no state: no act (verifies spec: STAT)

## Status screen: Tailscale row

- [x] Not signed in and sign-in expired: Connect, which opens Connect to Tailscale (verifies spec: STAT, TSCR)
- [x] Turned off: Turn on, sent without a confirmation (verifies spec: STAT)
- [x] Connected and state unknown: Network check (verifies spec: STAT)
- [x] Connecting: no act, though the device lists `turn-off`, `log-out` and `netcheck` (verifies spec: STAT)
- [x] Waiting for approval: Cancel; tapping it asks "Cancel the sign-in?" with Keep waiting as the filled button, and confirming sends `log-out` (verifies spec: STAT)
- [x] Network check under way shows "Checking…" in place of the button (verifies spec: STAT)
- [x] A result with any relay says the device can connect, whether or not `udp` is true (verifies spec: STAT)
- [x] A result with no relay says blocked: that the network asks for a sign-in where `captive-portal` is true, no internet where `ipv4` and `ipv6` are both false, and that Tailscale cannot be reached otherwise (verifies spec: STAT)
- [x] Details beside the verdict opens the Tailscale screen showing that same result (verifies spec: STAT, TSCR)
- [x] The verdict stays until the check is run again or the operator leaves the Status screen (verifies spec: STAT)
- [x] A `tailscale-failed` for a network check shows its reason in the row's detail (verifies spec: STAT)

## Advanced

- [x] Advanced holds All readings, Network settings, Tailscale, Power and battery, in that order (verifies spec: STAT)
- [x] Tailscale is not under Advanced where the device reports no `tailscale-state` (verifies spec: STAT)

## Tailscale screen

- [x] Titled Tailscale with Back beside it, which returns to the Status screen, as does the browser's back (verifies spec: TSCR, WEB)
- [x] Opening it opens a Tailscale stream, and leaving closes it (verifies spec: TSCR)
- [x] Details show the state with the row's mark, words and reason, then device name, tailnet and sign-in lapse as a date and from now, each only where it has a value (verifies spec: TSCR)
- [x] Acts appear in the order Connect, Turn on, Turn off, Log out, only as listed, and the section is absent until listed or where none is (verifies spec: TSCR)
- [x] Turn off asks to confirm and says the fleet cannot reach the device until it is turned on again (verifies spec: TSCR)
- [x] Log out asks to confirm and says the fleet cannot reach the device until it is signed in again (verifies spec: TSCR)
- [x] Connect opens Connect to Tailscale, and its Back returns to the Tailscale screen (verifies spec: TSCR, WEB)
- [x] The network check shows the verdict, UDP, IPv4, IPv6, address varies and asks for sign-in where the result says, and relays nearest first by name, or number where unnamed, with latency (verifies spec: TSCR)
- [x] A check run from the Status row shows here without running it again (verifies spec: TSCR)
- [x] Share passes the hostname, when the check ran, the result as JSON without `report`, then `report` exactly as received (verifies spec: TSCR)
- [x] Without a share sheet, Share copies that text to the clipboard (verifies spec: TSCR)
- [x] Ping is offered only while `ping` is listed; an empty machine field sends no `PEER` (verifies spec: TSCR)
- [x] A ping result shows the machine tested, whether it answered, direct or through a named relay, and the round trip (verifies spec: TSCR)
- [x] A `cannot-act` or `tailscale-failed` on this screen shows its reason as the device wrote it (verifies spec: TSCR)

## Connect to Tailscale

- [x] Opens on the auth key form, with a link to use a login link instead and back (verifies spec: TSCR)
- [x] The key field hides what is typed until Show is tapped (verifies spec: TSCR)
- [x] The device name field starts as the hostname; unchanged, the sign-in sends no `NAME`; changed, it sends the new name (verifies spec: TSCR)
- [x] Connect sends a sign-in by `auth-key` with the key typed (verifies spec: TSCR)
- [x] A rejected key shows the device's reason and leaves the form to try again (verifies spec: TSCR)
- [x] Asking for a login link shows that it is waiting until the first `tailscale-login-link` arrives (verifies spec: TSCR)
- [x] The link shows as a QR code with Open, Copy and Share, and the line saying it connects once approved (verifies spec: TSCR)
- [x] A second `tailscale-login-link` replaces the first everywhere: QR, Open, Copy and Share (verifies spec: TSC, TSCR)
- [x] A `tailscale-failed` for a link shows the reason and offers Get a new link, which sends a fresh sign-in (verifies spec: TSCR)
- [x] The stream closing while a link shows drops the link and offers a new one (verifies spec: TSCR)
- [x] Reaching `running` or `needs-approval` returns to the screen it was opened from, Status or Tailscale, and the browser's back does not land on it again (verifies spec: TSCR, WEB)
- [x] Leaving and returning starts with an empty key field (verifies spec: TSCR)

## On a device

- [ ] Sign in a real device with a tagged auth key typed in; the Status row goes connecting then connected
- [x] Scan an auth key from a QR code on another screen; the field fills with it (verifies spec: TSCR)
- [ ] Sign in by login link, approving from a second device by its QR code
- [ ] Sign in by login link opened on the same phone; the page comes back to the sign-in screen, and either finishes or offers a new link
- [ ] Share the login link to a chat app through the phone's share sheet
- [ ] Turn Tailscale off and on again from the Tailscale screen, watching the Status row follow
- [ ] Restart Tamanu on a real device, watching the row show restarting then running
- [ ] Run a network check on a network with UDP blocked, and one behind a captive portal; share the result and read `report` in the chat
