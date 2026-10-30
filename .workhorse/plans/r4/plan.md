# Check the hardware and program the EEPROM

The first two provisioning steps, `hardware` (PHW) and `eeprom` (PEEP), as `Step`s in `crates/bliti/src/provision/`, registered in the order RUN lists them.
Device tests on real boards happen against F4 once the whole flow is built, so this card is covered by unit tests over fakes of the bus, the lines and the TPM.

## Tech notes

**Where it goes.** A `provision/steps.rs` holding `hardware` and `eeprom` submodules, each split further where it nears the size limit (`eeprom/chip.rs`, `eeprom/hat.rs`, `hardware/place.rs`).
`step::registry()` returns them; it takes what the steps share from the daemon (below), so its signature changes and `daemon.rs` passes that in.
The `expect(dead_code)` on `context` and `model` in `provision.rs` come off once a step uses them.

**Hardware behind traits.** Each step holds its hardware through small traits, so tests drive them without a board:
an I²C device (`facts::i2c::Bus` in production), a line read by name (`button::Line`, already there), and a board ID read.
Every bus and line access is blocking, so it runs on `spawn_blocking`, as `button::Named` does.

**USER press since start.** `window::keep` consumes the button's presses, so the daemon wraps `button::user()` in a button that also sets a shared flag (a `watch` or an atomic) on each press.
The hardware step reads the flag. If no press is recorded, it opens the `user` notice and waits for the flag to change.

**CASE.** Poll `GPIO17` through `button::Named`, reusing `button.rs`'s `Debounce` for the stable level, rather than `gpio::Edges`.
An `Edges` read blocks a thread until the next edge, which the five-minute limit can't cancel cleanly.
`Debounce` currently reports only presses, so it grows a way to report a stable release too.

**Sensors.** `facts::i2c` and `facts::environment` are private to `facts`.
Make `i2c` `pub(crate)`, and give `bme680` and `ltr308` a one-shot `check(bus) -> Result<(), Problem>` that opens the chip, checks its ID and takes one reading, reusing the existing `open`/`measure` paths, the `Problem` wording and the gas-heater-off configuration.
The step maps `Problem` to its own reason text.

**Placing the board.** `place(bus) -> Option<Version>` reads the BME680's chip ID register (`0xd0`) at `0x77` on `/dev/i2c-3` with a plain register read, not through the driver, so placing doesn't soft-reset the chip.
It returns `facts::product::Version`.
Both steps call it: the EEPROM step calls it afresh in `done()` and `run()`.

**TPM.** Read the board ID from bliti-core's TPM Endorsement Key backend directly.
`identity::establish_provisioning` derives the memory-hard root, which the check doesn't need.

**The ID bus.** The medium's firmware brings it up as `/dev/i2c-9` (bit-banged on GPIO0/1), with no `at24` driver bound, so bliti talks to the CAT24C256 over raw `I2C_RDWR` through `facts::i2c::Bus`:
- reads: a two-byte big-endian address write, then the read, in one transfer; chunks of at most 8192 bytes, which is i2c-dev's limit per message
- writes: one 64-byte page per message (address plus data), then acknowledge-polling the address until the write cycle completes (a few milliseconds)
- while WP is high the chip NACKs the data bytes, which `Bus` reports as `NoAnswer`; in the write loop any error just means not written yet
- `done()` reads only the image's length; checking for blank (every byte `0xff`) reads all 32 KiB (a few seconds on the bit-banged bus), and only the run does that

**Images.** `/usr/share/bliti/provision/tamanu-iti-v<major>.<minor>.eep`, as `provision/build.sh` installs them.
The directory is a constant beside the step, overridable in tests.

**Describing what the EEPROM holds.** Parse the HAT header (signature `R-Pi`, version, atom count, length) and the vendor-info atom (uuid, `product_id`, `product_ver`, the two length-prefixed strings) with winnow's binary parsers (`cargo add winnow`, per AGENTS).
Atom CRCs aren't checked: this only describes the contents for the operator, and decides whether to ask before overwriting.
A header with version `0x02` and the Iti UUID is "a Tamanu Iti"; any other parsable vendor atom is "another HAT", named by its strings with non-printable characters replaced; anything else is "unrecognised".

