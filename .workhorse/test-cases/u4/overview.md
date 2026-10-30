# Signing in to Tailscale during provisioning

Scenarios for the `tailscale` step and the client's saved key.
Device tests run against epic, not on this card.

## The step (verifies spec: PTS)

- [x] The key and the name open as non-blocking prompts once the EEPROM step has passed, with the name defaulting to `tamanu-iti-` and the lower-case suffix
- [x] The prompts open at the start of the run where no step up to the EEPROM is chosen, and again after a restart
- [x] A name that is not 1 to 63 lower case letters, digits and hyphens, edged by letters or digits, is refused at the value
- [x] An unanswered name takes the default at the step's turn; an unanswered key makes the step wait
- [x] A board whose ID could not be established offers no default and waits for a name
- [x] The step waits for the board's suffix before offering the name
- [x] The sign-in is carried out against the state directory on the disk's root filesystem, with the system's accepted tailnet
- [x] The key is discarded when the step ends, passed or failed, and a retry asks again
- [x] The key reaches neither the disk nor the medium
- [x] The root filesystem is unmounted whether the step passes or fails
- [x] A sign-in waiting on approval passes with `needs-approval` true
- [x] A failure says why and does not contain the key
- [x] Done-state: done where the state records a sign-in, not done where it records none or was logged out, unknown where the root cannot be opened or the state is unreadable; read-only
- [x] A machine already signed in is signed out before the key signs it in, and one in another tailnet than the accepted one is signed out and fails
- [ ] A real tailscaled, run against a real unlocked root, leaves state the Iti's own first boot comes up on the tailnet from (device test, epic)
- [ ] A medium's own Tailscale holds no sign-in afterwards (device test, epic)

## The prompt default and the console (verifies spec: PRS, PCON)

- [x] A prompt carries `default` on the wire only where it has one
- [x] The console shows a default and takes an empty line as it, for a text prompt only

## The client (verifies spec: PWEB)

- [x] Both prompts show beneath the Tailscale row while it is pending, the name filled with its default, the key hidden until revealed
- [x] A typed key is kept in local storage with its time, and is in no activity record
- [x] A saved key answers the prompt at once, once, without the field, and the page says so with the date and offers to forget it
- [x] A key a week old is dropped and asked for; one just under is used; one that cannot be read is dropped
- [x] Forgetting removes it from local storage
- [x] A key the device refuses, or a failed step, discards it
- [x] A key typed on one board is used for the next, across a reload
- [x] A sign-in waiting on approval is said so on the row and on the finished screen
