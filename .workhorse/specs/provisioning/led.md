---
id: PLED
---

# Status LED

A device in provisioning mode shows on its status LED whether the board is provisioned, whether it is on the air, and how its [run](../overview.md#run) stands, so that an operator can read a bench of boards at a glance.

## The states

Amber is red and green lit together.

A blink at 1 Hz is one cycle of lit and dark each second, and a blink at 3 Hz is three.
Red and green alternating is red alone then green alone, each for half a second.

A board is provisioned where every step that changes the board reports a done-state of `done`, as [RUN](run.md#done-state) has it.

A device in provisioning mode MUST show the first of these states that applies.

| state | applies where | LED |
| --- | --- | --- |
| running | the device is `running` | green, blinking at 1 Hz |
| waiting | the device is `waiting` | amber, blinking at 1 Hz |
| on the air | the device advertises, as [PRV](overview.md#going-on-the-air) has it | red and green alternating |
| failed | the device is `failed` | red, blinking at 3 Hz |
| provisioned | the board is provisioned | green, steady |
| ready | otherwise | amber, steady |

A device MUST show its state from the moment provisioning mode starts.

A device MUST take a board as not provisioned until it has determined every step's done-state since it started.

A device MUST NOT drive the status LED when it is not in provisioning mode.

> [!NOTE]
> Steady green means the board is provisioned and the medium can be taken out, whether the run that did it finished this boot or the board was provisioned before it booted the medium.
> A run of only some steps leaves a board that is not provisioned, so it shows amber again once the run finishes.
> A press of the USER button shows at once while no run is under way, and a press partway through a run leaves the run's state showing.
> A step with a prompt open beside it does not make the run `waiting` unless the step is waiting for the prompt, so the LED shows green while work is under way and the operator is not needed.
