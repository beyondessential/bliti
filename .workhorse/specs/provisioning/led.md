---
id: PLED
---

# Status LED

A device in provisioning mode shows the state of its [run](../overview.md#run) on its status LED, so that an operator can read a bench of boards at a glance.

## The states

Amber is red and green lit together.

A blink at 1 Hz is one cycle of lit and dark each second, and a blink at 3 Hz is three.

A device in provisioning mode MUST show its state on the status LED as follows.

| state | LED |
| --- | --- |
| `idle` | amber, steady |
| `running` | green, blinking at 1 Hz |
| `waiting` | amber, blinking at 1 Hz |
| `failed` | red, blinking at 3 Hz |
| `finished` | green, steady |

A device MUST show its state from the moment provisioning mode starts.

A device MUST NOT drive the status LED when it is not in provisioning mode.

> [!NOTE]
> A board that has booted from a provisioning medium shows amber whenever no run is held, so one that would otherwise ship with the medium still in stands out.
> A finished run is not held across a stop, so a later boot of a finished board from the medium shows amber again, and green lasts until the board is powered off.
> A step with a prompt open beside it does not make the run `waiting` unless the step is waiting for the prompt, so the LED shows green while work is under way and the operator is not needed.
