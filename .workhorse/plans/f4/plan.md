# Testing provisioning on the v4 prototype

The test pass itself is the card's test cases. This plan holds what the pass turns up, and what is fixed in this card's PR.

## Fixed

- [x] The image's verity test read the device with uutils dd 0.8 to 0.10, which fails every direct read of a verity device, so no image ever published.
- [x] The medium stopped a USB boot on a supply reporting less than 5 A, waiting for the power button. Its `config.txt` sets `usb_max_current_enable=1`.
- [x] The medium found its partitions by the pi image's names, which an NVMe written from the payload shares, so it could boot or rotate the NVMe's partitions. Its partitions carry names of their own, its initramfs leaves out the identity rotation, and its root grow is masked.
- [x] The image is passed between CI jobs unarchived.

## To fix

- [ ] Steps that writing the disk undoes (`overlay`, `encryption`, `tailscale`) are offered unticked where the disk being replaced already has them, so a re-provision with the defaults leaves a system without the HAT overlay or Tailscale. Decided: the device reports them not-done, or in a state of their own, where `disk` is not done, and the board as it stands is shown apart from the list of steps to run. Needs a RUN change first.
- [ ] The console's offer to acknowledge a finished run defaults to yes, so a stray Enter discards it.
- [ ] CI caches nothing for the image build. Run `fetch.sh` beside the daemon build, and cache the device build's target directory and crate registry.
- [ ] `tailscaled` runs on the medium from boot, as the pi image enables it. Check what the Tailscale step leaves running.
- [ ] Implement PLED's board-level states: provisioned from the done-states, and red and green alternating while on the air with no run under way.
- [ ] The finished screen says "Provisioned" after a run of some steps, and its "Run steps again" is a small link under "Provision another Iti", so a finished board reads as one that cannot be provisioned further.
- [ ] Closing the payload's verity mapping after `disk` fails as still in use, leaving `bliti-payload` mapped. Find what still holds it (the mapping's file, or the loop device for its partitions) and close it first.
- [ ] On connecting, the page lists every step ticked, without the Iti's code, until the device has determined the done-states, then changes the ticks. It should say it is checking the board and hold the list back until then.
