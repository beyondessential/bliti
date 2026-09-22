# bliti

bliti provisions headless devices over Bluetooth Low Energy, anchored to a QR code carried on the
outside of the device. A device advertises an opaque handle, and a client that has scanned that
device's QR code, and only such a client, can recognise it among the advertisements it hears,
authenticate to it, and open a two-way channel.

The QR code stands in for the button press or on-screen code that other provisioning protocols use
to establish that the operator is physically present, because the devices bliti targets have neither
a button nor a screen. bliti is not an implementation of Improv Wi-Fi and does not interoperate
with it.

## The chain

Every value descends from an identifier the board's own firmware provides, under constants that are
public:

```
                            ┌──> presence token ──keyed hash──> advertised handle
board ID ──argon2id──> root ┤    (in the QR code)               (broadcast over BLE)
(firmware)                  └──> device static key
                                 (public half in the QR code)
```

The board ID is the strongest identifier the board offers: a TPM Endorsement Key, else written
one-time-programmable memory, else the Raspberry Pi device-tree serial. It appears in no QR payload and no
advertisement, and the derivation does not run backwards, so photographing a QR code does not yield
it, and cannot yield the device's static key either.

There is no fleet key and no authoritative per-device record. The whole chain is reproducible from
the board alone, so a QR code can be reproduced from the device itself rather than from a record of
what was issued, and anything a device stores about its own identity is a cache it can rebuild.

Above the link, the two ends run a Noise `NKpsk0` handshake: the device is authenticated by its
static key, whose public half the QR code carries, and the client by the presence token as the
pre-shared key. They then multiplex JSON messages over streams either end can open.

## The crates

| crate | what it is |
| --- | --- |
| `bliti-core` | the protocol: board-ID sources, key schedule, advertisement payload, framing, Noise, streams |
| `bliti` | the daemon a device runs and the QR code generator, sharing that core |
| `bliti-web` | the browser client, as a wasm module |

`bliti-core` builds without its default features for wasm, which drops the argon2id derivation and
every board-ID backend: a client reads the presence token from the payload rather than deriving it.

## Hardware

bliti derives a device's identity from a hardware-backed board ID, and how good that identity is
depends entirely on what the board offers.

A device that is not a Raspberry Pi needs a TPM 2.0. Nothing else on such hardware qualifies as a
source, so the daemon refuses to run without one. Vendor identifiers such as an SMBIOS system UUID
are not used: on many machines they are a reformatting of a service tag that is printed on the
chassis and kept in asset registers, which makes them public rather than secret.

On Raspberry Pi hardware, write at least 64 bits of random data into the customer OTP region, or fit
a TPM. Either gives a board ID an attacker cannot search for. The device-tree serial is accepted as
a fallback so that boards already in the field keep working, but a Pi serial is a fixed structured
value rather than a secret, and a device identified by one carries a weaker guarantee. Do not choose
it for new deployments.

## Running a device

The daemon advertises whenever it is running and serves sessions to clients that authenticate:

```console
$ bliti daemon
```

`services/` holds the systemd unit and the two BlueZ configuration files it needs. bluetoothd has
to be peripheral-only, because a stack that resolves the connecting client's attributes in turn will
ask to pair, and no client bliti serves can pair. The drop-in points bluetoothd at the config
shipped here rather than editing `/etc/bluetooth/main.conf`, which is a package conffile.

The daemon derives its presence token on its first start after imaging, and after a board change;
every later start reads the cache. The derivation wants 2 GiB of memory at once, and the kernel
kills a process that asks for more than there is rather than failing the allocation, so the daemon
establishes there is room before it begins.

## Generating a QR code

```console
$ bliti qr           # draw the QR code in the terminal
$ bliti qr --svg     # write it as SVG for printing
$ bliti board-id          # report which sources this board offers and which wins
```

`board-id` probes only, so it is instant even where a TPM would win: reading that value means
regenerating a key inside the TPM, and probing does not.

A QR code for a board whose ID comes from a TPM or from one-time-programmable memory can only be
generated with the board to hand. A platform serial can be known in advance, so those codes can
be printed from a list gathered beforehand.

## Clients

The browser client is the one an operator uses, because it runs without being installed first.
Scanning the QR code with a generic phone camera opens the page with the payload in the fragment,
which is never sent to a server; an already-open page reads further codes with its own camera.
It needs a secure context, since neither the camera nor Web Bluetooth is available without one.

Run `just setup` once to install what a build needs, then `just build` to build it.

The `bliti` binary also carries the client half, for working on a device without a browser:

```console
$ bliti scan <code>       # find the device the code belongs to
$ bliti connect <code>    # open a channel to it
```

`<code>` is a QR code URL, its fragment, or the rendering printed beneath the code.

## Development

Building the daemon needs libdbus and, for the TPM board-ID source, tpm2-tss. On Debian and Ubuntu:

```console
$ sudo apt install libdbus-1-dev libtss2-dev
```

The TPM feature is on by default, and deliberately: which board-ID sources a build can see decides
which one wins the precedence, so a build without it derives a different secret on a board that has
a TPM. Cross-compiling to a board with no TPM development files to hand, use `--no-default-features`
and `--features vendored-dbus`; the daemon then refuses to derive on any board whose TPM it can see,
rather than quietly deriving the wrong secret.

```console
$ cargo test
$ cargo clippy --all-targets --all-features
```

What the system requires is specified in [`.workhorse/specs/`](.workhorse/specs/), starting with
[the overview](.workhorse/specs/overview.md). Each module names the spec it implements. The specs
are the description of what is correct; the code is one implementation of it.

## Licence

[GPL-3.0-or-later](./COPYING).
