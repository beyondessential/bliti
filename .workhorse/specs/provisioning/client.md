---
id: PWEB
---

# Provisioning client

The provisioning client is the page of the [web application](../web-app.md) through which an operator at a device in [provisioning mode](../overview.md#provisioning-mode) finds it, chooses the steps of a [run](../overview.md#run), and watches the run through to the board's record, over the stream of [PRS](stream.md).

## The page

The provisioning client MUST be the page of [WEB](../web-app.md) at `/provision`.

The page that reaches a device MUST NOT link to the provisioning client.

The provisioning client MUST title itself Provision an Iti, and MUST call the device it reaches an Iti.

The provisioning client MUST work with one device at a time.

Where the browser cannot open a channel at all, the provisioning client MUST say so, and MUST NOT offer anything else.

The provisioning client MUST keep the record of [Activity](../web-app.md#activity), and show it in dev mode, as the page that reaches a device does.

> [!NOTE]
> Several boards at once means several tabs, each with its own device.

## Finding a device

Until a channel opens, the provisioning client MUST ask the operator to press the [USER button](../overview.md#user-button) on the device, with the picture of the USER button of [Pictures](#pictures).

The provisioning client MUST offer opening the chooser as the primary action.

The provisioning client MUST filter the chooser by the prefix of the name of [PRV](overview.md) up to the suffix, one for each version marker it implements, and MUST ask for access to the service of [ADV](../discovery.md) alongside the filter.

Where the chooser closes without a device picked, the provisioning client MUST say that only an Iti whose USER button was just pressed is listed, and MUST let the operator open the chooser again.

Once a device is picked, the provisioning client MUST say that it is connecting until the channel opens or fails, and MUST offer to cancel, which abandons the attempt and returns to finding a device.

The provisioning client MUST open the channel as [PRV](overview.md) requires of a client connecting to a device in provisioning mode.

Where the device has no slot free, the provisioning client MUST say that something else is connected to that Iti.

Once the channel is open, the provisioning client MUST open a provisioning stream, and MUST name the device by the suffix of its name for as long as it is connected.

Once connected, the provisioning client MUST offer to disconnect, which closes the channel and returns to finding a device.

## Refusing a medium

The provisioning client MUST implement a range of the revisions of [PRS](stream.md#the-revision).

Where the device sends a revision below that range, or an image date of [PRS](stream.md#the-images-date) 30 days or more before the client's own date, the provisioning client MUST say that the provisioning medium needs writing again with the latest image.

Where it refuses an image by its date, the provisioning client MUST say the date the image was built.

Where the device sends a revision above that range, the provisioning client MUST say that the page is out of date and offer to reload it, whatever the image's date.

Where it has said either, the provisioning client MUST NOT send `provision-act`, and MUST NOT show the steps.

## Choosing the steps

While the device is `idle`, the provisioning client MUST list every step in order, each with a checkbox, and each with its reason where the device gives one.

The provisioning client MUST tick each step whose done-state is not `done`, as [RUN](run.md) recommends, and MUST let the operator tick and untick any step.

The provisioning client MUST offer starting a run of the steps ticked as the primary action, and MUST NOT offer it while no step is ticked.

## The run

While the device is `running`, `waiting` or `failed`, the provisioning client MUST list every step in order with its state, showing a step that is `unchosen` as skipped.

The provisioning client MUST show the progress of a step that reports one as a bar.

The provisioning client MUST show each open prompt beneath the row of the step that asked it, and MUST show its text.

The provisioning client MUST offer an answer to each prompt by its kind:

| kind | offered |
| --- | --- |
| `notice` | nothing to answer |
| `confirm` | a choice of yes or no |
| `text` | a field, filled with the prompt's default where it has one, and saving it |
| `secret` | a field that hides what is typed until the operator reveals it, and saving it |

The provisioning client MUST NOT keep the answer to a `secret` prompt once it has sent it, other than the answer to the prompt `auth-key` as [Tailscale details](#tailscale-details) has it.

The provisioning client MUST NOT put the answer to a `secret` prompt in the record of Activity.

Where the device refuses an answer, the provisioning client MUST say so beneath the prompt and let the operator answer again.

Where a step has failed, the provisioning client MUST show why beneath its row, MUST offer to try the step again as the primary action, and MUST offer to stop.

While the device is `waiting`, the provisioning client MUST offer to stop.

While a run is under way, the provisioning client MUST say that the page can be closed and that pressing the USER button checks back.

> [!NOTE]
> A device holds the whole run, so a client that connects partway through shows the step list from the first `provision-state` it receives.

## Tailscale details

The prompts `auth-key` and `name` of [PTS](tailscale.md) open while the step `tailscale` is pending, and the provisioning client MUST show them beneath its row, as [The run](#the-run) has it.

The provisioning client MUST keep the answer to `auth-key` in the browser's local storage, with the time it was saved, so that it is kept across tabs and across visits.

The provisioning client MUST discard a saved key a week or more after it was saved.

Where `auth-key` opens and a key is saved, the provisioning client MUST answer the prompt with the saved key at once, without showing the field, MUST say that a saved key was used and when it was saved, and MUST offer to forget it.

Forgetting the saved key MUST remove it from local storage.

The provisioning client MUST NOT show a saved key.

Where the device refuses the answer a saved key gave, or the step `tailscale` fails, the provisioning client MUST discard the saved key, and MUST show the field for `auth-key` where it opens again.

The provisioning client MUST NOT keep the answer to `name`.

Where the step `tailscale` has passed with `needs-approval` true, the provisioning client MUST say that a tailnet administrator has to approve the Iti before it joins the tailnet.

> [!NOTE]
> A reusable key lets an operator provision a bench of boards from one entry, and a week is as long as the key itself is asked to last.
> A key that can add machines to a tailnet stays on the browser of the bench for that week, and the week bounds it.
> A step that fails discards the key it was given, so the saved key is dropped with it, and the operator is asked for another instead of sending one Tailscale already rejected.

## When the run has finished

While the device is `finished`, the provisioning client MUST show the board's record of [PRS](stream.md#the-record), if a step reports one, and MUST say that it is also saved to the provisioning medium.

The provisioning client MUST show the recovery passphrase, where the record carries one, and MUST offer to copy it.

The provisioning client MUST tell the operator to power the device off and remove the provisioning medium.

The provisioning client MUST offer to choose steps to run again, which lists the steps as [Choosing the steps](#choosing-the-steps) has it, and to provision another Iti, which disconnects and returns to finding a device.

The provisioning client MUST NOT send `acknowledge`.

> [!NOTE]
> The device holds a finished run until it is powered off, so an operator who closed the page before copying the passphrase gets the record back by pressing the USER button and connecting.
> Running steps again on a finished board is how a single step, such as printing its label, is repeated.

## When the channel closes

Where the channel closes other than by the operator disconnecting, the provisioning client MUST go on showing what it last showed of the run, marked as no longer current, and MUST NOT offer anything that sends an act.

While it does, the provisioning client MUST ask the operator to press the USER button to check back, and MUST offer opening the chooser.

While it does and the page is shown, the provisioning client SHOULD reconnect to the same device on its own once the device advertises again, without offering the chooser.

> [!NOTE]
> A device in provisioning mode stops advertising when a client connects and stays silent after it leaves until its USER button is pressed again, so a client cannot reach it again on its own until then.
> A phone that puts the page in the background, a step that reboots the device, and an operator who walks out of range all close the channel.

## Pictures

The provisioning client MUST carry a picture of the board for each of the following, and MUST show it where the table says.

| shows | where |
| --- | --- |
| the USER button | while finding a device |
| the CASE switch | beneath the prompt `case` of the step `hardware` |
| the ID WP pads | beneath the prompt `id-wp` of the step `eeprom` |

Each picture MUST mark the part it shows, and the provisioning client MUST caption it with where the part is and what to do with it.

The provisioning client MUST show the pictures offline once it is ready offline, as [WEB](../web-app.md) requires of everything it needs.

> [!NOTE]
> The picture carries only a mark, and the words sit in the caption, so they stay legible at the width of a phone.
> The prompt's own text names the part by its silkscreen label, as [RUN](run.md) requires, so the picture adds to it rather than replacing it.
