# Tailscale status and control

Scenarios that show the device reports Tailscale and carries out its acts. Automated cases run against a fake tailscaled and recorded output; the by-hand cases need a device on a real tailnet.

## Facts

- [x] Each of the six states reports its name and status, with a reason where it is not `passed` (verifies spec: NFO)
- [x] A connection problem while `running` is a warning carrying Tailscale's own words (verifies spec: NFO)
- [x] A lapsed or stopped device keeps its name and tailnet `passed`; a device with no sign-in skips them (verifies spec: NFO)
- [x] Key expiry is `passed` beyond seven days, `warning` within them, `failed` once lapsed, `skipped` where it does not lapse or none is held (verifies spec: NFO)
- [x] A daemon that does not answer breaks all four with one reason; a device without Tailscale reports none (verifies spec: NFO)
- [x] A change of state reaches the feed as a change in the facts
- [x] Each recorded `status` body maps to the state it was recorded in, `expired` from `Self.Expired`, `needs-login` from no node key

## The acts listed

- [x] The acts listed for each state match the table (verifies spec: TSC)
- [x] A broken daemon, and a device without Tailscale, list only `netcheck`, and none where the command is missing (verifies spec: TSC)
- [x] The list is pushed on every open stream when the state changes it (verifies spec: TSC)

## Carrying acts out

- [x] An act not listed is refused at `$['act']` (verifies spec: TSC)
- [x] A sign-in without a method, or with an unknown one, is refused at `$['method']`; by auth key without the key at `$['key']`, taking no slot (verifies spec: TSC)
- [x] An act asked for while another is under way, on another stream, is refused at `$` naming the act under way (verifies spec: TSC)
- [x] `turn-off` and `turn-on` are accepted and the state follows
- [x] A transition that never arrives fails with `tailscale-failed` on its deadline and frees the slot
- [x] A call tailscaled refuses fails with tailscaled's words
- [x] A stream ending mid-`turn-on` leaves it to finish (verifies spec: TSC)

## Signing in

- [x] An auth key signs in, registers the name asked for, and the device reaches `running`
- [x] A rejected key fails with tailscaled's reason, with the key scrubbed from it
- [x] A login link is forwarded as soon as Tailscale issues it, and approving it signs the device in
- [x] A login link nobody opens fails on the deadline
- [x] A stream ending mid-login-link leaves Tailscale as it stood and frees the slot (verifies spec: TSC)
- [x] A device restricted to one tailnet signs out of another a sign-in joins and fails with a reason naming its own
- [x] An unrestricted device accepts any tailnet
- [x] The key never appears in a log line, a `Debug` rendering of a message, or a reason (verifies spec: TSC)

## Netcheck and ping

- [x] `netcheck` runs while signed out and its relays are named from the cached map, ordered nearest first
- [x] HTTPS-only relays are a result with `udp` false; no relay answering is a result with an empty `relays` and `udp` false
- [x] An unknown IPv6 or NAT is reported as the test found it, `null` where it could not tell
- [x] A command that cannot start is `tailscale-failed` with its own reason
- [x] A stream ending mid-`netcheck` ends it and frees the slot (verifies spec: TSC)
- [x] The relay name cache survives a restart, tolerates a corrupt file, and gives a null name for an id it does not hold
- [x] `ping` of a direct path stops at the first direct probe; relayed then direct reports `direct`; relayed only names the relay
- [x] A machine that exists and does not answer is a result with `reachable` false
- [x] An unknown peer, or an address given as the peer, is `tailscale-failed`; the default peer is used where none is named; none configured fails

## Wire

- [x] Every Tailscale message round trips, with the optional members of `tailscale-act` present and absent
- [x] `tailscale-act` writes every member it carries in upper case, and an unknown critical member makes it unprocessed
- [x] The corpus line for a key sign-in reads back as sent

## By hand, on a device

- [ ] Sign in by auth key and by login link against Tailscale's own control plane
- [ ] A device awaiting approval reports `needs-approval`; a lapsed key reports `expired`; a rejected key's reason is readable
- [ ] Turn off, turn on and log out each take effect and the Status screen follows
- [ ] `netcheck` while signed out and while signed in, on a network that blocks UDP
- [ ] Ping the default peer and a named peer
- [ ] Start the daemon with `--tailnet` set to another tailnet and see it refuse; start it with Tailscale not installed and see it run
