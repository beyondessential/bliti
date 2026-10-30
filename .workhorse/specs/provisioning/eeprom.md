---
id: PEEP
---

# Programming the HAT EEPROM

The step `eeprom` of a [run](../overview.md#run) writes to a Tamanu Iti's HAT EEPROM the contents [HAT](../hat.md) defines for the board's version, while the operator bridges the pads that lift the EEPROM's write protection.

## Borrowed terms

| term | meaning |
| --- | --- |
| HAT EEPROM | As [NFO](../device-info.md) has it. On a Tamanu Iti v4 it is an onsemi CAT24C256 of 32 KiB. |
| ID bus | The I²C bus on GPIO0 and GPIO1 of the Pi's header, which carries the HAT EEPROM. |
| ID WP pads | The two pads, silkscreened ID WP, that lift the HAT EEPROM's write protection while bridged. |
| HAT+ | As [HAT](../hat.md) has it. |

## The image

A device MUST reach the HAT EEPROM at address `0x50` on the ID bus.

The step MUST write the image the provisioning medium carries for the board's version, `tamanu-iti-v<version>.eep` of [PMED](medium.md#what-the-medium-carries), from the EEPROM's first byte.

The step MUST place the board's version as [PHW](hardware.md#placing-the-board) has it each time it determines its done-state or is carried out, and MUST NOT take the version from the result of the step `hardware`.

> [!NOTE]
> An operator can choose this step without the hardware check, as for a board that is already imaged.

The EEPROM holds an image where its bytes, from its first, are those of the image for the image's length.

## What the EEPROM holds

A device MUST describe what the EEPROM holds as the first of these that applies.

| holds | where |
| --- | --- |
| the image | it holds the image for the board's version |
| blank | every byte reads `0xff` |
| a Tamanu Iti | it holds HAT+ vendor information whose `uuid` is the product UUID of [HAT](../hat.md#contents), described by its version |
| another HAT | it holds HAT vendor information, described by its `vendor` and `product` strings |
| unrecognised | anything else |

A device MUST replace each character of the `vendor` and `product` strings that is not printable ASCII before it shows them.

## Done-state

The step MUST determine its done-state by reading the EEPROM, and MUST NOT determine it from what the Pi's firmware publishes under `/proc/device-tree/hat/`.

The step MUST report:

| done-state | where |
| --- | --- |
| `done` | the board is placed and the EEPROM holds the image for its version |
| `not-done` | the board is placed and the EEPROM holds anything else |
| `unknown` | the board cannot be placed, the medium carries no image for its version, or the EEPROM cannot be read |

The step SHOULD give as its reason what the EEPROM holds, or why the done-state is `unknown`.

> [!NOTE]
> The firmware reads the EEPROM only as the board boots, so what it publishes describes the EEPROM as it was then.

## Carrying out the step

The step MUST fail, saying that the board could not be placed and needs looking at, where it cannot place the board.

The step MUST fail, naming the version, where the medium carries no image for the board's version.

The step MUST fail where it cannot read the EEPROM.

Where the EEPROM holds the image for the board's version, the step MUST pass without writing.

Where the EEPROM is blank, or holds a Tamanu Iti of the board's version, the step MUST write the image without asking.

Otherwise the step MUST open a `confirm` prompt with the id `overwrite`, whose text says what the EEPROM holds and the version it would be written as, and MUST write the image only where the operator confirms.

Where the operator declines, the step MUST fail, saying that the EEPROM was left as it was.

> [!NOTE]
> A Tamanu Iti of the board's version whose contents differ holds an earlier image of that version, and is brought up to date without asking.
> Anything else may be a board placed wrongly or a HAT that is not an Iti's, which is for the operator to judge.

## Writing

To write, the step MUST open a `notice` prompt with the id `id-wp`, whose text tells the operator to bridge the ID WP pads with bare metal, such as the blades of scissors, says that coated tools do not conduct, and says to hold the bridge until the step passes.

The step MUST try to write the image repeatedly until a write succeeds, for up to five minutes from when the prompt opened.

A write MUST count as succeeded only when the EEPROM reads back as holding the image, whatever the write itself reported.

Once a write has succeeded, the step MUST close the prompt and pass.

Where no write has succeeded within five minutes, the step MUST close the prompt and fail, with a reason telling the operator to check that bare metal bridges both ID WP pads.

> [!NOTE]
> While the pads are not bridged, the EEPROM refuses a write and the write reports an error.
> The step passing is the operator's cue to let go of the pads.
> A retry of a step that ran out of time gives the operator another five minutes.
