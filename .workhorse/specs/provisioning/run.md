---
id: RUN
---

# Runs

A [run](../overview.md#run) carries out the [steps](../overview.md#step) an operator has chosen, one after another, on the device, whether or not any client or console is attached to it.

## Steps

A device MUST hold its steps in one fixed order, and MUST carry out the steps of a run in that order.

A step MUST be safe to start again after it has been interrupted at any point.

The steps a device offers, and the effect of each, are specified by the specs that define them.

## Done-state

A step MUST report a done-state, which is `done`, `not-done` or `unknown`, and MAY give a reason as text.

A step MUST report `done` where the board already holds what the step would produce, `not-done` where it does not, and `unknown` where the device cannot tell.

A step that changes nothing, as a check does, MUST report `not-done`.

A device MUST determine a done-state from the board as it stands, and MUST NOT infer it from the record of an earlier run.

A device MUST determine every step's done-state when it enters the `idle` or `finished` state, and whenever a provisioning stream is opened or the console attaches while it is in either.

A device MUST report `unknown` for a step whose done-state it has not determined since it started.

A client SHOULD offer a step reported `done` unticked, and every other step ticked.

> [!NOTE]
> An operator is offered what a board lacks, and can tick any step back on.
> A reason such as "Up to date" tells an operator why a step is unticked.

## Run states

A device is in one of five states.

| state | meaning |
| --- | --- |
| `idle` | no run is held |
| `running` | a run is under way and no step is waiting on a person |
| `waiting` | the step being carried out cannot go further until a person acts |
| `failed` | a step has failed and the run has stopped at it |
| `finished` | every step chosen has passed |

A step is in one of six states.

| state | meaning |
| --- | --- |
| `unchosen` | the step is not part of the run, or no run is held |
| `pending` | the step is part of the run and has not been reached |
| `running` | the step is being carried out |
| `waiting` | the step has an open blocking prompt |
| `passed` | the step has been carried out |
| `failed` | the step has failed |

A device MUST be in `waiting` while a step is `waiting`, and in `running` while a step is `running`.

## Acts

A device MUST accept each act in the states below, and MUST refuse it in any other.

| act | accepted in | effect |
| --- | --- | --- |
| `start` | `idle`, `finished` | begins a run of the steps chosen |
| `answer` | `running`, `waiting`, `failed` | answers an open prompt |
| `retry` | `failed` | carries out the failed step again, then the rest |
| `stop` | `waiting`, `failed` | ends the run |
| `acknowledge` | `finished` | discards the finished run |

A run MUST carry out exactly the steps chosen, and MUST NOT begin a step until every chosen step before it has passed.

A device MUST refuse a `start` that chooses no step, or a step it does not have.

A device that accepts `start` in `finished` MUST discard the finished run.

A device that accepts `retry` MUST carry out the failed step again from its beginning, and MUST NOT carry out again a step that has passed.

A device that accepts `stop` or `acknowledge` MUST enter `idle`.

A device MUST enter `failed` when a step fails, MUST record why, and MUST NOT carry out any later step.

A device MUST enter `finished` when the last chosen step passes, and MUST discard the stored state of the run.

A device MUST hold a `finished` run, with the results of its steps, until the run is acknowledged, replaced by a `start`, or the device stops.

A step MAY report the progress of its work as a fraction.

A step MAY report a result, whose members the spec that defines the step gives.

> [!NOTE]
> A run that has finished is not carried out again, and a board booted from the medium afterwards comes up with no run held.

## The run belongs to the device

A run MUST continue whether or not a client is connected and whether or not the console is attached.

A client connecting or leaving, and the console attaching or exiting, MUST NOT start, stop, pause or otherwise change a run.

A device MUST accept acts from a client and from the console in the same states and with the same effect.

> [!NOTE]
> A long step, such as writing a disk, runs while the operator is elsewhere, and an operator checks back by pressing the USER button and connecting.

## Surviving a reboot

A device MUST keep the state of a run on the provisioning medium: the steps chosen, the steps that have passed, and, where the run has failed, the step that failed and why.

A device MUST record that a step is about to begin before it begins the step, and that the step has passed once it has.

A device MUST key that state by the platform serial number of [BID](../board-id.md), and MUST read only the state of the board it runs on.

A device that starts in provisioning mode with the stored state of a run it was carrying out MUST resume the run, without any client, by carrying out the step it was at again from its beginning, then the steps after it.

A device that starts with the stored state of a failed run MUST enter `failed`, and MUST NOT carry out the failed step until it accepts a `retry`.

A device MUST NOT write the answer to any prompt to the provisioning medium.

> [!NOTE]
> A reboot comes from a step that needs one, as a firmware update can, and from a power cut partway through a long step.
> One medium can serve several boards in turn, each with its own run.

## Prompts

A step MAY ask an operator for something through a prompt, which has an id, a kind, text for the operator, and whether it is blocking.

| kind | answer | meaning |
| --- | --- | --- |
| `notice` | none | asks an operator to do something, and closes when the step sees it done |
| `confirm` | true or false | asks an operator to confirm |
| `text` | text | asks an operator for text |
| `secret` | text | asks an operator for text that is not to be shown or kept |

A prompt of kind `notice` MUST be blocking.

A step with an open blocking prompt MUST NOT go further until the prompt is closed, and MUST be `waiting`.

A step MAY end with a non-blocking prompt open.
The prompt remains open until it is answered or the run ends.

A step that needs an answer the device does not hold MUST open a blocking prompt for it, and MUST be `waiting` until it is answered.

A device MUST close every prompt when a run is stopped, replaced or finished.

The text of a prompt MUST be enough to act on without a picture, and MUST name every pad, button and switch by the label silkscreened on the board.

A device MUST accept the answer to a prompt from a client or from the console, and MUST refuse a second answer to the same prompt.

A device MUST hold the answer to a `secret` prompt in memory only, and MUST discard it when it stops.

> [!NOTE]
> A prompt that does not block lets the run go on while a question stays open, and the step that needs the answer waits for it only when its turn comes.
> A device that loses power between the question and the step that needs the answer holds no answer afterwards, so that step asks again.
> The text names parts of the board by their silkscreen labels because a console shows no picture, and a client can add one.
