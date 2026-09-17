---
id: SEC
---

# Security properties

The [presence token](overview.md#presence-token) is the only credential in the system.

## What is guaranteed

### An operator can pick out their device

An operator who has scanned a device's QR code can identify that device among every device advertising nearby.

Upheld by the handle derivation of [KEY](key-schedule.md) and the matching of [ADV](discovery.md).

### The link reveals neither the secret nor the session

A listener on the BLE link learns neither the presence token nor the contents of a session.

Upheld by the handshake of [CHN](channel.md), which sends the secret in neither direction and encrypts everything after itself.

### A recorded advertisement is not a session

An attacker who records or replays an advertisement obtains no session from it, because completing the handshake requires the presence token.

Upheld by [CHN](channel.md).

### A recovered secret does not open past sessions

Recovering a presence token does not decrypt a session recorded before it was recovered.

Upheld by the forward secrecy of the handshake in [CHN](channel.md).

### The QR code does not reveal the board ID

Reading the QR code does not yield the [board ID](overview.md#board-id), and does not yield it even to someone who knows the derivation constants.
The board ID appears in no QR payload, in no advertisement, and nowhere else reachable without access to the device.

Upheld by the derivation of [KEY](key-schedule.md), which does not run backwards, and by the payload of [QR](qr-code.md).

### An observer cannot tell which device it hears

An observer who has not scanned a device's QR code cannot tell which device an advertisement belongs to.

Upheld by the handle derivation of [KEY](key-schedule.md) and the rotation of [ADV](discovery.md).

### A photograph does not permit impersonation

Someone holding a device's QR code, or a photograph of one, cannot complete a handshake as that device.

Upheld by the device static key of [KEY](key-schedule.md), whose private half descends from the board ID and appears in no QR code, and by the handshake of [CHN](channel.md), which authenticates the device against it.

### Compromising one device tells nothing about another

There is no fleet key and no authoritative per-device record.
Each device's presence token derives from its own board ID alone, so recovering one device's secret, or its board ID, yields nothing about any other device.

Upheld by the derivation of [KEY](key-schedule.md).

## Where the guarantees stop

### Holding the token opens a session

Anyone holding a device's presence token can open a session with that device.

A photograph of the QR code yields the token, because the code carries it outright.

### The board ID yields everything

Anyone who learns a device's board ID can derive that device's root, and so both its presence token and its device static key.

Any software on a device can read its board ID, so anyone who has had access to a device can obtain it.

> [!NOTE]
> Physical access therefore still permits impersonation. What the static key closes is the path that needs no access at all.

### A device's presence is not hidden

An observer who has not scanned the QR code can still tell that some bliti device is present, because the service UUID is advertised in the clear so that clients can filter a scan on it.

Such an observer can also tell that two advertisements come from the same device whenever the adapter's address does not rotate, which is a property of how the host is configured rather than something bliti controls.

bliti claims only that such an observer cannot tell *which* device it is hearing.

### A board ID can be searched for

Search here means exhaustive enumeration, not consulting a record: an attacker derives the advertised handle for each candidate board ID in turn and compares it against a handle observed on the air.

Because the derivation constants are public, that attack needs neither the device's QR code nor physical access to it.
A recorded handshake serves as well as a recorded advertisement does, because either lets a candidate be tested offline.

What stands against it is the cost of one derivation multiplied by the size of the board ID's space, both specified in [KEY](key-schedule.md).

Where a board ID comes from a TPM Endorsement Key or from written one-time-programmable memory, that space is large enough that the derivation cost is not what holds the scheme up.

Where it comes from a platform serial, the cost is what the guarantee rests on, and it does not make every such board safe.
A Raspberry Pi 4 or 5 serial occupies its full width and is out of reach.
A serial that collapses to a short value, as on earlier boards, is small enough to be searched by an adversary willing to spend on it, and no parameters tolerable on a provisioning path change that.

### Compressed sizes carry a signal about content

The channel compresses what it carries, as [CHN](channel.md) specifies, so what crosses the link varies with a message's content and not with its length alone.

An observer learns nothing of what is said, and something of how much of a message the compression context had already seen.

> [!NOTE]
> The attacks that recover a secret from compressed sizes need input an attacker chooses to share a context with the secret. Neither direction offers that: the presence token is never sent, and each end compresses only what it chose to say.

### What a handshake proves

A handshake proves that the device holds a static key derived from its own board ID, and that the client holds that device's presence token.

A board ID is not verified directly and cannot be, because it is absent from the QR payload and the derivation does not run backwards.
