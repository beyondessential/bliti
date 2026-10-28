# Case switch: plan

Report `case-closed` and `case-last-opened` on a v4.0, keep the history of openings and closings in `/var/lib/bliti/case-history.jsonl`, and give the device view a case tile.
The specs are the Case section of NFO and the case rules in VIEW; the scenarios are in the test cases.

## Notes

**Shape.** The case follows the backup supply's pattern: a thread started with the device that never stops, holding shared state that the fact poll reads from.
The thread owns the GPIO line and the history file.
`facts_now` asks it for the current state and takes both case facts from the same snapshot, so a change lands in one batch.
Sampling going idle doesn't touch it.

**Edges and bounce.** The power code reads GPIO6 through the v1 character-device ABI, which only polls a value.
The case needs edge events, so the GPIO module gains a v2 line request: both edges, with the kernel's debounce period attribute.
gpiolib debounces in software where the chip can't, so bounce never reaches bliti.
The debounce period is a code constant.

**Prompt delivery.** The session polls facts every two seconds, which is how `wired-carrier` meets "as soon as it observes the change".
The case thread also publishes on a `tokio::sync::watch`, and the feed loop wakes on it as well as on the poll, so an opening goes out at once.

**The history file.** Open it append-only, write one line per event and `fsync` it before updating the shared state.
After creating the file, `fsync` its directory too.
Read it once at start to find the latest entry and the latest opening.
A final line torn by a power cut is ignored when reading.
Before the next append, a newline is written if the file doesn't end in one, so nothing earlier is rewritten.
If the file can't be read, `case-last-opened` is `broken`.
If an append fails, the error is logged and `case-closed` still follows the switch.
`time` is `jiff::Timestamp::now()`, `boot.id` comes from `/proc/sys/kernel/random/boot_id`, and `boot.offset` is `Facts::since_boot()` taken when the event is handled.

**The gate.** Only a v4.0 has the switch.
`product.rs` on main already parses `Hat::Iti(Version)`, so it gains a small public accessor returning the version, and the case thread starts only when that version is 4.0.

**Provisioning mode (K4).** K4's provisioning mode serves no `default` feed, so it never reports case facts.
The only rule left for that mode is not keeping a history, and the device meets it by not starting the case thread there.
This card only starts it from the ordinary device startup.
Whichever of K4 and this card merges second has to check that provisioning startup doesn't start it.
Once K4 merges, NFO's borrowed term "provisioning medium" can link to the definition in K4's overview spec.

## Checklist

### Base

- [ ] Rebase onto main, so the HAT spec and the major.minor `product` parsing are on the branch

### GPIO

- [ ] Move `facts/power/gpio.rs` to a crate-level `gpio` module, since the case uses it too, and update the power code to match
- [ ] Add a v2 line request by name: input, both edges, a debounce period, returning a handle whose events can be read along with the line's current value
- [ ] Unit-test the struct layouts against the kernel ABI sizes, and the line lookup by name, as the existing GPIO tests do

### History

- [ ] `facts/case/history.rs`: the `Entry` line type (`event`, `time`, `boot { id, offset }`, `found`) with serde, reading the latest entry and the latest opening from a file, and appending with `fsync`
- [ ] Tests: round-trip a line, find the latest entry and opening, ignore a torn final line, add a newline before appending after a torn line, leave earlier bytes untouched
- [ ] Tests for the start comparison: empty history and closed adds nothing; empty and open adds a found `opened`; a last line that differs adds a found entry with the current boot; a matching last line adds nothing

### Watcher

- [ ] `facts/case.rs`: a `Case` handle (cloneable, shared state plus `watch::Receiver`), started on its own thread with the line and the history path injected, so tests drive it with a fake line
- [ ] On start, read the switch, compare it with the history, and append any found entry; then loop on edge events, appending and publishing each change
- [ ] `Case::entries(at)`: `case-closed` (`passed` when closed, `warning` "the case is open" when open, `broken` when the line can't be read) and `case-last-opened` (value, `boot` and `found` traits, `passed`, `broken` when the history can't be read, absent with no opening)
- [ ] Tests with a fake line: an open, a close, an unreadable line, an unreadable history, and both facts changing together on an opening

### Wiring

- [ ] `device.rs`: start `Case` beside the supply's record thread when the product is v4.0, and pass it to the sessions
- [ ] `session.rs`: add the case entries in `facts_now`, and wake the feed loop on the case's `watch` as well as on `facts_poll`
- [ ] Session test: an opening yields one batch carrying `case-closed` false and the new `case-last-opened`

### Web client

- [ ] `readings.js`: add `case-closed` to the tile order before `last-boot`, `case-last-opened` to the reveal-only list, the labels, and the boolean words Closed and Open
- [ ] `Readings.jsx`: render `case-last-opened` in the `case-closed` reveal as how long ago and the instant, saying it was found at startup where `found` is true
- [ ] Playwright tests in `readings.spec.js`: tile placement and words, warning colour when open, the reveal with and without `found`, no tile for `case-last-opened`, the tile without `case-last-opened`

### On the prototype

- [ ] Deploy with the `bliti-prototype` skill, then run the hands-on cases: open and close with a client connected, open with no session for over thirty minutes, cut power just after an opening, and boot with the case open
- [ ] `cargo fmt`, clippy, and the full test suites
