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

The Status screen's Tailscale and Tamanu rows needed device work, now landed by M3 and N3 as specific facts and streams (TSC, TMU) in place of F3's generic `service` fact and SVC stream. F3's SVC and NFO `service` drafts are not adopted.

### Gaps and contradictions to fix

- **No carrier fact.** NSET shows "whether a wired interface has carrier" and the cable screen waits on carrier, but NFO reports no carrier. CFG's `state` only covers enabled candidates, and on wifi the wired attachments are off, so carrier can't be observed from wifi, which is exactly where the cable flow starts. Settled: NFO gains a carrier fact per wired interface, reported whether or not its attachment is enabled.
- **WIFI's ordering rule contradicts itself.** It says to leave the ordering as it is, and also to send the joined network to the top, without saying whether joining replaces the one candidate it presents or adds one. Settled: joining replaces that candidate in place, keeping the ordering; the old network's credentials are not kept, which keeps the configuration within the simple states.
- **When the simple screens hold a session is unsaid.** Settled below under Sessions.
- **Service findings:** resolved by M3 and N3. An accepted act that fails sends `tailscale-failed` or `tamanu-failed`; Tamanu reports `restarting` on the feed while a restart is under way; `tailscale-name` stays `passed` in every state that holds it; a Tamanu restart that half-succeeds leaves `tamanu-state` saying which.
- **Stale reference.** WEB:105 still returns to "the device view" after a reconnect; it should be the Status screen.
- **NFO wording.** "MUST NOT rely on the name's meaning being understood to report the service" doesn't parse.

### Decisions to confirm

- [x] Device-side work for Tailscale and Tamanu goes to a follow-up card each.
- [x] Cable screen: a new carrier fact tells it a cable is in.
- [x] Another client holding the session is found out on tap (see Sessions).
- [x] Leaving after Apply changes nothing: the configuration is already saved. Join wifi has no Cancel.

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

The sequence: Apply sends the document; the device saves it, applies it, and reports whether it worked.
- Leaving before Apply loses the edits. As in NSCR, the simple screens ask before leaving with edits not applied, and the browser warns before the page closes; a dropped channel loses them without asking.
- After Apply the app shows it is applying, with a spinner and without saying "confirmed", until the device answers, then shows any errors. There is no Cancel, on Join wifi or anywhere else.
- Where no answer comes (the channel dropped, the device crashed or lost power), the app drops the attempt and shows the device as disconnected, as after any closed channel. The operator walked away or the device needs fixing in person, and the app can't tell which.

Landed by K3 across protocol, device and the shipped client.
There is no undo on this card: the operator remembers the settings they changed and puts them back. A configuration history with undo is a follow-up card.

### Sessions

The configuration session of CFG is the only exclusive one: power and service streams take any number of clients at once.

- Change on the Status screen's network row opens a session, held across the simple network screens (Network, Cable, Join wifi, Share hotspot); returning to the Status screen closes it.
- The full network settings editor, reached under Advanced, opens and closes its own session the same way.
- Change and the Advanced network settings entry stay enabled. Tapping either tries to open a session; where the device answers `busy`, the application stays on the Status screen and says "Someone else is making changes" in the row, in place of opening the screen.
- Since K3 a device serves every session, the longest-served holds the configuration and the rest are busy, each told so by `busy` on `state`. Tapping Change opens a session and reads the first `state`: busy closes it again and shows the note; not busy carries on to the screen.
- The Status screen can't hold a session idly to grey Change out up front: an idle session would itself be the holder and lock everyone else out.

### Branding

The product is Tamanu Iti, and the client is branded as a Tamanu Iti setup tool, not a generic device tool. `bliti` stays the repository's codename.
Mockup lane 1's copy ("Tamanu Iti", "Find my Iti", "Save code for reprinting", "Recently used") follows from that, and needs a spec behind it.

## Implementation options

### What's there

