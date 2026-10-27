# Short board and OS names

## Shape

One descriptive `short` trait, carried on `board` and `os` (NFO, Identity).
No new facts, so All readings has nothing new to place and keeps its full header line.

- Board: "Raspberry Pi 5" where the Pi revision code's board-type field (bits 4–11 of a new-style code) is a Pi 5-generation type: 0x17 Pi 5, 0x18 CM5, 0x19 Pi 500, 0x1a CM5 Lite (check the current list in Raspberry Pi's revision-code docs). Any other type, an old-style code, or a DMI machine carries no `short`.
- OS: os-release `NAME` and `VERSION_ID`, or `NAME` alone where there is no `VERSION_ID`.

## Sequencing

V3 (Status screen, PR #48) merges first with the full identity line.
This card then rebases onto it and switches the Status screen's identity line to `short`, falling back to the fact's value where a fact carries none, editing STAT and the client.

## Steps

- [x] Device: decode the board type and carry `short` on `board`; carry `short` on `os`
- [x] Device tests: type decoding (known, unknown, old-style code), os-release with and without `VERSION_ID`
- [ ] Rebase onto V3 once merged
- [ ] STAT: identity line from `short` on `board` and `os`, falling back to their values
- [x] Client: `short` is descriptive in series keying; All readings unchanged
- [ ] Client: Status screen identity line
