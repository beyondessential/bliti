---
id: PRV
---

# Provisioning mode

A device booted from a [provisioning medium](../overview.md#provisioning-medium) runs in [provisioning mode](../overview.md#provisioning-mode), in which an operator at the device sets it up through a client or through the device's console.

[PMED](medium.md) specifies the medium, [RUN](run.md) runs, [PRS](stream.md) the stream a client drives them over, [PWEB](client.md) the page of the web application that does, [PCON](console.md) the console command, and [PLED](led.md) the status LED.
[PHW](hardware.md), [PEEP](eeprom.md), [PDSK](disk.md), [PENC](encryption.md) and [PBL](bootloader.md) specify the steps of a run.

## Borrowed terms

| term | meaning |
| --- | --- |
| GPIO | A general-purpose input or output line of the board's processor, numbered by the processor's own scheme. |
| bootloader, boot order | As [PBL](bootloader.md) has them. |

## The mode

A device MUST run in provisioning mode when it has booted from a provisioning medium.

A device MUST NOT run in provisioning mode when it has not booted from a provisioning medium.

A device in provisioning mode MUST NOT serve any stream role or topic other than the hello stream of [MSG](../messages.md) and the provisioning stream of [PRS](stream.md).

## The boot order

A device has booted from a USB stick where the disk it booted from, as [PMED](medium.md#layout) has it, is attached by USB.

A device that has booted from a USB stick MUST, when it starts in provisioning mode, put USB first in the boot order, ahead of every other medium, and MUST leave the order of the other media as it was.

A device MUST NOT carry out a step of a run until the board holds that boot order in its bootloader's flash, whether the run is new or resumed.

A device MUST NOT write the bootloader where USB is already first.

A device MUST NOT change the boot order when it has not booted from a USB stick.

A device that cannot put USB first MUST report the cause as [DEV](../device.md) requires, and MUST go on running in provisioning mode.

> [!NOTE]
> A reboot partway through a run, from a step that needs one or from a power cut, comes back to the medium rather than to the system the run has written to the NVMe, so the run resumes.
> The boot order of the Pi as shipped and the one [PBL](bootloader.md#the-configuration) writes both try the SD card first, so a board booted from an SD card needs no change.
> Where holding the boot order takes a reboot, the device reboots, and finds the boot order held when it starts again.

## Going on the air

The [USER button](../overview.md#user-button) MUST be the input on GPIO27, which reads low while the button is held.

A device in provisioning mode MUST NOT advertise from the moment it starts until the USER button is pressed, whatever state a run it resumes is in.

A press of the USER button MUST open a window of five minutes in which the device advertises.

A device MUST stop advertising when a client connects.

A device MUST NOT advertise after a client that connected has gone, until the USER button is pressed again.

A press of the USER button while the device is advertising MUST restart the window.

A device MUST ignore a press of the USER button while a client is connected.

> [!NOTE]
> A device silent until pressed lets an operator pick the physical board: the boards a client's chooser lists are those just pressed and not yet connected.
> The five-minute window and the first connection both end the exposure of a device whose credential is public, as [SEC](../security.md) records.

## The advertised name

A device in provisioning mode MUST advertise the service UUID of [ADV](../discovery.md) in its advertisement, as ADV requires of every device.

A device in provisioning mode MUST advertise the local name `ITI_PROVISION_`, followed by the version marker of [VER](../version.md) in decimal, then `_`, then the suffix.

A device MUST serve that local name as the value of the Device Name characteristic of its GAP service, and MUST NOT change it while it runs.

The suffix MUST be the last four characters of the encoded payload of [QR](../qr-code.md) for the device's board ID.

A device in provisioning mode MUST be told, by means outside this protocol, which kind of source of [BID](../board-id.md) its board ID is to come from.

A device MUST NOT take the suffix from a source weaker than that kind.

A device MUST read its board ID from its source and derive the suffix each time it starts, and MUST NOT take it from the cache of [KEY](../key-schedule.md).

Where a device cannot establish that board ID, or cannot derive from it, the suffix MUST be `FAULT`, and the device MUST report the cause as [DEV](../device.md) requires.

A client MUST treat the version marker in the name as [VER](../version.md) requires of the advertised marker.

> [!NOTE]
> A name such as `ITI_PROVISION_1_K7QA` lets a client filter on the prefix up to the suffix for each marker it implements, and lets an operator tell apart boards pressed together by comparing the suffix with the one on a device's label.
> The suffix is the part of the payload that lies wholly within the key fingerprint, so broadcasting it reveals nothing secret.
> A board ID read from a weaker source would give a suffix, and so a label, that no longer matched once the stronger source answered.
> A step that changes the board, such as a firmware update to the TPM, can change the board ID, which is why the suffix is derived afresh at each start.

## The provisioning token

The provisioning token MUST be the 16 bytes of the ASCII text `ITI PROVISIONING`.

A device in provisioning mode MUST use the provisioning token in the place of a presence token in the derivation of the pre-shared key of [KEY](../key-schedule.md).

A client MUST use the provisioning token in the same place when it connects to a device in provisioning mode.

A device in provisioning mode MUST generate its static key at random each time it starts.

A device in provisioning mode MUST send, as the payload of the second handshake message of [CHN](../channel.md), 32 bytes generated at random.

A client MUST NOT compare anything against a key fingerprint when it connects to a device in provisioning mode, and MUST NOT treat the handshake as authenticating the device.

> [!NOTE]
> Everyone holds the provisioning token, so the handshake keeps a session from a passive observer and proves nothing about either end.
> What stands in for authentication is an operator pressing the button on the device.

## One client at a time

A device in provisioning mode MUST give out one slot only, and MUST answer a read of the slot allocation characteristic of [CHN](../channel.md) from any other connected client as for a device with no slot free.
