---
id: CFG
---

# Configuration session

A client reads and changes a device's network configuration through a session: one stream on which the device reports the configuration it holds and how it is faring, and on which the client proposes another.

The session is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## The exchange

A client MUST open a configuration session by opening a stream whose first message is `configure`.

A device MUST answer `configure` with `configuration`, carrying the configuration it holds and the capabilities of [NET](overview.md), then with `state`.

A device MUST send `configuration` on every open session whenever the configuration it holds changes, and on the session that proposed it after every proposal it refuses.

A device MUST send `state` after every `configuration` it sends, and again whenever the outcome of the configuration it holds, the state of one of its candidates, its capabilities, or whether the session is busy change.

A client MUST propose a change by sending `configuration` carrying the document it wants the device to hold.

A client MAY propose again at any time.

A device MUST carry `capabilities` on the next `configuration` or `state` it sends whenever they differ from those it last sent in the session, as a new regulatory domain changes the usable channels, whether a proposal or an access point joined under [NET](overview.md) brought that domain into force.

A client MUST check its next proposal against the capabilities it was last sent.

A device MUST serve a configuration session on every stream that asks for one, on any of its channels.

The session a device has served longest among those open MUST hold the configuration, and every other MUST be busy.

A device MUST answer a proposal from a busy session with `cannot-configure` at `$`, and an act from one with `cannot-act` at `$`.

> [!NOTE]
> The session reports the device rather than pairing requests with answers. A client that opens one is told what a client that never left would know by then, so leaving part way through a change and coming back loses nothing.
> The channel already offers a bidirectional stream, so nothing is gained by pretending otherwise.
> Two operators able to configure one device at once could undo each other silently. Letting one hold the configuration while others watch it change keeps every operator's view current without either undoing the other. Both are standing at the device, so they can settle between themselves who holds it.

## Message types

| type | sent by | carries |
| --- | --- | --- |
| `configure` | client | nothing beyond `type` |
| `configuration` | either end | `document`, and from a device `capabilities` on the first and where they changed |
| `state` | device | `busy`, `outcome`, `failure` where the outcome is `failed`, `attachments`, and `capabilities` where they changed |
| `cannot-configure` | device | as [MSG](../messages.md) has it |
| `cannot-act` | device | as [MSG](../messages.md) has it |

`configuration` MUST carry `DOCUMENT` as a critical member, so that an end which cannot read the document does not act on the message carrying it.

> [!NOTE]
> A device acting on a partial reading of a configuration would apply a configuration other than the one asked for, which is the case [MSG](../messages.md) reserves critical members for.

## The configuration a device holds

A device MUST hold exactly one configuration.

A device that has never saved a proposal MUST hold one `wired-dynamic` candidate for each wired interface, labelled by the interface and carrying `enabled` and `verify` true, and no hotspot.

A device MUST refuse a proposal asking for anything it did not state under [NET](overview.md), and one [LINK](attachment.md), [WLAN](wireless.md) or [HOT](hotspot.md) requires it to treat as invalid or to refuse.

A device refusing a proposal MUST send `cannot-configure` of [MSG](../messages.md), then `configuration` carrying the configuration it holds, unchanged.

A device MUST otherwise save the proposal: record it as the configuration it holds before applying any of it, send `configuration` carrying it, then apply it.

The `configuration` a device sends on saving a proposal MUST carry the proposed document unchanged.

A client MUST take a proposal as saved where the next `configuration` the device sends after it carries a document equal to it, member for member and value for value, and as not saved otherwise.

A device MUST run the configuration it holds from when it saves it, and from when it starts.

A loss of power while a device records a configuration MUST leave it holding either the configuration it held or the one it was recording, whole.

A session ending MUST NOT change the configuration a device holds or runs, nor interrupt its verification.

> [!NOTE]
> Sending the configuration back is the device saying it saved the proposal, so a client needs no answer of its own for that: it compares what it sent with what came back.
> A network change cannot break the channel the operator holds, so a device has no reason to hold a change at arm's length until someone vouches for it: the operator who applied it is still connected and can apply another.
> Keeping a configuration that fails verification keeps the operator's work. A network that is not up yet, or out of range for now, is selected under [LINK](attachment.md) once it can be, with nobody needing to come back and apply it again.

## Verifying it

A device MUST verify each configuration it saves, judging it on the candidates it adds or changes against the configuration it replaces that carry `enabled` and `verify` true, and on no others.

A candidate differing from one of the configuration it replaces only in `enabled` is one the proposal changes.

The outcome of the configuration a device holds MUST be `verifying` from when it is saved until its verification ends, then `applied` or `failed`.

A device MUST make the outcome `applied` where, on every interface carrying a candidate it is judged on, some candidate is established.

A device MUST otherwise make it `failed`, with the failure at the candidate, among those it is judged on that sit on an interface where none is established, that passed the most stages of [LINK](attachment.md), the first in the ordering among equals.

Where the device can tell which member of that candidate is at fault, the failure's `at` MUST name that member rather than the candidate: the `passphrase` of a key-based network whose access point it still hears refusing the join.

