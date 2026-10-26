# Tailscale fixtures

Bodies recorded from tailscaled 1.104.2's local API, with the public key and addresses blanked.

| file | what it is |
| --- | --- |
| `status-running.json` | `status` of a signed-in, connected device with one peer |
| `status-stopped.json` | the same device after `WantRunning` was set false |
| `status-expired.json` | the same device after its control server expired its key: `NeedsLogin`, `HaveNodeKey` true, `Self.Expired` true |
| `status-needs-login.json` | the same device after `logout`: `NeedsLogin` with no node key, as on a device that never signed in |
| `ping-relayed.json`, `ping-direct.json`, `ping-unknown-peer.json` | one probe of the local API's `ping` for each outcome |

`ping-*` were assembled from observed answers, with the peer's name and address replaced.

## Recording them again

The control server was Headscale in a container, and the device a second `tailscaled` in userspace networking with its own socket and state directory, so no real tailnet is touched.

- Rootless podman without a subordinate ID range needs `--storage-opt overlay.ignore_chown_errors=true` to unpack the image, and the container needs `--user 0`.
- Run `headscale serve` with its example configuration, `server_url` set to the address published on the loopback interface, then create a user and a pre-auth key with `headscale preauthkeys create`.
- Start the device with `tailscaled --tun=userspace-networking --socket=... --statedir=... --port=0`, set `ControlURL` through `start` and not through `prefs`, then sign in as the plan describes.
- `headscale nodes expire` produces the expired state, `logout` the needs-login one, and a second device gives a peer to ping.
- Match the daemon's own version to the `tailscale` package the image pins.
