---
id: PRS
---

# Provisioning stream

A client chooses the steps of a [run](../overview.md#run), starts it, answers its prompts and watches it, through a provisioning stream, and learns where the run stands from the device.

The provisioning stream is a stream role beyond those of [MSG](../messages.md), established as MSG requires by which end opened the stream and by its first message.

[RUN](run.md) defines the runs, steps, states and prompts the messages below carry.

## The exchange

A client MUST open a provisioning stream by opening a stream whose first message is `provision`.

A device MUST answer `provision` with `provision-state`.

A device MUST send `provision-state` on every open provisioning stream each time anything it carries changes.

A client MUST ask for an act by sending `provision-act` on the provisioning stream, carrying as `ACT` a critical member naming the act, and the act's own parameters.

A device MUST answer every `provision-act` exactly once, with `provision-accepted` where it will carry the act out, and otherwise with `cannot-act` of [MSG](../messages.md).

A device MUST serve any number of provisioning streams at once.

A device MAY combine several changes of progress into one `provision-state`.

> [!NOTE]
> The device sends the whole state each time, so a client that connects after pressing the button on a board partway through a run learns where it stands from one message.
> The selector and every parameter are critical because an act carried out without one it did not read would be a different act, as for [CTL](../control/power.md).

## The acts

| `ACT` | parameters | meaning |
| --- | --- | --- |
| `start` | `STEPS`, an array of step ids | begin a run of those steps |
| `answer` | `PROMPT`, the id of a prompt, and `VALUE`, a boolean for a `confirm` prompt and a string for a `text` or `secret` prompt | answer a prompt |
| `retry` | none | carry out the failed step again |
| `stop` | none | end the run |
| `acknowledge` | none | discard a finished run |

The states that accept each act, and its effect, are specified by [RUN](run.md).

A device MUST refuse an act at the paths below.

| refused where | at |
| --- | --- |
| `ACT` names no act the device recognises | `$['act']` |
| the state does not accept the act | `$` |
| `STEPS` is empty | `$['steps']` |
| `STEPS` names a step the device does not have | `$['steps'][i]`, with `i` the index of the first such step |
| `PROMPT` names no open prompt, a prompt already answered, or a `notice` prompt | `$['prompt']` |
| `VALUE` is not of the type the prompt's kind takes | `$['value']` |

## The state

`provision-state` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `revision` | integer | yes | the revision of the steps the device offers, as [The revision](#the-revision) has it |
| `built` | string | yes | the date the image on the provisioning medium was built, as [The image's date](#the-images-date) has it |
| `run` | string | yes | the state of the device, one of `idle`, `running`, `waiting`, `failed` and `finished` |
| `steps` | array | yes | every step in order, each as below |
| `prompts` | array | yes | every open prompt, each as below, and empty where none is open |

Each member of `steps` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `id` | string | yes | names the step, in lower case letters, digits and hyphens |
| `title` | string | yes | the step's name for an operator |
| `done` | string | yes | the done-state, one of `done`, `not-done` and `unknown` |
| `why` | string | no | the reason for the done-state |
| `state` | string | yes | the step's state, one of `unchosen`, `pending`, `running`, `waiting`, `passed` and `failed` |
| `progress` | number | no | the fraction of the step's work done, from 0 to 1, where the step reports one |
| `reason` | string | no | why the step failed, where `state` is `failed` |
| `result` | object | no | the step's result, where it reports one |

Each member of `prompts` MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `id` | string | yes | names the prompt among those open |
| `step` | string | yes | the id of the step that asked |
| `kind` | string | yes | one of `notice`, `confirm`, `text` and `secret` |
| `text` | string | yes | what to tell the operator |
| `blocking` | boolean | yes | whether the step waits for the prompt |

A step or a prompt MAY carry further members that the spec defining the step gives.

A client MUST show the `text` of a prompt whose kind it does not recognise, and MUST NOT answer it.

## The revision

The revision names one set of steps, with their ids, prompts and results, that a device offers and a client is built for.

The revision MUST change whenever a step, a prompt or a result changes in a way that a client built for the earlier revision would misread or could not drive.

A device MUST send the revision these specifications describe, which is 1.

A client MUST NOT send `provision-act` to a device whose revision it does not implement.

> [!NOTE]
> A client is built knowing each step, so it can show the board's parts beside the prompts that name them.
> The revision is what keeps that client from driving a provisioning medium written before, or after, the steps it knows, and it gates nothing outside provisioning mode.

## The image's date

A device MUST send as `built` the date, as an RFC 3339 full date, on which the image on its provisioning medium was built.

A device MUST be told that date by means outside this protocol.

A client MUST NOT send `provision-act` to a device whose image was built 30 days or more before the client's own date.

> [!NOTE]
> The image is rebuilt every week, so one a month old has missed several builds, and with them the newest system, firmware and steps.
> A medium that has sat in a drawer is caught by its date even where its revision is still one the client implements.

## The record

A step that saves the board's record MUST report it as the member `record` of its result, carrying:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `suffix` | string | yes | the suffix of the name of [PRV](overview.md) |
| `version` | string | no | the board's version, as [HAT](../hat.md) has it, where the run established it |
| `disk` | string | no | the UUID of the encrypted partition on the board's disk, where the run set it up |
| `passphrase` | string | no | the recovery passphrase of that partition, where the run set it |
| `at` | string | yes | when the record was saved, as an RFC 3339 date and time |

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `provision` | client | a provisioning stream, as the first message | nothing beyond `type` |
| `provision-state` | device | the provisioning stream | `revision`, `built`, `run`, `steps`, `prompts` |
| `provision-act` | client | the provisioning stream | `ACT` and the act's parameters |
| `provision-accepted` | device | the provisioning stream | nothing beyond `type` |
| `cannot-act` | device | the provisioning stream | as [MSG](../messages.md) has it |
