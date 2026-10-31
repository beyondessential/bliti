# Battery facts on the power and battery screen

Scenarios for the battery section of the power and battery screen, as CSCR has it.

## Battery blocks

- [x] The battery section is left out while the device reports no battery charge, even where it has answered with a curve (verifies spec: CSCR)
- [x] A single battery headlines its charge with its direction beside it and its scale beneath, then lists power source, time to full or time left, and voltage, without a battery name (verifies spec: CSCR)
- [x] Several batteries each get a named block, ordered by name with `built-in` last, each listing only its own figures, with the power source shown once above them (verifies spec: CSCR)
- [x] A low charge colours the value and bar as a warning and shows the device's reason beneath the bar, and a failed charge colours it as bad (verifies spec: CSCR)
- [x] A skipped direction shows a dash with its reason, and a broken voltage shows as unavailable with its reason (verifies spec: CSCR)
- [x] A newer value replaces the one shown, and a figure the device ends is removed (verifies spec: CSCR)
- [x] Voltage and direction are labelled in the same words as on All readings (verifies spec: CSCR)

## Battery curve

- [x] The curve is left out until the device answers with a document, and where it answers with none (verifies spec: CSCR)
- [x] The curve is left out where the device reports no `built-in` battery (verifies spec: CSCR)
- [x] The curve sits directly beneath the `built-in` battery's block (verifies spec: CSCR)

## On a device

- [ ] On the prototype, the battery section shows the live charge, direction and voltage, and moves between time left and time to full as mains is unplugged and plugged back in
