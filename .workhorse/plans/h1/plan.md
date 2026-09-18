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

## Hazards to write carefully

The one that actually bit, and was not anticipated: a read must drain the decompressor before it pulls from the transport. The decompressor holds output of its own, so a caller asking for twelve bytes of yamux header leaves the frame body inside it with the transport's buffer already empty. Pulling from the transport first strands those bytes and reads a busy connection as idle. It deadlocks on the first frame whose header and body arrive in one chunk, which is every frame, so the whole stream layer hung until the read drained the decompressor first.

An inflating `poll_read` that has consumed input without producing output yet returns `Pending`, never `Ok(0)`. `Ok(0)` means end of stream to every `AsyncRead` caller, so returning it while waiting for the rest of a deflate block would tear the connection down at random. The same reasoning applies to `poll_write` returning `Ok(0)`, which callers read as a refusal to write.

`frame()` is currently shared between the two framing layers, and says so in its own comment. Once the Noise prefix is two bytes and the message prefix three, that sharing ends and framing has to carry the width rather than hardcode one. The `Reassembler` maximum follows the same width.

## yamux flow control

yamux counts uncompressed bytes, because it sits above the compressor. A message at the new maximum is far larger than the 256 KiB receive window and moves only if the receiver consumes and credits as it arrives. `read_message` already grows with what has arrived rather than reserving the claimed length, so the read path needs no change, only a test at the maximum.

Defaults stand. The initial window could not be lowered anyway, since `DEFAULT_CREDIT` is a constant in yamux 0.14 and the only window setting caps the connection total. The case for lowering is weak: the window counts uncompressed bytes while the link carries compressed ones, so a window's worth of queued backfill is around 25 KiB on the wire, about half a second at the send rate rather than the five and a half seconds the raw figure suggests.

`split_send_size` is the lever if live-feed latency behind a backfill shows up. At its 16 KiB default a frame of backfill is roughly 1.6 KiB compressed, about 33 milliseconds of link time.

## Absorbed from Q1

Q1, separating the transport and application framing layers, is cancelled and its work lands here. Its L1-settled position that application framing keeps a four-byte prefix and the 128 KiB ceiling is overturned: removing the ceiling is what lets the prefix carry the bound instead.

What carries over: handshake messages are bounded by the application ceiling rather than by 65535, which is the allocation an unauthenticated peer in range can induce; `framing.rs` uses a four-byte prefix where CHN specifies two; and `FrameTooLarge` with the `with_max` apparatus go dead once each bound is structural.

## Framing, after review

The two layers' widths turned out to be the only difference between their read/write pairs, so `framing.rs` carries one length-delimited pair parameterised by width (`write_delimited`, `read_delimited`) and the four functions in `stream.rs` became call sites of it. `decode_prefix` and `encode_prefix` replace the three hand-rolled copies of the "copy N bytes into a `[u8; 8]` at `8 - N`" idiom.

`frame()` refuses a message wider than its prefix instead of asserting in debug and truncating in release. A narrowed length is not a dropped message: the receiver reads the body as framing and every message after it is off by the difference, which is a desync a peer could steer wherever any part of a payload is influenced by what it sent. The structural bounds mean no conforming caller reaches it, but the check belongs in the shipped binary.

`write_delimited` writes the prefix and the body separately rather than copying both into one buffer, so a message at the new maximum does not double its own peak memory on the way out.

## Classification, after review

`is_peer_fault` matched `ConnectionError::Io` with an `InvalidData` kind, and no decompression failure is ever shaped that way: yamux reads the socket inside its frame decoder, so a read failure arrives as `ConnectionError::Decode(FrameDecodeError::Io(..))`. Every fault the layer exists to report was being logged as an ordinary ending, so CHN's "the receiver MUST report it" was not being met.

Both layers now attach a `ChannelError` to the `io::Error` they produce, and the classification walks the chain of causes for one. That is a typed contract rather than a convention about error kinds, so an unrelated transport failure reporting the same kind is not mistaken for a peer that cannot speak the protocol. Note that `io::Error`'s own `source` skips past the error it was built from, so the walk reads `get_ref` at each link rather than following `source` alone.

A transport that ends before the compressed stream does is now an error rather than a clean end of stream, since the partially consumed block can never be finished. It is not laid at the peer's door: a client walking out of range ends a connection the same way, so it carries no `ChannelError` and reads as an ordinary ending.