A device MUST make the outcome `failed` where it cannot run some part of the configuration it saved, whatever its candidates carry.

A device MUST verify and select a candidate carrying `verify` false as it does any other, and report it through `state`.

A device saving a proposal while it verifies another MUST abandon that verification.

The configuration a device holds before it has saved any proposal is judged on no candidate.

A device MUST keep the outcome of the configuration it holds until it saves another, across restarts, and MUST verify again, on the same candidates, a configuration whose verification a restart interrupted.

> [!NOTE]
> Judging each interface rather than each candidate lets one document carry a static candidate for each of two sites on one port: at either site the other's fails, and the port is still established.
> A candidate the proposal leaves as it was has already been judged, so a network that is out of range today does not stop an operator changing something else.
> A candidate that is not verified is for the operator who knows better than the device, as with a network that is not up yet. It only stops that candidate failing the change: the device still brings it up only as far as it can observe.
> The outcome is the verdict on the change and stands until the next one, while `attachments` report each candidate as it is now: a network out of range when it was applied can come up later without the failure being forgotten.

## What a failure says

A failure MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `at` | string | yes | which part is at fault |
| `reason` | string | yes | the device's account of the failure, in its own words |
| `reached` | string | where a candidate's verification stopped | the verification stage of [LINK](attachment.md) at which the attempt stopped |

A failure's `at` MUST be an RFC 9535 Normalized Path, rooted at the configuration the device holds.

`reached` MUST name a stage of [LINK](attachment.md) by its wire name, every stage before it having passed, and MUST be absent where the failure is not a candidate's verification, as a hotspot that does not start.

> [!NOTE]
> The three do different work. `at` puts an operator's cursor in the field that was wrong, `reached` says how far the attempt got, and `reason` carries the part nobody anticipated: a path, a permission, an errno.
> "Associated, then the gateway did not answer" corrects one field and establishes that the key was right. A single code for failure would carry only the half that was foreseen.

## The state of each candidate

`state` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `busy` | boolean | yes | whether another session holds the configuration |
| `outcome` | string | yes | `verifying`, `applied` or `failed`, for the configuration the device holds |
| `failure` | object | where `outcome` is `failed` | the failure, as [What a failure says](#what-a-failure-says) has it |
| `attachments` | array | yes | the state of each candidate, matching position for position the attachments of the configuration the device holds |

Each entry of `attachments` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `is` | string | yes | `default-route`, `up`, `verifying`, `standby`, `off` or `unavailable` |
| `reached` | string | where `is` is `unavailable` | the stage of [LINK](attachment.md) at which it stopped, as a failure carries it |
| `reason` | string | where `is` is `unavailable` | the device's observation, in its own words |

`standby` MUST mean a candidate not tried because every interface it could be brought up on carries a candidate above it, or is kept for the hotspot.

`off` MUST mean a candidate whose `enabled` is false.

> [!NOTE]
> The stage gives the device's observation of a candidate that is not up, and a client words it: stopping at `addressing` is no lease, and a wireless candidate stopping at `carrier` is out of range.

## Acting now

A client MAY ask a device to act rather than to hold a setting, by sending on the session stream:

| type | sent by | carries | asks the device to |
| --- | --- | --- | --- |
| `scan` | client | `interface` where the client names one wireless interface | report the wireless networks it can see |
| `survey` | client | `interface` where the client names one wireless interface | report the occupied and usable spectrum its radios can see |

A device MUST answer `scan` with `networks`, and `survey` with `spectrum`.

A device MUST answer an act it cannot perform with `cannot-act` of [MSG](../messages.md), naming as `act` the act's message type.

`networks` MUST carry `access-points`, from a fresh scan, one entry for each access point each radio scanned heard, leaving out the device's own hotspot:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `interface` | string | yes | the wireless interface whose radio heard it |
| `bssid` | string | yes | the access point's radio address, lower case and colon-separated |
| `ssid` | string or null | yes | the network's name, null where the access point hides it and the device does not know it |
| `hidden` | boolean | yes | whether the access point leaves its name out of its beacons |
| `security` | array | yes | the security it advertises, from `psk`, `sae`, `enterprise`, `open`, `owe` and `wep` |
| `band` | string | yes | its band, as [HOT](hotspot.md) names bands |
| `channel` | number | yes | its channel |
| `channel-width` | number | yes | the width it occupies, in megahertz |
| `secondary-channel` | number | no | its secondary 20 MHz channel, where it occupies more than 20 MHz and names which |
| `signal` | number | yes | how strongly the radio hears it, in dBm |
| `country` | string | no | the country its Country element names, as the two letters it sends |
| `environment` | string | no | where its Country element says it operates: `indoor`, `outdoor` or `any` |

A device MUST leave `country` out where the access point sends no Country element, or one whose country is not two ASCII letters, and MUST send the letters upper case.

A device MUST leave `environment` out where it leaves `country` out, and where the Country element names an operating class table in place of an environment.

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

