# Compress the channel

Scenarios verifying H1. Behaviour is in [CHN](../../specs/channel.md) under "Compression" and "When the channel closes", with consequences in [MSG](../../specs/messages.md) and [SEC](../../specs/security.md).

## The shared context

- [ ] A session's first message and a steady-state message both decode to the same JSON they were sent as, cold context and warm. Verifies spec: CHN
- [ ] Two streams running at once share one context, and both decode. A message on the second stream benefits from what the first put in the context. Verifies spec: CHN
- [ ] A context is not reset between messages or between streams: a message that decodes in sequence fails to decode against a fresh context. Verifies spec: CHN
- [ ] Compression carries client to device as well as device to client. Verifies spec: CHN
- [ ] No preset dictionary is used, so a decompressor built with no dictionary reads the stream. Verifies spec: CHN

## Flushing

- [ ] A message written with nothing following it is readable by the receiver without further input. This is the flush guarantee, and the most important regression test for the design. Verifies spec: CHN
- [ ] A burst written back to back arrives whole, whether the sender flushes per message or defers while it has more to write. Verifies spec: CHN
- [ ] A flush with nothing written since the last flush emits no bytes, so an idle connection stays silent under the driver's per-iteration flush. Pinned against `miniz_oxide` rather than assumed from zlib.

## Faults

- [ ] A corrupt or truncated compressed stream closes the connection, and the failure is reported: logged on a device, surfaced to the operator on a client. Verifies spec: CHN
- [ ] A malformed message closes only the stream it arrived on, leaving the connection and other streams alive. The two faults are distinguishable from one another. Verifies spec: CHN, MSG
- [ ] A decompressor waiting for the rest of a block returns pending rather than zero, so a partial block is not read as end of stream.

## Message framing

- [ ] A message larger than the former 128 KiB ceiling round-trips. Verifies spec: MSG
- [ ] A message at the three-byte prefix's maximum round-trips, and a sender cannot express one larger. Verifies spec: MSG
- [ ] A message length prefix is three bytes and a Noise message length prefix is two, so neither layer reads the other's framing. Verifies spec: MSG, CHN
- [ ] A handshake message is bounded by what its own two-byte prefix expresses, not by an application ceiling. Verifies spec: CHN

## Client

- [ ] The application reports a channel that has closed and offers to open it again. Verifies spec: CHN
- [ ] The round trip holds in the wasm build, not only in native tests.

## Measurements

- [ ] Encoder and decoder contribution to the wasm bundle is measured and recorded against the 1,081,989 raw and 293,340 gzipped it stood at before this card.
- [ ] A recorded run of samples through one context reaches a ratio in the region the card measured, so a regression in shape or flushing shows up as a ratio that has fallen.
