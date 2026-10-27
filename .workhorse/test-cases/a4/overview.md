# Short board and OS names

## Board

- [x] Every Pi 5-generation revision code (5, CM5, 500, CM5 Lite, any memory or revision) carries `short` "Raspberry Pi 5" (verifies spec: NFO)
- [x] Pi 4, CM4, 400 and CM0 codes carry no `short` (verifies spec: NFO)
- [x] An old-style revision code carries no `short`, even where its low bits read as a Pi 5 type (verifies spec: NFO)
- [ ] On the prototype device, `board` carries `short` "Raspberry Pi 5" and keeps its full model as the value

## OS

- [x] os-release with `NAME` and `VERSION_ID` gives `short` "Ubuntu 26.04" beside the pretty name (verifies spec: NFO)
- [x] os-release without `VERSION_ID` gives the name alone (verifies spec: NFO)
- [x] os-release without `NAME` carries no `short` (verifies spec: NFO)

## Readers

- [x] All readings keeps the full board and os line where they carry short forms (verifies spec: VIEW)
- [x] The Status screen's identity line reads "Raspberry Pi 5 · Ubuntu 26.04" from the short forms (verifies spec: STAT)
- [x] The Status screen falls back to a fact's value where it carries no `short` (verifies spec: STAT)
