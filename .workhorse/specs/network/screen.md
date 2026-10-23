---
id: NSCR
---

# Network configuration screen

The screen an operator configures a device from: the document of [NET](overview.md), edited through the session of [CFG](session.md), by our application.

[NET](overview.md) and [CFG](session.md) bind every implementation.
This spec covers how ours handles them, and binds nothing else that configures a bliti device.

## Editing is the application's own

The application MUST hold the operator's edits itself, and MUST NOT propose a document until the operator asks it to.

The application MUST move through three stages:

| stage | fields | offers |
| --- | --- | --- |
| editing | writable | apply, reset |
| applying | read-only | nothing |
| errored | writable | apply, reset |
| watching | read-only | nothing |

The application MUST be watching while the device reports the session busy, MUST say there that someone else is configuring the device, and MUST NOT offer a scan or survey there.

Otherwise, the application MUST take its stage from the outcome the device reports when a session opens, when it stops being busy, and whenever that outcome changes: applying while it is `verifying`, errored where it is `failed`, and editing where it is `applied`.

The application MUST enter applying on apply, MUST take its stage from the `state` that follows the device sending the proposal back, and MUST enter errored where the device refuses the proposal.

The application MUST fill the fields with the configuration the device holds when a session opens, on reset, and with each configuration the device sends while watching, and MUST keep the document it proposed in them after apply.

The application MUST NOT let the operator edit a field while a proposal is being verified.

The application MUST show that it is waiting on the device while a proposal is being verified, a scan or survey is running, or a session is opening.

The application MUST close the session when the operator leaves the screen, and MUST drop the operator's edits with it.

Edits are not applied while the application is editing or errored and the fields differ from the configuration the device holds.

The application MUST ask the operator to confirm leaving the screen while edits are not applied, saying that they will be lost.

The application MUST have the browser warn before the page is closed, reloaded or navigated away from while edits are not applied.

> [!NOTE]
> An application that proposed as the operator typed would hand the device a gateway half entered, and the device would fail it for a reason that is not real.
> Keeping a failed proposal in the fields lets an operator correct the one field that was wrong. A device that saved it holds it too, as [CFG](session.md) requires, so the next session opens on it and on its failure.
> Taking the stage from the device rather than from the conversation lets an operator apply, leave while the device verifies, and come back to find it still verifying or to see how it went.
> A field edited part way through verification belongs to neither the attempt being verified nor the next one.
> A failed proposal the device saved is the configuration it holds, so leaving after a failure asks nothing: the work is kept on the device. A refused one is not, so leaving asks.
> A second operator watching sees the first one's changes and failures as the device reports them, and can take over once the first leaves without opening the screen again.

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

The application MUST say beside the list, in terms of the device's behaviour, that it tries the candidates from the top, uses the first that connects, and moves to another when that changes.

The application MUST make plain which candidate the fields being edited belong to.

The application MUST let the operator turn each candidate off and on again while editing, keeping its fields as they are and editable, MUST give a candidate the operator adds `enabled` true, and MUST show in the list which candidates are off.

The application MUST let the operator turn the hotspot off and on again while editing, keeping its settings as they are and editable, and MUST give a hotspot the operator adds `enabled` true.
The application MUST remove a hotspot the operator turns off whose settings could not work whatever the device supports, as one with no SSID or a passphrase too short to be one.
The application MUST hold a hotspot that is off to the same checks before proposing as one that is on.

The application MUST say, beside a hotspot that is on, where it cannot run beside a wireless connection the device reports joined now, naming the connection and its channel and that turning the connection off lets it run, and MUST still let the operator apply.

The application MUST show each candidate's state against it, distinguishing the candidate carrying the default route, one that is up, one that is off, and each way a candidate is unavailable.

The application MUST describe an unavailable candidate by the device's observation of it.

> [!NOTE]
> "No gateway", "no lease" and "out of range" are the same facts as "not here" and "not offered" with the diagnosis left in. An operator reads the first three and knows which setting to change.

## Scanning

The application MUST list the networks a scan heard by SSID, showing for each network the strongest signal among its access points and how many there are, with the access points behind it on request.

The application MUST leave out access points with no SSID unless the operator asks to see hidden networks.

The application MUST NOT let the operator change whether a network is hidden where a scan has settled it: one the last scan heard by its name is not hidden, and one picked from an access point with no SSID is.

The application MUST offer, from a scan, a view for siting an access point: every access point heard, by signal, with its channel and the adapter that heard it, and the channels taken on each band the device's radios can use, counting every channel a wide access point spans.

The application MUST let the operator scan or survey one adapter rather than every one.

> [!NOTE]
> Joining asks which network to add. Siting asks where the device's signal comes from and which channel a new access point should take, and it is the same scan read differently.
> Scanning takes a radio off its channel for a moment, so scanning one adapter spares a radio carrying the uplink or the hotspot.

## Suggesting the country

The application MUST offer to scan beside the country, where the device can scan.

The application MUST suggest, from the last scan of the session, the country named by the most access points it heard, counting only countries the device offers.
Where several are named by as many access points, the application MUST suggest each of them, none above the others.
The application MUST offer beside the suggestion every other such country the scan heard named.

The application MUST offer to set the country to each country it suggests or offers.

While the country is unset, the application MUST make setting a suggested country the section's primary action.
While the country is set to one it does not suggest, the application MUST say in a line beneath the field which country the access points name.
While the country is set to one it suggests, the application MUST NOT show any of this.

The application MUST say where the last scan heard no access point naming a country the device offers.

## Rendering a failure

The application MUST mark the field named by the `at` of a failure or a refusal.

The application MUST render the verification stages of [LINK](attachment.md), showing which the proposal passed and which it failed.

The application MUST render the `reason` of a failure or a refusal as the device wrote it.

The application MUST offer, on the candidate a failure's `at` names or names a member of, to propose the document again with that candidate's `verify` false.

The application MUST give a candidate the operator adds `verify` true, MUST show which candidates carry `verify` false, and MUST let the operator turn verification back on for one while editing.

> [!NOTE]
> The stages say the addressing was fine and the network is not routing, without a sentence having to say so.
> The reason is the device's own words about something the application did not anticipate, so there is no wording of its own to supply.
> Offering to skip verification only on the candidate that failed keeps it for the operator who has seen why the device refused and knows better, as with a network that is not up yet, and leaves every other candidate held to it.

## When the session ends

The application MUST say where the session has ended and offer to open it again, and where it cannot be opened again, MUST say so and offer to disconnect from the device so the operator can connect again.

> [!NOTE]
> A channel closing while a proposal is being verified ends the session with it, and the device is disconnected as after any closed channel of [CHN](../channel.md).
> The device holds a proposal from when it saves it and goes on verifying it whatever becomes of the session, as [CFG](session.md) requires, so an operator who leaves or loses the channel part way through verification loses nothing they applied.

## Wording

The application MUST supply its own wording for everything it renders, and MUST NOT put the vocabulary of [CFG](session.md) in front of an operator.

The application MUST name a setting by the term a technician configuring a network would use, except where a plainer term is the one they would look for.

> [!NOTE]
> SSID, passphrase and DHCP range name the thing precisely; name, password and addresses handed out are vaguer, and vagueness costs someone a second visit to a site.
> Country is the exception that shows the limit: someone hunting for the regulatory domain setting looks for the country, so the more correct term would be the harder to find. Precision gives way to discoverability, and to nothing else.
