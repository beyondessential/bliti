---
id: CFG
---

# Configuration session

A client reads and changes a device's network configuration through a session: one stream, carrying the configuration in force, each change, what became of it, and the confirmation.

The session is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## The exchange

A client MUST open a configuration session by opening a stream whose first message is `configure`.

A device MUST answer `configure` with `configuration`, carrying the configuration in force and the capabilities of [NET](overview.md).

A client MUST propose a change by sending `configuration` carrying the document it wants in force.

A device MUST answer a proposal with `applied` or with `invalid`.

A client MUST make a proposal durable by sending `confirm`, which a device MUST answer with `configuration` carrying what is now in force.

A client MUST abandon a proposal by sending `discard`.

A client MAY propose again on the same stream after any answer.

A device MUST answer every proposal exactly once, and MUST answer one interrupted by `discard` or by a newer proposal before its answer with `invalid` at `$`, carrying no `reached`.

A device MUST NOT answer `discard`.

A device MUST carry `capabilities` on `applied` where applying the proposal changed them, as a new regulatory domain changes the usable channels, and on the `state` it sends next where returning to its recorded configuration changed them back.

A client MUST check what it proposes next against the capabilities it was last sent.

A device MUST serve at most one configuration session at a time, and MUST answer `configure` on a second stream with `busy` while one is open.

> [!NOTE]
> A configuration is a conversation with state rather than a request and its answer, which is why it holds one stream open rather than pairing messages. The channel already offers a bidirectional stream, so nothing is gained by pretending otherwise.
> Two operators able to configure one device at once could undo each other silently. Both are standing at the device, so telling the second that the device is busy is enough for them to resolve it between themselves.

## Message types

| type | sent by | carries |
| --- | --- | --- |
| `configure` | client | nothing beyond `type` |
| `configuration` | either end | `document`, and `capabilities` on the first from a device |
| `state` | device | `attachments`, and `capabilities` where returning to the recorded configuration changed them |
| `applied` | device | `capabilities` where the proposal changed them |
| `invalid` | device | `at`, `reason`, and `reached` where a proposal was applied and then failed |
| `confirm` | client | nothing beyond `type` |
| `discard` | client | nothing beyond `type` |
| `busy` | device | nothing beyond `type` |

`configuration` MUST carry `DOCUMENT` as a critical member, so that an end which cannot read the document does not act on the message carrying it.

> [!NOTE]
> A device acting on a partial reading of a configuration would apply something other than what was asked for, which is the case [MSG](../messages.md) reserves critical members for.

## What a failure says

`invalid` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `at` | string | yes | which part is at fault |
| `reason` | string | yes | what happened, in the device's own words |
| `reached` | string | no | the verification stage of [LINK](attachment.md) at which the attempt stopped |

`at` MUST be an RFC 9535 Normalized Path, rooted at the proposed document for a proposal and at the message asking for an act for an act.

`reached` MUST name a stage of [LINK](attachment.md) by its wire name, every stage before it having passed, and MUST be absent only where nothing was applied.

> [!NOTE]
> The three do different work. `at` puts an operator's cursor in the field that was wrong, `reached` says how far the attempt got, and `reason` carries the part nobody anticipated: a path, a permission, an errno.
> "Associated, then the gateway did not answer" corrects one field and establishes that the key was right. A single code for failure would carry only the half that was foreseen.

## The state of each candidate

A device MUST send `state` after the first `configuration` it sends in a session, and again whenever the state of a candidate changes.

A device MUST NOT send `state` while a proposal is being verified, and MUST send it once the proposal is answered.

`state` MUST carry `attachments`, an array matching position for position the attachments of the configuration running, which is a proposal once one is applied, each entry carrying:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `is` | string | yes | `default-route`, `up`, `verifying`, `standby` or `unavailable` |
| `reached` | string | where `is` is `unavailable` | the stage of [LINK](attachment.md) at which it stopped, as `invalid` carries it |
| `reason` | string | where `is` is `unavailable` | what the device observed, in its own words |

