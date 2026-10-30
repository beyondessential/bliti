---
id: PHW
---

# Checking the hardware

The step `hardware` of a [run](../overview.md#run) exercises the devices on a Tamanu Iti's HAT, so a faulty board is caught at the bench, and places the board as a version for the step `eeprom` of [PEEP](eeprom.md) to write.

## Borrowed terms

| term | meaning |
| --- | --- |
| HAT EEPROM | As [NFO](../device-info.md) has it. |
| case switch | As [NFO](../device-info.md) has it. Its silkscreen label is CASE. |
| TPM | A Trusted Platform Module, as [BID](../board-id.md) has it. |

## The checks

The step MUST carry out the checks below, in the order the table gives.

| check | passes where |
| --- | --- |
| `user` | the USER button has been pressed, as [The USER button](#the-user-button) has it |
| `case` | the case switch has been pressed and released, as [The case switch](#the-case-switch) has it |
| `air` | the air sensor answers and yields a reading, as [The sensors](#the-sensors) has it |
| `light` | the light sensor answers and yields a reading, as [The sensors](#the-sensors) has it |
| `tpm` | the TPM yields the board ID, as [The TPM](#the-tpm) has it |

The step MUST carry out every check, whether or not a check before it failed.

### The USER button

The step MUST pass `user` where the device has seen a press of the [USER button](../overview.md#user-button) since it started.

Where it has not, the step MUST open a `notice` prompt with the id `user` asking the operator to press the USER button, and MUST pass `user` and close the prompt once it sees a press.

> [!NOTE]
> The press that put the board on the air counts, so an operator who found the board through a client is not asked again.
> An operator at the console who has not pressed it is asked.

### The case switch

The step MUST read the case switch from the line the kernel names `GPIO17`, which reads low while the switch is held down.

The step MUST open a `notice` prompt with the id `case` asking the operator to press the CASE lever and let it go.

The step MUST pass `case` once the line has read low and then high since the prompt opened, and MUST close the prompt then.

The step MUST NOT take contact bounce as a press or a release.

> [!NOTE]
> A board is checked with its case open, so the lever is up until the operator presses it.

### Waiting on the operator

The step MUST fail `user` or `case` where it has not passed within five minutes of its prompt opening, and MUST close the prompt then.

The step MUST fail `user` or `case`, with a reason, where it cannot read the line.

### The sensors

The step MUST reach the sensors on the third I²C bus.

The step MUST pass `air` where a device answers at address `0x77` with the chip ID `0x61` of a Bosch BME680, and yields a reading of temperature, humidity and air pressure.

The step MUST pass `light` where a device answers at address `0x53` with the part ID `0xb1` of a Lite-On LTR-308ALS, and yields a reading of light.

The step MUST fail a sensor's check with a reason saying whether nothing answered, a device answered with another ID, which it MUST give, or the sensor yielded no reading.

The step MUST NOT heat the BME680's gas sensor.

### The TPM

The step MUST pass `tpm` where it reads a board ID from the TPM Endorsement Key, as [BID](../board-id.md) has it.

The step MUST fail `tpm`, with a reason, where the TPM does not answer or yields no board ID.

## Placing the board

A board's version for provisioning MUST be placed from the devices on its HAT, by the table below.

| version | placed where |
| --- | --- |
| 4.0 | a Bosch BME680 answers at address `0x77` on the third I²C bus with the chip ID `0x61` |

A device MUST place a board as the version of the one row that matches it, and MUST NOT place a board that no row, or more than one row, matches.

A device MUST NOT place a board from what its HAT EEPROM holds, or from anything an operator says.

> [!NOTE]
> Placing a board from the devices that answer misreads a board with a dead or missing part, which is why a running system never does it and reads its version from the HAT EEPROM, as [HAT](../hat.md) has it.
> Provisioning does it only to decide what the EEPROM is to hold, and from then on the EEPROM is what tells the version.
> Later revisions of the board carry other sensors, and each adds a row.

## The outcome

The step MUST pass where every check passes and the board is placed.

Otherwise the step MUST fail, with a reason naming each check that failed and why, and saying that the board could not be placed and needs looking at where it could not.

The step MUST report a result, whether it passes or fails, carrying:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `checks` | array | yes | every check, in the order of [The checks](#the-checks), each as below |
| `version` | string | no | the version the board was placed as, where it was |

Each member of `checks` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `id` | string | yes | the check, as [The checks](#the-checks) names it |
| `passed` | boolean | yes | whether it passed |
| `reason` | string | no | why it failed, where it did |
