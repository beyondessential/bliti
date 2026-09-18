# Compress the channel

Scenarios verifying H1. Behaviour is in [CHN](../../specs/channel.md) under "Compression" and "When the channel closes", with consequences in [MSG](../../specs/messages.md) and [SEC](../../specs/security.md).

## The shared context

- [x] A session's first message and a steady-state message both decode to the same JSON they were sent as, cold context and warm. Verifies spec: CHN
- [x] Two streams running at once share one context, and both decode. A message on the second stream benefits from what the first put in the context. Verifies spec: CHN
- [x] A context is not reset between messages or between streams: a message that decodes in sequence fails to decode against a fresh context. Verifies spec: CHN
- [x] Compression carries client to device as well as device to client. Verifies spec: CHN
- [x] No preset dictionary is used, so a decompressor built with no dictionary reads the stream. Verifies spec: CHN

## Flushing

- [x] A message written with nothing following it is readable by the receiver without further input. This is the flush guarantee, and the most important regression test for the design. Verifies spec: CHN
- [x] A burst written back to back arrives whole, whether the sender flushes per message or defers while it has more to write. Verifies spec: CHN
- [x] A flush with nothing written since the last flush emits no bytes, so an idle connection stays silent under the driver's per-iteration flush. `miniz_oxide` in fact emits on a redundant sync flush, so this is carried by the wrapper's `dirty` guard, and both are pinned.
- [x] `miniz_oxide` reports the end of a stream with output attached and keeps reporting it afterwards. Pinned, because the wrapper deliberately does not lean on the second half of that.

## Faults

- [x] A corrupt compressed stream closes the connection, and the failure is reported: logged on a device, surfaced to the operator on a client. Verifies spec: CHN
- [x] A corrupt compressed stream reaches the host classified as a fault in the peer, through yamux, which wraps a socket read failure as a decode error rather than an I/O one. Verifies spec: CHN
- [x] A transport that ends before the compressed stream does closes the connection rather than reading as a complete exchange, and is not laid at the peer's door: a client walking out of range ends a connection the same way. A transport that ends before a byte ever arrives is an ordinary end of stream. Verifies spec: CHN
- [x] A malformed message closes only the stream it arrived on, leaving the connection and other streams alive. The two faults are distinguishable from one another. Verifies spec: CHN, MSG
- [x] A decompressor waiting for the rest of a block returns pending rather than zero, so a partial block is not read as end of stream.
- [x] A peer that closes its side properly reads as a clean end of stream, and the same end however often it is asked for. The call that completes the zlib stream carries output with it, so the end is recorded from the status rather than from output running out. Verifies spec: CHN
- [x] A message whose length prefix, or whose body, stops short of what the peer began reads as a truncation rather than a complete exchange. A stream that never began a message is an ordinary ending. Verifies spec: MSG
- [x] A close that could not bring the deflate stream to its end fails rather than recording itself as finished, since the peer would read the missing tail as a truncation.
- [x] A write after close is a closed pipe, distinct from the write path's unreachable-invariant error.
- [x] A backlog drains in order across a transport that accepts a few bytes per call, and a transport that accepts nothing fails the write rather than spinning.
- [x] A deliberate teardown closes the connection rather than dropping it, so the far end reads the clean ending it is and the operator is not told of a fault that did not happen. Verifies spec: CHN

## Message framing

- [x] A message larger than the former 128 KiB ceiling round-trips. Verifies spec: MSG
- [x] A message at the three-byte prefix's maximum round-trips, and a message larger than a prefix can express is refused to the sender rather than narrowed to fit. Verifies spec: MSG
- [x] A message length prefix is three bytes and a Noise message length prefix is two, so neither layer reads the other's framing. Verifies spec: MSG, CHN
- [x] A handshake message is bounded by what its own two-byte prefix expresses, not by an application ceiling. Verifies spec: CHN

## Client

- [x] The application reports a channel that has closed and offers to open it again. Verifies spec: CHN
- [ ] The round trip holds in the wasm build, not only in native tests.

## Measurements

- [x] Encoder and decoder contribution to the wasm bundle is measured and recorded against the 1,081,989 raw and 293,340 gzipped it stood at before this card.
- [x] A recorded run of samples through one context reaches a ratio in the region the card measured, so a regression in shape or flushing shows up as a ratio that has fallen.