The client (`crates/bliti-web/app`) is React 19 with no router or state library.
`App.jsx` holds everything for a connected device and switches on a `screen` string (`device`, `control`, `network`), each screen built by `screenOf(title, actions, body)`.
`Network.jsx` is NSCR: its session lives exactly as long as it is mounted, and `network.js` holds the stage reducer (editing, applying, errored, watching, all driven by the device's `state`) and the document edits as pure functions.
The client has no Tailscale or Tamanu stream yet; it only renders their facts as readings.
Tests are Playwright against `tests/fake-client.js`, a fake device at the message layer; pure modules are tested in Playwright without a page (`capabilities.spec.js`).

### Screens

New screens: Status (the landing), Network, Cable, Join wifi, Share hotspot.
Renamed and re-parented: Info becomes All readings, Control becomes Power and battery (its network section goes), NSCR becomes Network settings, all three under Advanced.

### The simple screens' session

Change has to learn about `busy` before leaving the Status screen, so the session can't be scoped to the simple screens' mount the way NSCR's is.
Proposed: lift session opening into a hook App owns. Change calls it, waits for the first `configuration` or `busy`, and only then switches to the Network screen or shows "Someone else is making changes". The simple screens share that one handle, and returning to Status closes it.
NSCR's entry under Advanced goes through the same hook, so both entries handle `busy` the same way.

### The simple screens' logic

A pure module beside `network.js`, holding:
- the classifier for the four simple states, which sends anything else to Advanced;
- the transitions as document edits (join wifi, use the cable, hotspot on and off), each returning the document to propose;
- the way marked in use.

The screens stay thin renderers over it, and the module is tested without a page like `capabilities.spec.js`.

### Navigation

Each screen change pushes a browser history entry, so the phone's back gesture goes up one level, as the on-screen Back does. Back from the Status screen leaves the app as now. No router library: the History API and a `popstate` handler in App.
Going back out of the simple network screens closes the session, as returning to Status does.

### Device dependencies

One-step apply (K3), the carrier fact (L3, `wired-carrier`), Tailscale (M3) and Tamanu (N3) have all merged.

### Service rows

The Tailscale row reads `tailscale-state` and `tailscale-name`; the Tamanu row reads `tamanu-state` and the two version facts.
Each row's action comes from the acts its stream lists (TSC, TMU), so the device decides what's offered, and the Status screen opens a Tailscale and a Tamanu stream while it's showing.
Tamanu offers Start while `stopped` and Restart while `running` or `degraded`, and shows its own `starting`/`restarting` state from the feed.
Tailscale has no restart: its row offers what TSC lists for the state (Turn on when stopped, Sign in when signed out or expired), and nothing while running. The mockup's "Restart Tailscale" goes.

## Splitting

Device-side and protocol work is created as cards of its own, independent of this one, worked and merged first.
This card then comes back to evaluate what they landed, rebases on main, and splits out more where needed, until all that's left is changing the UI.
At that point we decide whether the UI change is done here in one go or split further.

## Follow-ups

Proposed split. Each card is independent of this one.

Created and merged:

1. **One-step network apply · K3.** Apply saves at once: CFG loses `confirm`, `discard` and the recorded/provisional split, and NFO loses `network-configuration`. The device's session and apply code changes to match, and so does the shipped client: NSCR's stages drop applied and confirm, and App's kept-session bar, the provisional notice and Control's provisional wording go. The largest of the four.
2. **Wired carrier fact · L3.** NFO reports whether each wired port has a cable in, whether or not its attachment is enabled. The shipped client shows it as All readings would. Small, and parallel with 1.
3. **Tailscale status and restart · M3.** The device reports whether Tailscale is up and the name the device is reached by, and restarts it on request without dropping the channel. Defines the restart exchange. Carries the overlay findings above.
4. **Tamanu status and restart · N3.** The device reports Tamanu's web and facility server with their versions, and restarts both together. Reuses 3's restart exchange, so it follows 3. Carries the half-succeeded restart.

Short board and OS names · A4: the device reports short forms of board and OS for the Status screen's identity line, beside the full `board` and `os`.

Created after K3 merged:

5. **Network configuration history · S3.** How a device keeps past configurations and offers undo back to one that worked.
6. **Take over a busy session · T3.** A "Take over" button beside "Someone else is making changes", making this session the holder in place of the longest-served one.

### The UI, split

Mockup lanes: 1 connect, 2 status, 3 choose how the Iti connects, 4 join wifi, 5 share hotspot, 6 Advanced.

7. **Restyle the existing screens for Tamanu Iti · U3** (lanes 1 and 6). Branding, renames, the shared screen shell, history navigation. First.
8. **Status screen · V3** (lane 2). The landing, its rows showing state only, Advanced, and the busy note. Its identity line is short (board and OS only, "Raspberry Pi 5 · Ubuntu 26.04"); All readings keeps the full one. Follows U3 and A4.
9. **Simple network screens · W3** (lanes 3 to 5). Network, Cable, Join wifi, Share hotspot. Follows V3.
10. **Tailscale and Tamanu actions on the Status screen · X3.** The rows' acts over TSC and TMU, and Tailscale's sign-in design. Follows V3, independent of W3.

Each card adopts its share of F3's UI specs, with this doc's decisions and the review's fixes.