## Second review round

The end of an inflate stream was recorded when output ran out rather than when the status reported it, and `miniz_oxide` reports `StreamEnd` on the call that emits the tail of the final block, with bytes attached. The flag was therefore set one read late. It did not break anything, because `miniz_oxide` goes on reporting the end on every later call, so the late set always landed before the end-of-transport check and a graceful close was never reported as a truncation (`a_closed_stream_reads_as_a_clean_end_of_stream`). But the correctness of a clean shutdown rested on an unpinned codec behaviour, which is the same dependency the `dirty` guard exists to avoid. The flag now comes off the status, and the backend behaviour is pinned the way the flush one is.

The two framing layers pair their widths by type now: `Reassembler<PREFIX>` cannot be built against the other layer's `frame`, and a width the decoder could not handle fails the build rather than panicking on a slice underflow in release.

`read_delimited` distinguishes a prefix that never began from one begun and abandoned. `read_exact` reports `UnexpectedEof` for both, so both used to read as a complete exchange; the first byte is read on its own and only a genuinely empty stream is an ending. The body is read into the tail of the message rather than through an intermediate chunk, which drops a copy of every byte and takes an 8 KiB array out of the future the wasm client holds per stream.

`WriteBacklog` in `framing.rs` holds the partial-write loop both wrappers had a copy of.

### On splitting the write

The first round asked for the prefix and body to go out as two writes rather than one buffer; the second asked for a size threshold, on the grounds that a yamux `Stream` frames per write call and the extra 12-byte header taxes every sample. The framing claim is right: `Stream::poll_write` builds one `Frame::data` per call. The cost is not. Measured over the same 300-sample run as the ratio test, feeding one deflate context the byte sequences yamux produces in each case:

| | emitted | ratio |
| --- | --- | --- |
| prefix and body in one frame | 7,818 | 6.56x |
| prefix and body in two frames | 7,837 | 6.55x |

19 bytes over 300 messages, 0.06 bytes each, against about 26 bytes per message on the wire. The repeated header is what the shared context back-references away, and notifications are chunked from the compressed byte stream rather than per yamux frame, so the notification budget does not notice either. No threshold: a size branch on the write path is not worth 0.2% of the link.

## Third review round

`poll_close` recorded itself as finished whether or not the deflate stream had actually ended, and `drive_compress`'s "all input taken with room to spare" exit fires on the first call of a `Finish`, whose input is empty. No path through `miniz_oxide` was found that returns `Ok` short of `StreamEnd` there, so this was latent rather than live, but the cost of it had risen: since the truncation change, a missing tail is read at the far end as a fault. `drive_compress` now reports whether the stream ended, a finish keeps going while it is making progress, and a close that cannot finish fails instead of sending a stream with no end and calling it done.

`frame()` is gone. `NoiseStream` was its only caller, and it now puts the prefix and the ciphertext into the backlog directly, which drops a per-message allocation and a full copy of the ciphertext and reuses the buffer the drain just emptied. With the caller gone, `frame` had no non-test use and its const-generic width was pinning a path nothing reached.

`Reassembler` is fixed at the transport width for the same reason: it exists because GATT delivers a Noise message in attribute-sized chunks, and above the handshake yamux delivers a stream's bytes in order for `read_delimited` to read straight off. The width pairing stays where two layers genuinely instantiate it, on `read_delimited`/`write_delimited`.

`WriteBacklog` moved to `channel/write_backlog.rs`. It is a partial-write resume buffer with nothing to do with length prefixes, and having it in `framing` made `compress` depend on `framing` for something unrelated to framing. Its `replace` went with the move: it guarded "only when drained" with a debug assertion, which in a release build would have dropped a partially sent Noise frame and desynchronised the link. Nothing replaces now, so there is no precondition to guard.

`encode_prefix` returns `io::Result` rather than `Option`, so the width error is constructed once rather than at each call site.

`read_delimited` reads for the whole prefix instead of the first byte alone. The first-byte read told an absent prefix from an abandoned one, but at the cost of a second pass down the stack per message, and reading for all of it distinguishes the two just as well.

## Fourth review round

