# Testing provisioning on the v4 prototype

The test pass itself is the card's test cases. This plan holds what the pass turns up, and what is fixed in this card's PR.

## Fixed

- [x] The image's verity test read the device with uutils dd 0.8 to 0.10, which fails every direct read of a verity device, so no image ever published.
- [x] The medium stopped a USB boot on a supply reporting less than 5 A, waiting for the power button. Its `config.txt` sets `usb_max_current_enable=1`.
- [x] The medium found its partitions by the pi image's names, which an NVMe written from the payload shares, so it could boot or rotate the NVMe's partitions. Its partitions carry names of their own, its initramfs leaves out the identity rotation, and its root grow is masked.
- [x] The image is passed between CI jobs unarchived.

## To fix

- [x] A provisioned Iti ran no bliti: nothing installed it. The step `software` installs the medium's own bliti, its service and its configuration. Interim until G5 installs it from the apt repository F5 sets up.
- [x] `provision-state` carries `checked` and `provisioned`, for the page and the LED.

- [x] Steps that writing the disk undoes (`overlay`, `software`, `encryption`, `tailscale`) report not-done while `disk` is not done, saying what the board holds and that writing the disk replaces it (RUN).
- [x] The console's offer to acknowledge a finished run defaults to yes, so a stray Enter discards it.
- [x] CI caches nothing for the image build. Run `fetch.sh` beside the daemon build, and cache the device build's target directory and crate registry.
- [x] `tailscaled` ran on the medium from boot, as the pi image enables it. The medium masks it, and its first-boot sign-in, and the step `tailscale` runs its own.
- [x] Implement PLED's board-level states: provisioned from the done-states, and red and green alternating while on the air with no run under way.
- [x] The finished screen says "Provisioned" after a run of some steps, and its "Run steps again" is a small link under "Provision another Iti", so a finished board reads as one that cannot be provisioned further.
- [x] Closing the payload's verity mapping after `disk` failed as still in use: the mapping's own file was still open. Closed first now, and confirmed on the board.
- [x] On connecting, the page lists every step ticked, without the Iti's code, until the device has determined the done-states, then changes the ticks. It should say it is checking the board and hold the list back until then.
- [x] Downloading the sheet shows nothing between the click and the PDF opening. The button should say it is making the sheet, and not take a second click, until the PDF is ready.
- [ ] Not bliti's: on the `pi` image, SSH as `ubuntu` keeps reporting the password expired (`pam_unix(sshd:account) … root enforced`) after the forced change, and after `/etc/shadow` shows it changed, while a console login is let in. Raise with linux-images.
- [ ] Not bliti's: the default channel's bootloader of 2026-09-25 ignores `usb_max_current_enable` (rpi-eeprom issue 899), so a factory board carrying it needs its power button pressed to boot the stick on the backup supply. Say so in the assembly manual until the default channel carries a fixed release, and the medium then picks it up at its weekly build.
- [x] On a disk holding another system, the steps after `disk` reported unknown with cryptsetup's errors. They report not-done, "writing the disk comes first", while `disk` is not done.
- [x] A retried step took the answers it had been given before, so `disk` declined once failed on every retry. A retry discards the answers to the failed step's own prompts (RUN).
- [x] The app could not connect again in the same tab after a disconnect: the handshake completed and the page never spoke. The link now turns notifications off before it disconnects, and a reconnect works. The device still logs a subscription about half a second before the page reads its slot, so the exact mechanism isn't settled.
