# Q4: provisioning client page

## Notes

- The client is tightly coupled to the device's steps: provisioning is run from the latest deployed app, and the PRS revision is what refuses a medium written for other steps. The device side of the revision and the `record` result shape land in this card too (`bliti-core` report types and the provisioning stream), since K4 shipped without them.
- The step ids `hardware` and `eeprom`, and the prompt ids `case` and `id-wp`, are fixed by PWEB for the pictures. The card that builds those steps uses them.
- The device's step registry is still empty, so the page is built and tested against K4's fake steps.
- The board photos move from `.workhorse/design/assets/` into the app's assets. The uncropped original stays in `.workhorse/design/assets/` for re-cropping.
- The link to and from the assembly manual is left to G4.
- A step-specific card may override PWEB's generic handling. For example, the Tailscale card may keep the auth key for the session, where PWEB discards `secret` answers once they are sent.
- The device has to be told the image's build date by means outside the protocol. The image-build card decides where the date lives on the medium. Until then the daemon takes it from configuration, as it does the board ID source, so the fake steps and tests can set it.
