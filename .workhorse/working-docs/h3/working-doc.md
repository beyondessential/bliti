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

### Gaps and contradictions to fix

- **No carrier fact.** NSET shows "whether a wired interface has carrier" and the cable screen waits on carrier, but NFO reports no carrier. CFG's `state` only covers enabled candidates, and on wifi the wired attachments are off, so carrier can't be observed from wifi, which is exactly where the cable flow starts. Needs a carrier fact in NFO, or the cable flow turning wired on first.
- **WIFI's ordering rule contradicts itself.** It says to leave the ordering as it is, and also to send the joined network to the top. It also doesn't say whether joining another network replaces the one candidate it presents or adds one beside it; adding accumulates candidates, while replacing loses the old network's credentials.
- **When the simple screens hold a session is unsaid.** SHARE renders the hotspot's name and password from the configuration, so it needs a session open just to show the screen. With one-step apply nothing outlives a screen, but nothing says when a session opens or closes, nor handles `busy` when another client holds the one session.
- **A failed restart reads "restarting" forever.** STAT says restarting until the feed shows the service running again. SVC lets a device accept a restart and then fail it, and tells the client nothing. Needs a timeout, or a terminal message on the service stream.
- **"Leaving" its running state is ambiguous.** NFO's note has the restart seen as the entry "leaving and returning", which reads like `ended`. SVC means `failed` and then `passed`. Say which.
- **The overlay row can't always show the device name.** STAT shows it "whether the row is well or not", but NFO carries no value on a `broken` entry.
- **Restarting Tamanu can half-succeed.** STAT restarts both Tamanu services as one act through two `restart`s. One refused and the other accepted isn't covered.
- **Stale reference.** WEB:105 still returns to "the device view" after a reconnect; it should be the Status screen.
- **Provisional is lost from the landing.** VIEW's provisional notice now sits under Advanced, and STAT never renders `network-configuration`. A second client watching while someone else's proposal is applied gets no sign of it.
- **Turning the hotspot off doesn't bring back what it replaced.** SHARE's note says it restores the connections the hotspot took the place of, but the rule enables every wired attachment and the WIFI candidate, whatever they were before.
- **NFO wording.** "MUST NOT rely on the name's meaning being understood to report the service" doesn't parse.

### Decisions to confirm

- [ ] Whether the device-side work (`service` facts, SVC) belongs on H3, on F3, or on a card of its own. Deferred to when the card is split.

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
Recovering a previous working configuration would come from a history the device keeps, offered as undo once the operator reconnects. No history is specced today.

### Branding

The product is Tamanu Iti, and the client is branded as a Tamanu Iti setup tool, not a generic device tool. `bliti` stays the repository's codename.
Mockup lane 1's copy ("Tamanu Iti", "Find my Iti", "Save code for reprinting", "Recently used") follows from that, and needs a spec behind it.
