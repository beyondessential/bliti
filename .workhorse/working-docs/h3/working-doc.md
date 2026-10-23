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

- **No carrier fact.** NSET shows "whether a wired interface has carrier" and the cable screen waits on carrier, but NFO reports no carrier. CFG's `state` only covers enabled candidates, so while the hotspot has the wired attachments off, carrier can't be observed at all, and the cable flow can never start. Needs a carrier fact in NFO, or the cable flow turning wired on first.
- **WIFI's ordering rule contradicts itself.** It says to leave the ordering as it is, and also to send the joined network to the top. It also doesn't say whether joining another network replaces the one candidate it presents or adds one beside it; adding accumulates candidates, while replacing loses the old network's credentials.
- **When the simple screens hold a session is unsaid.** SHARE renders the hotspot's name and password from the configuration, so it needs a session open just to show the screen. NSET says to close the session "once a change is confirmed or has failed", and to confirm even after the operator has left the screen, so the session outlives the screen. Neither says when a session opens. Nor does anything handle `busy`, when another client holds the one session.
- **A failed restart reads "restarting" forever.** STAT says restarting until the feed shows the service running again. SVC lets a device accept a restart and then fail it, and tells the client nothing. Needs a timeout, or a terminal message on the service stream.
- **"Leaving" its running state is ambiguous.** NFO's note has the restart seen as the entry "leaving and returning", which reads like `ended`. SVC means `failed` and then `passed`. Say which.
- **The overlay row can't always show the device name.** STAT shows it "whether the row is well or not", but NFO carries no value on a `broken` entry.
- **Restarting Tamanu can half-succeed.** STAT restarts both Tamanu services as one act through two `restart`s. One refused and the other accepted isn't covered.
- **Stale reference.** WEB:105 still returns to "the device view" after a reconnect; it should be the Status screen.
- **Provisional is lost from the landing.** VIEW's provisional notice now sits under Advanced, and STAT never renders `network-configuration`. A second client watching while someone else's proposal is applied gets no sign of it.
- **Turning the hotspot off doesn't bring back what it replaced.** SHARE's note says it restores the connections the hotspot took the place of, but the rule enables every wired attachment and the WIFI candidate, whatever they were before.
- **NFO wording.** "MUST NOT rely on the name's meaning being understood to report the service" doesn't parse.

### Decisions to confirm

- [ ] Hotspot never alongside a cable: NSET and SHARE forbid hotspot plus cable, so sharing always takes the device offline (SHARE says so). HOT supports `share-upstream`, and a cabled clinic sharing its uplink is the obvious case. Is the device meant to be offline whenever it shares, Tamanu's sync included?
- [ ] NSCR now discards everything on leaving, without asking, including an applied proposal that's working. That's a sharp edge for a technician who taps Back by mistake.
- [ ] Auto-confirm on `applied` for the simple screens gives up CFG's provisional safety net. The note relies on the BLE channel to undo things, which holds.
- [ ] Mockup lane 1 renames the app to "Tamanu Iti" and its copy ("Find my Iti", "Save code for reprinting", "Recently used"), with no spec change behind it. The client is titled `bliti` today.
- [ ] Whether the device-side work (`service` facts, SVC) belongs on H3, on F3, or on a card of its own.
