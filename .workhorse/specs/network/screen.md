---
id: NSCR
---

# Network configuration screen

The screen an operator configures a device from: the document of [NET](overview.md), edited through the session of [CFG](session.md), by our application.

[NET](overview.md) and [CFG](session.md) bind every implementation.
This spec says what ours makes of them, and binds nothing else that configures a bliti device.

## Editing is the application's own

The application MUST hold the operator's edits itself, and MUST NOT propose a document until the operator asks it to.

The application MUST move through four stages:

| stage | fields | offers |
| --- | --- | --- |
| editing | writable | apply, reset |
| applying | read-only | cancel |
| errored | writable | apply, reset |
| applied | read-only | confirm, cancel |

The application MUST enter editing when a session opens, and MUST return to it on reset.

The application MUST fill the fields on entering editing with the configuration in force, and on entering errored with the document it proposed.

The application MUST NOT let the operator edit a field while a proposal is being verified.

The application MUST show that it is waiting on the device while a proposal is being verified, a scan or survey is running, or a session is opening.

The application MUST keep the configuration session open, and the operator's edits with it, when the operator leaves the screen with edits not applied, a proposal being applied, or one applied.
While it keeps a session open this way, the application MUST say so wherever the operator is, and MUST offer there to return to the screen, and to confirm or discard an applied proposal or discard the edits.
Where a proposal kept open this way fails, the application MUST keep what it proposed until the operator returns to the screen or discards it.
The application MUST close the session when the operator leaves the screen with nothing changed, and once nothing is left to apply, confirm or discard.

> [!NOTE]
> An application that proposed as the operator typed would hand the device a gateway half entered, and the device would fail it for a reason that is not real.
> Filling the errored stage from what was proposed rather than from what the device returned to is what lets an operator correct the one field that was wrong. The device keeps no record of the attempt, as [CFG](session.md) requires, so the application is the only end that can offer it back.
> A field edited part way through verification belongs to neither the attempt being verified nor the next one.

## Validating before proposing

The application MUST check a document against the capabilities the device reported before proposing it.

The application MUST NOT offer a setting the device did not report supporting.

The application MUST say why a setting is absent where the device's capabilities exclude it, except that where the device offers none of the hotspot's band, channel and width on any of its adapters, the application MUST leave all three out without comment.

The application MUST offer the hotspot's band, channel and width on a shared-channel adapter only while no wireless candidate could be carried by that adapter, as [HOT](hotspot.md) has it.
Once one could, the application MUST take those settings out of the hotspot, and MUST say in their place that the hotspot runs on the channel of that wireless connection.

> [!NOTE]
> A setting absent from among others the device does offer is one an operator goes looking for, and is better served by a sentence than by an absence they have to work out.
> A device offering no radio setting for its hotspot at all leaves nothing to look for: the radio is not the operator's to tune there, and a sentence about it is noise.

## Rendering the ordering

The application MUST render the attachment ordering of [LINK](attachment.md) as a list the operator can reorder.

The application MUST say beside the list, in terms of what the device does, that it tries the candidates from the top, uses the first that connects, and moves to another when that changes.

The application MUST make plain which candidate the fields being edited belong to.

The application MUST let the operator turn each candidate off and on again while editing, keeping its fields as they are and editable, MUST give a candidate the operator adds `enabled` true, and MUST show in the list which candidates are off.

The application MUST let the operator turn the hotspot off and on again while editing, keeping its settings as they are and editable, and MUST give a hotspot the operator adds `enabled` true.

The application MUST say, beside a hotspot that is on, where it cannot run beside a wireless connection the device reports joined now, naming the connection and its channel and that turning the connection off lets it run, and MUST still let the operator apply.

The application MUST show each candidate's state against it, distinguishing the candidate carrying the default route, one that is up, one that is off, and each way a candidate is unavailable.

The application MUST describe an unavailable candidate by what the device observed of it.

> [!NOTE]
> "No gateway", "no lease" and "out of range" are the same facts as "not here" and "not offered" with the diagnosis left in. An operator reads the first three and knows what to change.

## Scanning

The application MUST list what a scan heard by SSID, showing for each network the strongest signal among its access points and how many there are, with the access points behind it on request.

The application MUST leave out access points with no SSID unless the operator asks to see hidden networks.

The application MUST NOT let the operator change whether a network is hidden where a scan has settled it: one the last scan heard by its name is not hidden, and one picked from an access point with no SSID is.

The application MUST offer, from a scan, a view for siting an access point: every access point heard, by signal, with its channel and the adapter that heard it, and the channels taken on each band the device's radios can use, counting every channel a wide access point spans.

The application MUST let the operator scan or survey one adapter rather than every one, and join by WPS on a chosen adapter or on one the device picks.

The application MUST offer to join by WPS from a network the scan lists by SSID, asking the device to join that network alone.

> [!NOTE]
> Joining asks which network to add. Siting asks where the device's signal comes from and which channel a new access point should take, and it is the same scan read differently.
> Scanning takes a radio off its channel for a moment, so scanning one adapter spares a radio carrying the uplink or the hotspot.

## Rendering a failure

The application MUST mark the field named by the failure's `at`.

The application MUST render the verification stages of [LINK](attachment.md), showing which the proposal passed and which it failed.

The application MUST render the failure's `reason` as the device wrote it.

The application MUST offer, on the candidate a failure's `at` names or names a member of, to propose the document again with that candidate's `verify` false.

The application MUST give a candidate the operator adds `verify` true, MUST show which candidates carry `verify` false, and MUST let the operator turn verification back on for one while editing.

> [!NOTE]
> The stages say the addressing was fine and the network is not routing, without a sentence having to say so.
> The reason is the device's own words about something the application did not anticipate, so there is no wording of its own to supply.
> Offering to skip verification only on the candidate that failed keeps it for the operator who has seen why the device refused and knows better, as with a network that is not up yet, and leaves every other candidate held to it.

## The state of the session

The application MUST show whether what the device is running has been made durable, and MUST keep that in view while the operator scrolls.

The application MUST say what ending the session would cost while a proposal is unconfirmed.

The application MUST say where the session has ended and offer to open it again, and where it cannot be opened again, MUST say so and offer to disconnect from the device so the operator can connect again.

> [!NOTE]
> A proposal that has been applied is invisible otherwise: the device is working, and nothing about it has been written down.

## Wording

The application MUST supply its own wording for everything it renders, and MUST NOT put the vocabulary of [CFG](session.md) in front of an operator.

The application MUST name a setting by the term a technician configuring a network would use, except where a plainer term is the one they would look for.

> [!NOTE]
> SSID, passphrase and DHCP range say what the thing is; name, password and addresses handed out are vaguer, and vagueness is what costs someone a second visit to a site.
> Country is the exception that shows the limit: someone hunting for the regulatory domain setting looks for the country, so the more correct term would be the harder to find. Precision gives way to discoverability, and to nothing else.
