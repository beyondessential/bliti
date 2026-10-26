# Tamanu status and restart (N3)

Scenarios for [TMU](../../specs/control/tamanu.md) and the Tamanu section of [NFO](../../specs/device-info.md).

## Reading Tamanu

- [x] A status with nothing short reads as `running`, with the web and facility server versions it runs (verifies spec: NFO)
- [x] A service short of its instances reads as `degraded`, with a reason naming it (verifies spec: NFO)
- [x] Every required service down reads as `stopped`, and each version entry holds the configured version as `failed` (verifies spec: NFO)
- [x] Instances on different versions give the highest and a `warning` saying which (verifies spec: NFO)
- [x] A version other than the configured one warns with both (verifies spec: NFO)
- [x] An expectation `bestool` cannot decide does not degrade Tamanu
- [x] A service present that must be absent degrades Tamanu while every required service runs
- [x] No installation found reads as `absent`, with both versions `skipped` (verifies spec: NFO)
- [x] A program that cannot be run, an unreadable status on an installation, and a status that fails on one read as `broken`, with all three entries `broken` (verifies spec: NFO)
- [x] Versions order as versions and not as text, and totally where some tags do not read as versions
- [x] Before the first look has ended, and once looking has stopped for want of a session, no Tamanu fact is reported, no act is listed and every act is refused (verifies spec: NFO)
- [x] Looks at Tamanu are serialised, so an older look is never published after a newer one
- [x] Looking at Tamanu stops once no session has been open for the idle period, and resumes at once when one opens
- [ ] On the prototype device, `bestool tamanu status --json` reads as the fixtures do, or the fixtures are replaced with a capture

## Facts

- [x] Each state maps to its `status` and a reason (verifies spec: NFO)
- [x] `starting` and `restarting` are reported from acceptance whatever is observed, including where observing is `broken` (verifies spec: TMU)
- [x] A change in Tamanu is a change in the facts sent on the feed

## The stream and the acts

- [x] The opening list is `start` while stopped, `restart` while running or degraded, and none otherwise (verifies spec: TMU)
- [x] An act not listed is refused at `$['act']`, including one this build does not know (verifies spec: TMU)
- [x] A second act while one is under way is refused at `$`, on another stream too (verifies spec: TMU)
- [x] An accepted restart is reported as `restarting` until carried out, then the list returns (verifies spec: TMU)
- [x] An accepted start is reported as `starting` until carried out (verifies spec: TMU)
- [x] An act that leaves services short sends `tamanu-failed` naming them, on the asking stream only (verifies spec: TMU)
- [x] An act whose command ends well but leaves Tamanu short fails once it has waited for it to settle (verifies spec: TMU)
- [x] A command that fails sends `tamanu-failed` saying how, even where Tamanu is `running` (verifies spec: TMU)
- [x] An act that never ends is given up on and fails (verifies spec: TMU)
- [x] An act is carried out after the stream that asked for it has closed (verifies spec: TMU)
- [x] The list is re-sent when Tamanu changes with no act under way

## The wire

- [x] The Tamanu messages round trip, and `tamanu-act` writes its act critical (verifies spec: TMU)
- [x] A `tamanu-act` carrying an unknown critical member is refused, and a lower case one is not
- [x] `tamanu-failed` always says why

## On a device

- [ ] A restart on a device with Tamanu runs through to `running`, the Status screen showing `restarting` meanwhile
- [ ] A start on a device with Tamanu stopped brings it to `running`
