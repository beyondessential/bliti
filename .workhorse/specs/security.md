---
id: SEC
---

# Security properties

The [presence token](overview.md#presence-token) is the only credential in the system.
Reading the QR code on a device is what yields it, and holding it is what authenticates.

Each property below names the mechanism that upholds it. The requirements are in those specs; what is stated here is what they add up to.

## What is guaranteed

### An operator can pick out their device

An operator who has scanned a device's QR code can identify that device among every device advertising nearby.

Upheld by the handle derivation of [BLI-KEY](key-schedule.md) and the matching of [BLI-ADV](discovery.md).

### The link reveals neither the secret nor the session

A listener on the BLE link learns neither the presence token nor the contents of a session.

Upheld by the handshake of [BLI-CHN](channel.md), which sends the secret in neither direction and encrypts everything after itself.

### A recorded advertisement is not a session

An attacker who records or replays an advertisement obtains no session from it, because completing the handshake requires the presence token.

Upheld by [BLI-CHN](channel.md).

### A recovered secret does not open past sessions

Recovering a presence token does not decrypt a session recorded before it was recovered.

Upheld by the forward secrecy of the handshake in [BLI-CHN](channel.md).

### The QR code does not reveal the board ID

Reading the QR code does not yield the [board ID](overview.md#board-id), and does not yield it even to someone who knows the derivation constants.
The board ID appears in no QR payload, in no advertisement, and nowhere else reachable without access to the device.

Upheld by the derivation of [BLI-KEY](key-schedule.md), which does not run backwards, and by the payload of [BLI-STK](sticker.md).

### An observer cannot tell which device it hears

An observer who has not scanned a device's QR code cannot tell which device an advertisement belongs to.

Upheld by the handle derivation of [BLI-KEY](key-schedule.md) and the rotation of [BLI-ADV](discovery.md).

### Compromising one device tells nothing about another

There is no fleet key and no authoritative per-device record.
Each device's presence token derives from its own board ID alone, so recovering one device's secret, or its board ID, yields nothing about any other device.

Upheld by the derivation of [BLI-KEY](key-schedule.md).

## Where the guarantees stop

### The QR code is the credential

Anyone who has had access to a device, or who otherwise knows its board ID, can derive its presence token, and can then both impersonate the device and connect to it.
The same is true of anyone holding a photograph of the QR code.

### A device's presence is not hidden

An observer who has not scanned the QR code can still tell that some bliti device is present, because the service UUID is advertised in the clear so that clients can filter a scan on it.

Such an observer can also tell that two advertisements come from the same device whenever the adapter's address does not rotate, which is a property of how the host is configured rather than something bliti controls.

bliti claims only that such an observer cannot tell *which* device it is hearing.

### A board ID can be searched for

Because the derivation constants are public, finding a device requires neither its QR code nor physical access to it, only its board ID, and board IDs can be searched for rather than known.

A recorded handshake serves that search as well as a recorded advertisement does, because either lets a guess be tested offline.

What stands against the search is the cost of the derivation and the size of the board ID's space, both specified in [BLI-KEY](key-schedule.md).
For boards whose only identifier is a short serial number that margin is narrow, and those boards carry a weaker guarantee than boards with a hardware-backed identifier.

### Authentication proves the secret, not the board

A device's board ID is not verified directly and cannot be, because it is absent from the QR payload and the derivation does not run backwards.

Possession of the presence token is what a handshake proves, and it is equivalent for the purpose, because deriving the secret requires the board ID.
