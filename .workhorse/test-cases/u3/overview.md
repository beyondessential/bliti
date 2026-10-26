# Restyle the existing screens for Tamanu Iti

The connect steps, the renamed screens, history navigation and dev mode. Automated cases are Playwright against the fake client unless they say the real one.

## Branding

- [x] The connect screens, the page and the installed app are titled Tamanu Iti (verifies spec: WEB)
- [x] Text that is not a code reads "This isn't a Tamanu Iti code." through the real client (verifies spec: WEB)

## Reaching a device

- [x] Three steps show until a channel opens, the current one named and done ones ticked (verifies spec: WEB)
- [x] Open camera is the first step's action; pasting the code text is under Advanced, closed at first (verifies spec: WEB)
- [x] Scanning says how, hides Recently used and Advanced, and Cancel stops it (verifies spec: WEB)
- [x] A camera that cannot be opened says so and offers Try again, and typing still works (verifies spec: WEB)
- [x] A pasted code that does not read marks the field and says why beside it (verifies spec: WEB)
- [ ] A code the camera sees that does not read is said beneath the camera, which keeps scanning (verifies spec: WEB)
- [x] Finding shows the name to look for, Scan a different code and Save code for reprinting (verifies spec: WEB)
- [x] Once a device is picked, step 3 says it is connecting; Cancel returns to finding it, and the abandoned attempt connects nothing (verifies spec: WEB)
- [x] A chooser closed with nothing picked says the Iti wasn't found, and offers Try again (verifies spec: WEB)
- [x] A channel that drops says the connection was lost, with the reason where there is one, and offers Try again (verifies spec: WEB)
- [x] A device that does not come back after a restart or reboot says the connection was lost (verifies spec: WEB)
- [x] Recently used sits above the means of reading a code, each with Connect and its code ending (verifies spec: WEB)
- [x] A browser that cannot open a channel says so and offers nothing else (verifies spec: WEB)

## Screens of a connected device

- [x] All readings carries Power and battery, Network settings and Disconnect beside its title (verifies spec: VIEW)
- [x] Power and battery holds power then battery, and its Back returns to All readings (verifies spec: CSCR)
- [x] An act's confirmation sits in its row in place of its button, with the other acts in view, and says a reboot leaves the Iti unavailable (verifies spec: CSCR)
- [x] Network settings is titled so, and its Back returns to All readings (verifies spec: NSCR)
- [x] The title stays on one line on a phone, with the actions beside it

## History

- [x] The browser's way back leaves Power and battery and Network settings as their Back does (verifies spec: WEB)
- [x] Going back from All readings leaves the app (verifies spec: WEB)
- [x] Going back from Network settings with edits not applied asks; Stay keeps the edits, Leave goes up (verifies spec: NSCR)
- [x] Leave chosen from the screen's own Back does not ask again (verifies spec: NSCR)
- [x] An act under way takes the operator to All readings and leaves no entry behind (verifies spec: WEB)
- [x] Going forward after disconnecting goes nowhere (verifies spec: WEB)
- [ ] The phone's back gesture on Android Chrome behaves as the on-screen Back, on a real device

## Dev mode

- [x] The activity record is hidden until a title is tapped seven times in a row, and seven more hide it; six do nothing (verifies spec: WEB)
- [x] Dev mode lasts through a reload of the tab (verifies spec: WEB)