## Checklist

### Shared plumbing
- [x] `facts.rs`: `pub(crate) mod i2c`; expose `environment`'s chip checks to `provision`
- [x] `facts/environment/bme680.rs`, `ltr308.rs`: a one-shot `check` that opens, checks the ID and takes one reading; `environment` exposes `check_air`, `check_light` and `air_chip_id`, which give the reason as text
- [x] `provision/button.rs`: let `Debounce` report a stable release as well as a press
- [x] `provision/button.rs` or `daemon.rs`: a button wrapper that records a USER press since start, shared with the steps
- [x] `provision/step.rs`: `registry` takes the shared pieces and returns `[hardware, eeprom]`; update `daemon.rs`
- [x] `provision.rs`: drop the `dead_code` expectations on `context` and `model`, leaving them on the methods no step calls yet
- [x] A prompt's id is the key a step gave it (`user`, `case`, `id-wp`, `overwrite`), as the specs and the client's pictures name them; `Context::notice` takes a key. The model had built `{step}-{key}` and `notice-N`

### Placing the board (PHW)
- [x] `provision/steps/hardware/place.rs`: the version table and `place()`, from the BME680 chip ID at `0x77` on I²C3
- [x] Tests: v4.0 placed from `0x61`; another ID, no answer and no bus place nothing

### Hardware check (PHW)
- [x] `provision/steps/hardware.rs`: the step, with id `hardware` and title "Check the hardware"; `done()` returns `not-done`
- [x] `user`: pass if a press is recorded; otherwise the `user` notice, closing on a press, failing after five minutes
- [x] `case`: the `case` notice; pass on a debounced low then high on `GPIO17`; fail after five minutes or where the line can't be read
- [x] `air`, `light`: the chip checks, each reason naming the chip and, for a wrong part, the ID read
- [x] `tpm`: read the board ID from the TPM EK backend
- [x] Carry out every check, set the result (`checks`, `version`) whether passing or failing, and fail with a reason naming each failed check and "could not be placed" where it applies
- [x] Tests with fake lines, bus and TPM: a press recorded skips the `user` prompt; CASE pass, timeout and unreadable line; bounce on CASE isn't a press; a wrong-part sensor; every check still runs after an early failure; the result's shape

### EEPROM step (PEEP)
- [x] `provision/steps/eeprom/chip.rs`: the CAT24C256 over a bus trait: chunked reads, paged writes with acknowledge polling
- [x] `provision/steps/eeprom/hat.rs`: the winnow parser and the "what it holds" description (image, blank, Tamanu Iti of a version, another HAT, unrecognised)
- [x] `provision/steps/eeprom.rs`: the step, with id `eeprom` and title "Program the EEPROM"
- [x] `done()`: place, find the image, read the image's length off the chip; `done` / `not-done` / `unknown` with a reason saying what it holds or why it's unknown
- [x] `run()`: fail if it can't place the board, has no image or can't read the chip; pass without writing if the chip already holds the image; write without asking if the chip is blank or holds the same Iti version; otherwise the `overwrite` confirm, failing on a decline
- [x] Writing: the `id-wp` notice (bare metal, coated tools don't conduct, hold until the step passes); retry until the chip reads back as the image, for five minutes; close and pass, or close and fail with the check-the-bridge reason
- [x] Tests over a fake chip that refuses writes until "bridged": write after N refusals; a write that reports success but reads back wrong is retried; timeout; blank and same-version written without a prompt; another HAT and unrecognised contents ask, with sanitised strings; decline fails and leaves the chip untouched; `done()` for each case
- [x] Parser tests against the real v4.0 image built from `hat/tamanu-iti-v4.0.txt` (checked in as a fixture), a legacy-format header, and truncated or garbage input

### Wrap-up
- [x] `// spec: PHW` / `// spec: PEEP` on the code that carries each rule
- [x] `provision.rs` module doc: add PHW and PEEP to the list of specs
- [x] `cargo fmt`, clippy, tests; the web tests are unaffected
