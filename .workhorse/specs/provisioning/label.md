---
id: PLBL
---

# Making the label

The step `label` of a [run](../overview.md#run) gives a client what it needs to print the board's label: the board's QR payload, its version, and when the step made them.
It changes nothing on the board, and the device draws no label.
[PSHT](sheet.md) specifies the label a client draws from the result.

## Borrowed terms

| term | meaning |
| --- | --- |
| payload | As [QR](../qr-code.md) has it. |
| HAT EEPROM | As [NFO](../device-info.md) has it. |
| version | As [HAT](../hat.md#contents) has it. |

## Done-state

The step MUST report `not-done`.

## Carrying out the step

The step MUST take the payload from the board ID the device established when it started, which is the board ID whose suffix the device advertises, and MUST NOT read the board ID again.

The step MUST fail where the device established no board ID, saying why, as [DEV](../device.md) requires of the device.

The step MUST take the board's version from the HAT EEPROM where the EEPROM holds a Tamanu Iti, as [PEEP](eeprom.md#what-the-eeprom-holds) has it, and MUST otherwise place the board as [PHW](hardware.md#placing-the-board) has it.

The step MUST read the EEPROM itself, and MUST NOT take the version from what the Pi's firmware publishes under `/proc/device-tree/hat/` or from the result of another step.

The step MUST fail, saying that the board could not be placed and needs looking at, where neither gives a version.

> [!NOTE]
> An EEPROM that holds a Tamanu Iti is what the running system will report, so it wins where it is there.
> That includes one that `eeprom` has just written, whose version is the one the board was placed as.
> A run that chooses `label` alone on a board whose EEPROM is blank still gets a label, from the devices on the HAT.

## The result

The step MUST report a result carrying:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `payload` | string | yes | the text the QR code of the board encodes, as [QR](../qr-code.md#the-code) has it |
| `version` | string | yes | the board's version |
| `at` | string | yes | when the step made the result, as an RFC 3339 date and time in UTC |

The result MUST NOT carry the recovery passphrase, or anything else that unlocks the board's disk.

The result of a step that fails MUST NOT carry `payload`.

> [!NOTE]
> The label is the last step of a run, and a run goes on without a client, so a client usually first sees the result when it connects to a finished run.
> The payload holds the presence token, so it is as sensitive as the code it makes, and [SEC](../security.md) records where it travels and is kept.
> `at` is what tells a client that has already drawn a label from a result apart from a result that is new.
