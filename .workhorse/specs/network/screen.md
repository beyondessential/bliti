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

> [!NOTE]
> An application that proposed as the operator typed would hand the device a gateway half entered, and the device would fail it for a reason that is not real.
> Filling the errored stage from what was proposed rather than from what the device returned to is what lets an operator correct the one field that was wrong. The device keeps no record of the attempt, as [CFG](session.md) requires, so the application is the only end that can offer it back.
> A field edited part way through verification belongs to neither the attempt being verified nor the next one.

## Validating before proposing

The application MUST check a document against the capabilities the device reported before proposing it.

The application MUST NOT offer a setting the device did not report supporting.

The application MUST say why a setting is absent where the device's capabilities exclude it.

> [!NOTE]
> A device that cannot give its hotspot a channel of its own has no channel field, and an operator who goes looking for one is better served by a sentence than by an absence they have to work out.

## Rendering the ordering

The application MUST render the attachment ordering of [LINK](attachment.md) as a list the operator can reorder.

The application MUST show each candidate's state against it, distinguishing the candidate carrying the default route, one that is up, and each way a candidate is unavailable.

The application MUST describe an unavailable candidate by what the device observed of it.

> [!NOTE]
> "No gateway", "no lease" and "out of range" are the same facts as "not here" and "not offered" with the diagnosis left in. An operator reads the first three and knows what to change.

## Scanning

The application MUST list what a scan heard by SSID, showing for each network the strongest signal among its access points and how many there are, with the access points behind it on request.

The application MUST leave out access points with no SSID unless the operator asks to see hidden networks.

The application MUST offer, from a scan, a view for siting an access point: every access point heard, by signal, with its channel and the adapter that heard it, and the channels taken on each band the device's radios can use, counting every channel a wide access point spans.

The application MUST let the operator scan or survey one adapter rather than every one, and join by WPS on a chosen adapter or on one the device picks.

> [!NOTE]
> Joining asks which network to add. Siting asks where the device's signal comes from and which channel a new access point should take, and it is the same scan read differently.
> Scanning takes a radio off its channel for a moment, so scanning one adapter spares a radio carrying the uplink or the hotspot.

## Rendering a failure

The application MUST mark the field named by the failure's `at`.

The application MUST render the verification stages of [LINK](attachment.md), showing which the proposal passed and which it failed.

The application MUST render the failure's `reason` as the device wrote it.

The application MUST offer, on the candidate a failure's `at` names, to propose the document again with that candidate's `verify` false.

The application MUST give a candidate the operator adds `verify` true, MUST show which candidates carry `verify` false, and MUST let the operator turn verification back on for one while editing.

> [!NOTE]
> The stages say the addressing was fine and the network is not routing, without a sentence having to say so.
> The reason is the device's own words about something the application did not anticipate, so there is no wording of its own to supply.
> Offering to skip verification only on the candidate that failed keeps it for the operator who has seen why the device refused and knows better, as with a network that is not up yet, and leaves every other candidate held to it.

## The state of the session

The application MUST show whether what the device is running has been made durable, and MUST keep that in view while the operator scrolls.

The application MUST say what ending the session would cost while a proposal is unconfirmed.

> [!NOTE]
> A proposal that has been applied is invisible otherwise: the device is working, and nothing about it has been written down.

## Wording

The application MUST supply its own wording for everything it renders, and MUST NOT put the vocabulary of [CFG](session.md) in front of an operator.

The application MUST name a setting by the term a technician configuring a network would use, except where a plainer term is the one they would look for.

> [!NOTE]
> SSID, passphrase and DHCP range say what the thing is; name, password and addresses handed out are vaguer, and vagueness is what costs someone a second visit to a site.
> Country is the exception that shows the limit: someone hunting for the regulatory domain setting looks for the country, so the more correct term would be the harder to find. Precision gives way to discoverability, and to nothing else.
