# Run provisioning mode on the device

Scenarios that show provisioning mode working on the device. Covered by automated tests unless a
case says it needs the prototype board.

## The protocol

- [x] The four provisioning messages round trip, each optional member present and absent (verifies spec: PRS)
- [x] `provision-act` writes `ACT`, `STEPS`, `PROMPT` and `VALUE` critical, and refuses one carrying a critical member it does not know (verifies spec: PRS)
- [x] A prompt of a kind a newer device defines is read and kept by name (verifies spec: PRS)
- [x] An answer never shows in a message's debug output
- [x] The generator reaches every new message type, so the wire-compat oracle exercises them
- [x] A client holding only the provisioning token completes the handshake against a device with random keys (verifies spec: PRV)
- [x] A client holding a different token does not complete the handshake (verifies spec: PRV)

## The name

- [x] The name renders and parses as `ITI_PROVISION_<marker>_<suffix>`, and `FAULT` where the board is unknown (verifies spec: PRV)
- [x] The suffix is the last four characters of the encoded QR payload (verifies spec: PRV)
- [x] The longest name fits the advertising budget (verifies spec: PRV)
- [x] A board whose expected source is missing advertises `FAULT` and says why on stderr (verifies spec: PRV)
- [x] A weaker source than the one expected is never used (verifies spec: PRV)

## Going on the air

- [x] The device does not advertise from start until the USER button is pressed, whatever a resumed run's state is (verifies spec: PRV)
- [x] A press opens a five-minute window, and a press while it is open restarts it (verifies spec: PRV)
- [x] Advertising stops when a client connects and does not resume after it leaves until the next press (verifies spec: PRV)
- [x] A press while a client is connected is ignored (verifies spec: PRV)
- [x] A second client reads the allocation of a device with no slot free (verifies spec: PRV)
- [x] A press held down counts once (verifies spec: PRV)

## Runs

- [x] The chosen steps run in the device's order, whatever order they were named in (verifies spec: RUN)
- [x] A failing step stops the run and says why, and later steps do not run (verifies spec: RUN)
- [x] A retry carries out the failed step again and none that passed (verifies spec: RUN)
- [x] A run carries on with no client or console attached (verifies spec: RUN)
- [x] A run carries on after the client that started it leaves (verifies spec: RUN)
- [x] A run in progress resumes after a restart at the step it was in (verifies spec: RUN)
- [x] A failed run stays failed across a restart until it is retried (verifies spec: RUN)
- [x] A stored run that does not match the device's steps is discarded (verifies spec: RUN)
- [x] A finished run is held until acknowledged or replaced, and leaves nothing on the medium (verifies spec: RUN)
- [x] Stopping a waiting run ends it, closes its prompts and clears the medium (verifies spec: RUN)
- [x] A secret answer never reaches the medium (verifies spec: RUN)
- [x] A second answer to a prompt is refused at `$['prompt']` (verifies spec: PRS)
- [x] A prompt asked early and answered before its step's turn is not asked again (verifies spec: RUN)
- [x] A step whose turn comes without the answer waits, and the run reads `waiting` (verifies spec: RUN)
- [x] A notice holds its step until the step closes it (verifies spec: RUN)
- [x] A step that panics fails the run
- [x] Done-states are read from the board when idle and when finished, and `unknown` until read (verifies spec: RUN)
- [x] A board with no platform serial refuses a start (verifies spec: RUN)

## The stream

- [x] The state is sent on opening and after every change (verifies spec: PRS)
- [x] Every refusal names the act and the member at fault (verifies spec: PRS)
- [x] Two streams at once see the same run (verifies spec: PRS)
- [x] Streams asking for any other role or topic are passed over and the connection is unharmed (verifies spec: PRV)

## The console

- [x] The console lists the steps with their done-states and offers the done ones unticked (verifies spec: PCON)
- [x] The console starts a run, shows progress, failures, prompts and results (verifies spec: PCON)
- [x] A secret is read without echo (verifies spec: PCON)
- [x] The console works with no USER press and no client, and alongside a client, showing the same run (verifies spec: PCON)
- [x] The first answer to a prompt wins, from the console or from a client (verifies spec: RUN)
- [x] A run carries on when the console exits (verifies spec: PCON)

## The LED

- [x] Each run state shows the pattern of the table, from the moment the mode starts (verifies spec: PLED)
- [x] A change of state mid-blink switches pattern cleanly (verifies spec: PLED)

## On the prototype board

- [ ] Silent until USER is pressed; the name and suffix match the label; advertising stops on connect and stays off after disconnect; a second press restarts it
- [ ] Each LED state reads as the table says, amber included
- [ ] A fake long step survives a power cut and resumes, and a failed step stays failed across a reboot