`standby` MUST mean a candidate not tried because every interface it could be brought up on carries a candidate above it, or is kept for the hotspot.

> [!NOTE]
> The stage says what the device observed of a candidate that is not up, and a client words it: stopping at `addressing` is no lease, and a wireless candidate stopping at `carrier` is out of range.

## Provisional and confirmed

A device that has never recorded a configuration MUST hold as its recorded configuration one `wired-dynamic` candidate for each wired interface, labelled by the interface, and no hotspot.

A device MUST apply a proposal to its running system without recording it.

A device MUST hold exactly one recorded configuration, and MUST replace it only on `confirm`.

A device MUST return to its recorded configuration when a session ends, however it ends.

A device MUST return to its recorded configuration on `discard`, whether the proposal is still being verified or already applied.

A device MUST NOT impose a deadline on a proposal.

A device MUST NOT retain what a proposal contained after returning to its recorded configuration.

> [!NOTE]
> Never recording a proposal is what makes every way a session can end (a discard, the stream closing, the channel dropping, a power cut, a reboot) restore the recorded configuration as a consequence rather than as a rule each has to implement separately.
> A network change cannot break the channel the operator holds, so nothing is racing a deadline, and a deadline would only take a working configuration away from an operator who was still looking at it.
> The client holds what it proposed, so a device that keeps nothing costs nobody the ability to correct a failed change. It is also what keeps a device from walking into a corner one unconfirmed step at a time.

## Acting now

A client MAY ask a device to act rather than to hold a setting, by sending on the session stream:

| type | sent by | carries | asks the device to |
| --- | --- | --- | --- |
| `scan` | client | `interface` where the client names one wireless interface | report the wireless networks it can see |
| `survey` | client | `interface` where the client names one wireless interface | report what its radios can see of the occupied and usable spectrum |
| `wps` | client | `method`, and `interface` where the client names the wireless interface to join on | join by WPS, as [WLAN](wireless.md) specifies |

A device MUST answer `scan` with `networks`, and `survey` with `spectrum`.

A device MUST answer `wps` as it answers a proposal: with `configuration` carrying the configuration in force with the joined network added first among its attachments, then with `applied` or `invalid`, after which it is a proposal like any other.

A device joining by PIN MUST first send `pin`, carrying as `pin` the PIN it generated for the operator to enter at the access point.

`networks` MUST carry `access-points`, from a fresh scan, one entry for each access point each radio scanned heard, leaving out the device's own hotspot:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `interface` | string | yes | the wireless interface whose radio heard it |
| `bssid` | string | yes | the access point's radio address, lower case and colon-separated |
| `ssid` | string or null | yes | the network's name, null where the access point hides it and the device does not know it |
| `hidden` | boolean | yes | whether the access point leaves its name out of its beacons |
| `security` | array | yes | what it advertises, from `psk`, `sae`, `enterprise`, `open`, `owe` and `wep` |
| `band` | string | yes | its band, as [HOT](hotspot.md) names bands |
| `channel` | number | yes | its channel |
| `channel-width` | number | yes | the width it occupies, in megahertz |
| `signal` | number | yes | how strongly the radio hears it, in dBm |

A device MUST answer a `scan` or `survey` naming an `interface` for that interface's radio alone, and one naming none for every radio able to.

A device MUST state among its capabilities which of its radios can scan and which can survey, and MUST omit `survey` where none can.

`spectrum` MUST carry a `spectrum` object whose `channels` member is an array with one entry for each channel usable under the regulatory domain in force, on each radio surveyed:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `interface` | string | yes | the wireless interface whose radio heard it |
| `band` | string | yes | the band, as [HOT](hotspot.md) names bands |
| `channel` | number | yes | the channel |
| `networks` | number | yes | how many access points the radio heard on it |
| `busy` | number | yes | the fraction of time the radio found it occupied, from 0 to 1 |

> [!NOTE]
> These are acts rather than settings: a document describes what is to be true, and none of these is a state a device could be left in.
