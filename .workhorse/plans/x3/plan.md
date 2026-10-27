# Tailscale and Tamanu actions on the Status screen

## Device work

The network check's result carries Tailscale's own report as `report` (TSC), so the Tailscale screen can share everything Tailscale found.
The device already runs `tailscale netcheck --format=json` and parses the members it maps; it keeps the command's output text as it is and sends it as `report` beside them.
The protocol's result schema and its tests change with it, and the shipped client gains nothing to parse: it passes `report` on untouched.
