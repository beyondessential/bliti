# Compress the channel

Implementation notes and build steps for H1. Behaviour is in [CHN](../../specs/channel.md) under "Compression" and "When the channel closes", with the consequences in [MSG](../../specs/messages.md) and [SEC](../../specs/security.md). Reasoning, measurements and rejected options are in the working doc at `.workhorse/working-docs/h1/working-doc.md`.

## Where the layer goes

`NoiseStream` presents `AsyncRead + AsyncWrite` and yamux runs on it. The compressor is a wrapper between the two: yamux's bytes go through the compressor, then into Noise framing. Compress then encrypt, necessarily, since encrypted bytes do not compress.

Wire it inside `multiplex()` rather than leaving each caller to compose it, so no call site can assemble an uncompressed channel. That makes the always-on property structural instead of a convention the daemon and the web client each have to remember.

## Codec

`flate2` on its pure-Rust `miniz_oxide` backend, measured against the alternative as cdylibs for `wasm32-unknown-unknown` under the workspace release profile:

| backend | raw added | gzipped added | share of the gzipped bundle |
| --- | --- | --- | --- |
| `flate2` on `miniz_oxide` | 45,692 B | 18,880 B | about 6% |
| `flate2` on `zlib-rs` | 116,065 B | 45,795 B | about 16% |

The bundle before this card is 1,081,989 bytes raw and 293,340 gzipped. `zlib-rs` costs 2.4x for speed on large buffers, which is not this workload: messages are hundreds of bytes and the device has time to spare on each.

The browser's own compression cannot serve. `CompressionStream` has no mid-stream flush, so its output is only guaranteed once the writable side closes, which suits per-message compression and not a long-lived context. `DecompressionStream` would serve the receiving direction, but an encoder is needed in wasm regardless, so keeping both in `bliti-core` gives one implementation on both ends with no JS boundary inside an `AsyncWrite` pipeline.

Compression level is not wire content, since any level decodes the same, so it is a per-end choice. State runs to a couple of hundred kilobytes for the deflate side and a few tens for the inflate side, one of each per connection per end.

## How the flush rule reaches the compressor

A yamux `Stream`'s flush only pushes its frame to the connection driver. What reaches the compressor is the driver's own `poll_flush` on the socket, which `connection.rs` calls on every iteration of its poll loop, after draining pending frames and before parking. So the flush lands once the driver has nothing further to send, which is the behaviour wanted: queued writes ride into the same deflate run and the bytes go out when the queue empties.

`write_message` already flushes after each message, so the message-boundary default needs no new call.

The driver flushing every iteration would be a problem if a redundant sync flush emitted bytes, because an idle connection would dribble empty stored blocks and spend both the notification budget and the device's radio. The assumption going in was that zlib emits nothing for a sync flush with no pending input; measured, `miniz_oxide` does the opposite and emits a five-byte empty stored block every time (pinned by `miniz_emits_on_a_redundant_sync_flush`). So the wrapper carries a `dirty` flag and runs a sync flush only when something has been written since the last one: idle silence is a property of the wrapper, not of the backend, and would hold even against a backend that behaved as first assumed.

## Two hazards to write carefully

An inflating `poll_read` that has consumed input without producing output yet returns `Pending`, never `Ok(0)`. `Ok(0)` means end of stream to every `AsyncRead` caller, so returning it while waiting for the rest of a deflate block would tear the connection down at random.

`frame()` is currently shared between the two framing layers, and says so in its own comment. Once the Noise prefix is two bytes and the message prefix three, that sharing ends and framing has to carry the width rather than hardcode one. The `Reassembler` maximum follows the same width.

## yamux flow control

yamux counts uncompressed bytes, because it sits above the compressor. A message at the new maximum is far larger than the 256 KiB receive window and moves only if the receiver consumes and credits as it arrives. `read_message` already grows with what has arrived rather than reserving the claimed length, so the read path needs no change, only a test at the maximum.

Defaults stand. The initial window could not be lowered anyway, since `DEFAULT_CREDIT` is a constant in yamux 0.14 and the only window setting caps the connection total. The case for lowering is weak: the window counts uncompressed bytes while the link carries compressed ones, so a window's worth of queued backfill is around 25 KiB on the wire, about half a second at the send rate rather than the five and a half seconds the raw figure suggests.

`split_send_size` is the lever if live-feed latency behind a backfill shows up. At its 16 KiB default a frame of backfill is roughly 1.6 KiB compressed, about 33 milliseconds of link time.

## Absorbed from Q1

Q1, separating the transport and application framing layers, is cancelled and its work lands here. Its L1-settled position that application framing keeps a four-byte prefix and the 128 KiB ceiling is overturned: removing the ceiling is what lets the prefix carry the bound instead.

What carries over: handshake messages are bounded by the application ceiling rather than by 65535, which is the allocation an unauthenticated peer in range can induce; `framing.rs` uses a four-byte prefix where CHN specifies two; and `FrameTooLarge` with the `with_max` apparatus go dead once each bound is structural.

## Steps

- [x] Split `frame()` and `Reassembler` so each layer carries its own prefix width, with the transport at two bytes and the message layer at three.
- [x] Drop `MAX_MESSAGE`, the ceiling check in `read_message`, and the `FrameTooLarge` variant and `with_max` apparatus left dead by structural bounds.
- [x] Add the compression layer in `bliti-core`, as an `AsyncRead + AsyncWrite` wrapper over `NoiseStream` with one context per direction (`channel/compress.rs`, `CompressStream`).
- [x] Map the wrapper's `poll_flush` onto a zlib sync flush. A redundant flush does *not* emit nothing under `miniz_oxide`, so the `dirty` guard carries the property instead (see the flush section above).
- [x] Wire the wrapper inside `multiplex()` so neither caller can compose an uncompressed channel.
- [x] Add `flate2` with `default-features = false` and the `rust_backend` feature to `bliti-core`.
- [x] Carry the closed-channel report and the offer to reconnect in the web application (driver-end surfaces through `Channel::connect`'s `on_channel_closed`, routed to `onDisconnected`).
- [x] Re-measure the wasm bundle and record the delta.

## Measurement

The baseline of 1,081,989 raw and 293,340 gzipped is the pre-`wasm-bindgen` cargo output (`target/wasm32-unknown-unknown/release/bliti_web.wasm`). Measured the same way after this card:

| stage | raw | gzipped |
| --- | --- | --- |
| pre-bindgen, before | 1,081,989 | 293,340 |
| pre-bindgen, after | 1,127,893 | 311,721 |
| delta | +45,904 | +18,381 (about 5.9%) |

The delta matches the predicted +45,692 raw and +18,880 gzipped, so `flate2` on `miniz_oxide` costs what it was measured to. The shipped bundle, after `wasm-bindgen` strips unused exports, is 695,038 raw and 235,335 gzipped.
