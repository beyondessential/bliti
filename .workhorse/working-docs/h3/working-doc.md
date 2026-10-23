---
status: draft
---

# Implement UI from mockup F3

Working out how to build the UI from card F3's mockup into the browser client.

Reference mockup: [Master redesign](../../design/mockups/h3/master-redesign.html), copied from F3 (`workhorse/f3` at 206532aa).

## F3's spec changes, reviewed

F3 (still specifying) changes nine specs on its branch: new Status screen (STAT), Network setup (NSET), Joining wifi (WIFI), Sharing the hotspot (SHARE) and Service control (SVC); a `service` fact in NFO; retitles and re-parents CSCR, NSCR and VIEW under Advanced; WEB's act-under-way follows STAT.
Reviewed against main at the same base, which hasn't moved since F3 branched.

### Scope: this isn't UI-only

The Status screen's Tailscale and Tamanu rows need device work that doesn't exist yet: the `service` facts (today the device only knows the overlay's interface name), finding Tamanu's two versions, and the whole SVC stream with its restarts.
That's a protocol addition plus sampler work on `bliti`, beside the client work.
It goes to a follow-up card (see Follow-ups); this card builds the rows' UI with every service shown as work in progress.

### Gaps and contradictions to fix

- **No carrier fact.** NSET shows "whether a wired interface has carrier" and the cable screen waits on carrier, but NFO reports no carrier. CFG's `state` only covers enabled candidates, and on wifi the wired attachments are off, so carrier can't be observed from wifi, which is exactly where the cable flow starts. Settled: NFO gains a carrier fact per wired interface, reported whether or not its attachment is enabled.
- **WIFI's ordering rule contradicts itself.** It says to leave the ordering as it is, and also to send the joined network to the top, without saying whether joining replaces the one candidate it presents or adds one. Settled: joining replaces that candidate in place, keeping the ordering; the old network's credentials are not kept, which keeps the configuration within the simple states.
- **When the simple screens hold a session is unsaid.** Settled below under Sessions.
- **Service findings, for the follow-up card:** a restart accepted then failed reads "restarting" forever; NFO's "leaving and returning" reads like `ended` where SVC means `failed` then `passed`; the overlay row can't show the device name on a `broken` entry; restarting Tamanu's two services can half-succeed.
- **Stale reference.** WEB:105 still returns to "the device view" after a reconnect; it should be the Status screen.
- **NFO wording.** "MUST NOT rely on the name's meaning being understood to report the service" doesn't parse.

### Decisions to confirm

- [x] Device-side service work goes to a follow-up card.
- [x] Cable screen: a new carrier fact tells it a cable is in.
- [x] Another client holding the session is found out on tap (see Sessions).

## Behaviour

### Simple and advanced network screens

The simple screens (Network, Cable, Join wifi, Share hotspot) are an easy front end over the same configuration.
The full network settings editor (NSCR) stays, reached under Advanced, and can express anything the device supports.
Restrictions below are the simple screens' only, never the device's or the protocol's.

The simple screens allow exactly these states:

| state | allowed |
| --- | --- |
| cable alone | yes |
| wifi alone | yes |
| hotspot alone | yes |
| cable and hotspot | yes |
| cable and wifi | no |
| wifi and hotspot | no |

Read as a model: the uplink is either wifi or cable, and the hotspot is an add-on available only off wifi.
So joining wifi turns the cable and the hotspot off; turning the hotspot on from wifi turns wifi off and the cable back on; using the cable turns wifi off and leaves the hotspot as it is.
"Hotspot alone" is cable-and-hotspot with no cable plugged in.

Where the configuration is outside those states (set in Advanced: wifi and cable both on, several wifi networks), the Network screen says it is set up under Advanced and offers only the way there, so the simple screens never overwrite a technician's setup.

This replaces F3's NSET/SHARE rule that the hotspot never runs beside a cable: sharing a cabled uplink is the main point of the hotspot.
The mockup's Status screen also shows the hotspot and cable as exclusive, which is wrong in the same way.

### One-step apply

Apply saves immediately, in the simple screens and the full editor alike.
There is no applied-but-unconfirmed state and no Confirm step: the operator edits, applies, and the device saves and reports whether it works. Errors come back and the operator iterates.
Editing then applying stays (no autosave), so a half-typed gateway is never proposed.
A configuration that fails is still saved: the device keeps what was applied and reports the errors, and stays on it until someone changes it.

This reaches past the UI: CFG's `confirm`, the recorded/provisional split, the `network-configuration` fact, VIEW's provisional notice and CSCR's provisional confirmation wording all go or change.
There is no undo on this card: the operator remembers the settings they changed and puts them back. A configuration history with undo is a follow-up card.

### Sessions

The configuration session of CFG is the only exclusive one: power and service streams take any number of clients at once.

- Change on the Status screen's network row opens a session, held across the simple network screens (Network, Cable, Join wifi, Share hotspot); returning to the Status screen closes it.
- The full network settings editor, reached under Advanced, opens and closes its own session the same way.
- Change and the Advanced network settings entry stay enabled. Tapping either tries to open a session; where the device answers `busy`, the application stays on the Status screen and says "Someone else is making changes" in the row, in place of opening the screen.
- No device change for this: `busy` is already in CFG.

### Branding

The product is Tamanu Iti, and the client is branded as a Tamanu Iti setup tool, not a generic device tool. `bliti` stays the repository's codename.
Mockup lane 1's copy ("Tamanu Iti", "Find my Iti", "Save code for reprinting", "Recently used") follows from that, and needs a spec behind it.

## Follow-ups

Cards to split out, not built here:

- **Service facts and restarts:** the `service` facts for Tailscale and Tamanu, the SVC stream, and the service findings above.
- **Network configuration history:** design how a device keeps past configurations and offers undo back to one that worked.
- **Take over a busy session:** a "Take over" button beside "Someone else is making changes", ending the other client's network session so this one can open. Needs a way in CFG for one client to end another's session.
