# Restyle the existing screens for Tamanu Iti

Lanes 1 (connect) and 6 (Advanced) of the Master redesign mockup on H3's branch, with H3's working doc.
Styling the screens that exist, not adding navigation: the Status screen and its Advanced section are V3's.

## Decisions

- All readings stays the landing. Its title carries Power and battery, Network settings and Disconnect beside it, where Info carried Control and Disconnect. V3 moves the first two under Advanced.
- Every title action is a small secondary button, as the mockup draws Back.
- The activity log stays on every screen, shown only once dev mode is on: seven taps in a row on a screen's title, kept for the tab.
- History entries cover the screens of a connected device only. Back from the landing leaves the app.
- A back gesture out of Network settings with edits not applied puts the entry back and asks, as the on-screen Back does.
- Palette follows the mockup: accent `#326699`, secondary buttons outlined in it. Sizes stay the shipped ones; the mockup's phone frames are scaled.
- Act confirmations sit in the act's row in place of its button, as F3's CSCR has it. Battery confirmations stay as they are.
- The downloaded SVG keeps the `bliti-` name WEB gives it.

## Steps

- [x] Specs: WEB (branding, connect steps, dev mode log, history), VIEW (All readings title and actions), CSCR (Power and battery, no network section, confirmation in the row), NSCR (Network settings title, back to the landing)
- [x] Palette and new component styles in `styles.css`; manifest and page title Tamanu Iti
- [x] Shared screen shell (title, actions beside it, dev mode taps, activity log)
- [x] Connect flow: steps indicator, scan, Advanced paste, Find my Iti, connecting with Cancel, not found, connection lost, camera not available, unsupported browser, Recently used
- [x] Client: say when a device has been picked, so step 3 shows; Cancel abandons the attempt; error copy says Iti
- [x] Renames: Info to All readings, Control to Power and battery (component and tests), Network to Network settings
- [x] History navigation with `popstate`, including forced returns to the landing and the Network settings leave guard
- [x] Tests updated and added; test cases file
