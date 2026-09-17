---
id: BID
---

# Board ID

A board MUST yield the same [board ID](overview.md#board-id) every time it is read, so that a QR code can be reproduced from the board rather than from a record of what was issued.

A board ID is not a secret, and any software on a device can read it.

> [!NOTE]
> Nothing depends on a board ID staying hidden, only on its being expensive to search for, which is a property of its size and of the derivation in [KEY](key-schedule.md).

## Borrowed terms

| term | meaning |
| --- | --- |
| TPM | A Trusted Platform Module: a coprocessor that holds keys and performs cryptographic operations, present either as a discrete chip or in firmware. |
| endorsement seed | A secret a TPM holds from manufacture, from which it generates its Endorsement Keys. |
| Endorsement Key | The key a TPM generates from that seed under a template fixing its algorithm and parameters. |
| Endorsement Key name | That key's identifier: its hash algorithm identifier followed by the digest of its public area. |
| one-time-programmable memory | Memory on a board that can be written once and not rewritten, part of which a board may reserve for its customer. |
| device-tree serial | The serial number a Raspberry Pi's firmware publishes to the operating system through the device tree. |

## Choosing a source

The precedence, strongest first, is the name of a TPM 2.0 Endorsement Key, then written customer one-time-programmable memory, then the platform serial number.

A board ID MUST be the value of the single strongest source present, and MUST NOT combine sources.

Precedence MUST be evaluated by kind of source, never by platform.

> [!NOTE]
> Combining sources would give each one its own way to change the board ID, and a board ID that changes orphans a QR code already fixed to an enclosure.
> Evaluating by kind rather than by platform means a board gains a stronger source by having the hardware for it, with no rule naming a model, and a device and a generator reach the same answer without either being told what machine it runs on.

### TPM Endorsement Key

Where a TPM 2.0 is present, the board ID MUST be the name of its Endorsement Key.

That key MUST be the one generated from the endorsement seed under the TCG low-range RSA 2048 template, and MUST be regenerated from the seed and the template rather than read from a persisted copy.

> [!NOTE]
> A TPM holds one Endorsement Key per algorithm, so the algorithm is part of the derivation: changing it re-derives every board ID taken under the old one.
> A persisted copy is not guaranteed to exist on a freshly imaged machine, while the seed and the template always are.

### Customer one-time-programmable memory

Where a board carries customer-programmable one-time-programmable memory that has been written, the board ID MUST be its contents.

### Platform serial number

Otherwise the board ID MUST be the platform serial number, which on Raspberry Pi hardware is the device-tree serial.

## Probing and reading

A device MUST establish which sources are present by probing each, and MUST read a value only from the source that wins the precedence.

> [!NOTE]
> Probing is cheap: a device node exists or it does not, one-time-programmable memory reads as written or as blank, a serial is readable or absent.
> Reading is not uniformly cheap. An Endorsement Key name is obtained by regenerating the key inside the TPM, which is paid every time the value is wanted.

## Sources that carry no identity

A source whose value reads as all zeros, as all ones, or as a known vendor constant MUST be treated as absent whatever the value's nominal width, and the precedence MUST fall through to the next source.

Reaching the end of the precedence with no usable source MUST be reported as [DEV](device.md) requires, rather than derived past.

> [!NOTE]
> Such a value is a placeholder, and deriving from it would give every board in the same position the same token.
> Unwritten one-time-programmable memory is the ordinary case, reading as zeros on every unprogrammed board.

## When the board ID changes

Fitting hardware that carries a stronger source changes which source wins, and so changes the board ID and every value derived from it.

Where a board's platform serial is unchanged but its strongest present source is stronger than the one it last derived from, its QR code is dead, and the device MUST report that as [DEV](device.md) requires rather than advertise a handle no client can match.

Where a board's platform serial differs, it is a different board, and the device MUST derive from the board it now sits on without reporting a fault.

Where a board offers no platform serial, any change in its board ID MUST be reported.

A source that will supersede a platform serial MUST be written before that board's QR code is derived.

> [!NOTE]
> The platform serial is the last tier of the precedence, so it is present whichever source wins, and it does not itself change when stronger hardware is fitted. That is what makes it able to identify a board across a change of source.
> A differing serial is the case of a disk moved from one enclosure into another, where the board matches the QR code already fixed to its new enclosure.
> Recovering from a dead code means printing a new one for that board.

## Hardware in scope

A board ID is derived on physical machines.