Making a truncated stream an error changed what the device's own teardown means, and the teardown was
a `driving.abort()`. Aborting drops the yamux connection without polling its close, so the deflate
stream was never finished and the client read the device's deliberate shutdown as a truncation, which
its `on_channel_closed` reported as a failure string. The clean path was unreachable in production:
only the unit test for it ever took it.

The driver now closes the connection when its handle is dropped. Nothing can open or accept a stream
without the handle, so the handle going is the signal that the connection has no further purpose, and
taking it from the drop means no teardown path can forget to close. yamux's own close sends its term
frame and then closes the socket, which reaches the compressor as the finish. `session::run` drops the
handle and waits on the driver, bounded at 500 ms: the device asks to go back on the air only once
`run` returns, so a client that has already vanished must not hold it off (ADV). Falling through the
bound costs only the clean ending, which the client reports as an ending either way.

`dropping_the_handle_closes_the_connection_cleanly` holds both ends' drivers and asserts neither ends
with an error. Both streams stay open through the teardown on purpose: a stream dropped at the same
moment queues a reset, and whether that reset makes it out before the transport goes is a race that
says nothing about how a close is read. It surfaced as a `BrokenPipe` on the client's write while the
test was being written, which is worth knowing is possible but is an ordinary ending, not a fault.

`drive_compress` split into `compress_input` and `flush_compress`, each returning what one caller
wants, over a `deflate` that makes one compressor call. The `type StreamEnded = bool` alias is gone: it
gave no safety a bare bool did not. The finish special case went with the split rather than moving
into it, because a flush with output room to spare has emitted everything it had whatever it was asked
to do, and for a finish that means the stream could not be completed.

## Steps

- [x] Split `frame()` and `Reassembler` so each layer carries its own prefix width, with the transport at two bytes and the message layer at three.
- [x] Drop `MAX_MESSAGE`, the ceiling check in `read_message`, and the `FrameTooLarge` variant and `with_max` apparatus left dead by structural bounds.
- [x] Add the compression layer in `bliti-core`, as an `AsyncRead + AsyncWrite` wrapper over `NoiseStream` with one context per direction (`channel/compress.rs`, `CompressStream`).
- [x] Map the wrapper's `poll_flush` onto a zlib sync flush. A redundant flush does *not* emit nothing under `miniz_oxide`, so the `dirty` guard carries the property instead (see the flush section above).
- [x] Wire the wrapper inside `multiplex()` so neither caller can compose an uncompressed channel.
- [x] Add `flate2` with `default-features = false` and the `rust_backend` feature to `bliti-core`.
- [x] Carry the closed-channel report and the offer to reconnect in the web application (driver-end surfaces through `Channel::connect`'s `on_channel_closed`, routed to `onDisconnected`).
- [x] Re-measure the wasm bundle and record the delta.
- [x] Consolidate the two framing layers onto one width-parameterised read/write pair, and refuse an over-wide message rather than narrowing its length (see above).
- [x] Carry a `ChannelError` through both layers so a fault is classified by type rather than by error kind, and report a truncated stream rather than reading it as a clean end (see above).
- [x] Record the end of an inflate stream from the status rather than from output running out, and pin the backend behaviour a late flag was relying on (see above).
- [x] Pair each framing layer's width by type, read a message body in place, and tell an abandoned prefix from an absent one (see above).
- [x] Finish the deflate stream before recording a close as finished, and fail the close rather than leaving the tail unwritten (see above).
- [x] Drop `frame()`, fix `Reassembler` at the transport width, and move `WriteBacklog` to its own module (see above).
- [x] Close the connection on a deliberate teardown rather than aborting the driver, so the far end reads the clean ending it is (see above).

## Measurement

The baseline of 1,081,989 raw and 293,340 gzipped is the pre-`wasm-bindgen` cargo output (`target/wasm32-unknown-unknown/release/bliti_web.wasm`). Measured the same way after this card:

| stage | raw | gzipped |
| --- | --- | --- |
| pre-bindgen, before | 1,081,989 | 293,340 |
| pre-bindgen, after | 1,127,893 | 311,721 |
| delta | +45,904 | +18,381 (about 5.9%) |

The delta matches the predicted +45,692 raw and +18,880 gzipped, so `flate2` on `miniz_oxide` costs what it was measured to. The shipped bundle, after `wasm-bindgen` strips unused exports, is 695,038 raw and 235,335 gzipped.
