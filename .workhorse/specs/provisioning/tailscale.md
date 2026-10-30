---
id: PTS
---

# Signing in to Tailscale

The step `tailscale` of a [run](../overview.md#run) signs the Iti in to the operator's tailnet from the provisioning system, so the Iti's first boot comes up on it.

The step asks for the auth key and the Iti's name early in the run, without holding the run up, and uses them once the root volume is open.

## Borrowed terms

| term | meaning |
| --- | --- |
| tailnet | As [NFO](../device-info.md) has it. |
| auth key | As [TSC](../control/tailscale.md) has it. |
| the disk's system | The system that [PDSK](disk.md) wrote to the board's disk, which the Iti runs from after provisioning, as opposed to the provisioning system the device is running now. |
| root volume, TPM keyslot | As [PENC](encryption.md) has them. |
| root filesystem | As [PDSK](disk.md) has it. |
| Tailscale state | What Tailscale keeps on a machine to remain signed in to a tailnet: the node's keys and its settings, the name among them. |

## Asking

Once every step of the run chosen up to and including `eeprom` has passed, or when the run begins where none of those is chosen, the step MUST open two non-blocking prompts.

| id | kind | asks for |
| --- | --- | --- |
| `auth-key` | `secret` | the auth key |
| `name` | `text` | the name the Iti registers under in the tailnet |

The text of `auth-key` MUST say that the key is to be reusable.

The prompt `name` MUST carry as its default `tamanu-iti-` followed by the suffix of [PRV](overview.md#the-advertised-name) in lower case.

A device MUST refuse an answer to `name` that is not a name of 1 to 63 lower case letters, digits and hyphens, beginning and ending with a letter or digit.

While a run that includes the step is under way and the step has not passed, a device MUST keep each of those prompts open that it holds no answer for, from the point above.

The step MUST NOT hold up the steps before it for either prompt.

Where the step's turn comes with `auth-key` unanswered, the step MUST make the prompt blocking, and MUST be `waiting` until it is answered.

Where the step's turn comes with `name` unanswered, the step MUST take its default, and MUST close the prompt.

> [!NOTE]
> The prompts come after `eeprom` because the steps before it need the operator at the board, while the steps after it, the disk write above all, are long and unattended.
> An operator answers while the disk is written and leaves.
> The default follows the suffix at the time the prompt opens, which is after any step that could change the board ID.
> A power cut loses both answers, as [RUN](run.md#prompts) says of any answer, so the prompts open again once the run resumes.

## Signing in

The step MUST open the root volume through its TPM keyslot, and mount the root filesystem, and MUST unmount and close them when it ends.

The step MUST sign in by auth key, registering under the name, as [TSC](../control/tailscale.md#signing-in) has `sign-in` do, applying any restriction to a single tailnet that the disk's system is configured with as TSC has it.

The step MUST sign in against the Tailscale state of the disk's system, such that the state is on the disk's system when the step ends, and the disk's own Tailscale resumes from it, signed in under the name, the first time that system starts.

The provisioning system MUST NOT hold the Tailscale state of the sign-in when the step ends, and MUST NOT leave Tailscale running.

The step MUST pass when Tailscale reaches `running` or `needs-approval`.

The step MUST fail, with a reason, where:

- the root volume cannot be opened through its TPM keyslot
- the device cannot reach Tailscale
- Tailscale rejects the auth key, or reaches neither state
- the sign-in joins a tailnet the disk's system does not accept

The step MUST discard the auth key when it ends, whether it passed or failed.

The step MUST NOT write the auth key to the disk or to the provisioning medium, and MUST NOT put it in a log or in a reason.

The step MUST report as its result `needs-approval`, a boolean that is true where Tailscale reached `needs-approval`, and false otherwise.

> [!NOTE]
> A failed step asks for the key again, since the key it had was discarded, and a key Tailscale rejected is no use a second time.
> Where the step is skipped or fails, the Iti is signed in afterwards from the operator application's Tailscale screen, [TSCR](../control/tailscale-screen.md).
> The provisioning medium serves several boards in turn, so a sign-in left in its own Tailscale state would be taken for the next board's.
> `needs-approval` is a sign-in that succeeded and is waiting on a tailnet administrator, which the operator can do nothing about at the bench.

## Done-state

The step MUST determine its done-state by opening the root volume through its TPM keyslot, mounting the root filesystem read-only, reading its Tailscale state, and closing them again, and MUST NOT write to the disk while it does.

The step MUST report:

| done-state | where |
| --- | --- |
| `done` | the Tailscale state of the disk's system records a sign-in |
| `not-done` | the root opened and its Tailscale state records no sign-in |
| `unknown` | the root volume cannot be opened through its TPM keyslot, or the disk holds no system |

The step SHOULD give as its reason why the done-state is `unknown`.
