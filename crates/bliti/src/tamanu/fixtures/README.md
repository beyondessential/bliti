# Status fixtures

Each file is what `bestool tamanu status --json` prints, in the shape of the structs `bestool` serialises it from.
They are written from those structs and not captured from a device, so they hold what `bestool` is documented to print and not what a device has been seen to.
Replace them with captures when one is to hand, keeping each file's name and the state it pins.

| file | the device |
| --- | --- |
| `status-running.json` | every service runs the version it is configured to |
| `status-degraded.json` | one of the facility server's two instances is down |
| `status-stopped.json` | no service runs |
| `status-drift.json` | the facility server's instances run different versions, one not the configured one |
