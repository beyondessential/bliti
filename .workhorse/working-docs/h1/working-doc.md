---
status: draft
---

# Compress application messages on the channel

Make compression an always-on property of the channel while the protocol is still free to change, so a device can say more without spending the link to say it.

## Where it sits

Settled: beneath yamux, on the Noise byte stream, one compressor per direction for the life of the connection.
Not per application message, and not per yamux stream.

The measurements decide it.
Over 300 samples of a real device's readings, 941,523 bytes raw: each message compressed independently gives 293,932 (3.2x), all messages through one streaming context gives 88,578 (10.6x).
The redundancy lives across messages rather than within one. Every sample repeats the same nine reading labels, twenty detail labels, units and state strings, and only a shared context sees it.
Steady state, a live sample costs 152 bytes on the wire against 2,928 raw; the first costs 981 because the context is cold.

Per-stream compression is rejected for the same reason at smaller scale: the live feed and a backfill feed carry identical labels, so two separately-warmed contexts give most of the gain away.

A connection-level context also pays off against a pattern MSG already asks for.
A client SHOULD drop its subscriptions when the operator is no longer looking and open them again when they are, and the connection outlives that.
With the context on the connection, a reopened subscription starts warm and costs 152 bytes a sample immediately; with it on the stream, every reopening pays the cold start again.

What this costs, accepted knowingly: a receiver can no longer skip an unknown message without decompressing it, because the context has to stay fed.
In practice a receiver has to decompress to learn the type anyway, and skipping after that is cheap.

## Behaviour

Compression is unconditional.
There is no capability exchange, no negotiation, no state, and no uncompressed code path.
Both ends are at the same version marker before a channel exists, and [VER](../../specs/version.md) already covers the transport and streams of CHN, so the marker is what says the channel is compressed.

Both directions compress, symmetrically.
Client-to-device traffic today is hellos and subscribes, but a feature that uploads a file would want the same treatment, and adding a direction afterwards is the negotiation this card exists to avoid.

The wire format is zlib, RFC 1950: deflate under a two-byte header, one stream per direction opened when the connection opens.

A sender MUST NOT leave a message it has finished writing unreadable by the receiver.
Flushing at each message boundary satisfies this, and is the starting point.
A sender MAY defer a flush while it still has more to write, which coalesces a burst, so that a backfill of many samples compresses as one run, provided the guarantee above holds when it goes idle.

There is no preset dictionary.
The context warms itself.

A message has no size ceiling.
The 128 KiB ceiling of [MSG](../../specs/messages.md) goes, and with it the fault a receiver raises on an over-long message.
A message's length prefix narrows from four bytes to three, so a message is at most 16 MiB and the bound is structural rather than a rule.
This is the argument CHN already makes for its own two-byte prefix: a prefix that expresses exactly the range its content can occupy cannot ask a receiver to buffer more than the maximum, and so needs no rule refusing one.

A decompression failure is a fault in the peer and closes the connection.
The context is shared across every stream and unrecoverable once it has diverged, so there is nothing to keep alive.
This sits alongside MSG's existing rule rather than replacing it: a malformed message still closes only the stream it arrived on.
Because the compressed bytes sit inside the Noise transport, a failure can only mean the peer emitted a broken deflate stream. Neither corruption nor an attacker can produce one.

A client SHOULD offer the operator a way to reconnect after the connection closes, rather than leaving them on a dead view.

[SEC](../../specs/security.md) gains an entry under "Where the guarantees stop": message sizes on the link carry some signal about what is being said, because compressed size correlates with content.
CRIME does not apply, since neither direction mixes attacker-chosen input with a secret and the presence token is never sent, but the limit is real and would otherwise go unstated.

### Why the ceiling goes

Two of its premises stopped holding.

It bounded what the link must carry, and compression breaks the relation between a message's JSON size and its size on the wire. Under stream placement a message has no compressed size of its own at all, so a ceiling counting compressed bytes cannot be written.

Its stated reason was that one large message "denies the connection to everything else for as long as it takes", which yamux already prevents: a large message is carried as frames interleaved with every other stream's.
What remains bounding link occupancy is the send rate of CHN, 200 notifications in any one-second window, which is the ceiling that was doing the real work.

## Where this lands in the specs

CHN splits into a folder.
It is carrying handshake, transport, send rate, peripheral role and streams already, and compression would be the sixth subject in one file.

Proposed shape, to be settled when the split is written:

| file | carries |
| --- | --- |
| `channel/overview.md` | keeps the id `CHN`: borrowed terms, the layering, and what the channel is |
| `channel/authentication.md` | the Noise handshake and what it proves |
| `channel/transport.md` | GATT, the two characteristics, framing, send rate, peripheral-only |
| `channel/streams.md` | yamux |
| `channel/compression.md` | this card |

`CHN` stays on the overview because an id never changes.
The new siblings each need their own id, and every existing reference to `channel.md`, in MSG, VER, SEC and the `spec: CHN` comments in the code, is repointed as part of the split.

MSG loses its size ceiling and the fault that goes with it, and narrows its length prefix to three bytes.
SEC gains the entry described above.
VER's list of what the marker covers already says "the handshake, framing, transport and streams of CHN", which the split and the new section both need to stay true to.

## Implementation options

The pipeline is `NoiseStream` (`AsyncRead + AsyncWrite`) with yamux above it, so the compression layer is a wrapper sitting between them: yamux's bytes go through the compressor, then into Noise framing.
Compress then encrypt, necessarily, because encrypted bytes do not compress.

`poll_flush` is the natural carrier for the flush rule.
yamux flushes the stream beneath it when its send queue drains, so mapping the layer's flush onto a zlib sync flush gives the burst-coalescing optimisation for free, without the compressor ever needing to see a message boundary.
A sync flush emits an empty stored block, four or five bytes, which is what makes flushing per message affordable.

### Codec: a wasm codec, not the browser's

The browser's built-in compression cannot do this job.
`CompressionStream` has no mid-stream flush: output is only guaranteed once the writable side is closed, so it can drive per-message compression and not a long-lived context.
`DecompressionStream` is unaffected and would serve the receiving direction, but compressing client-to-device needs a real encoder regardless.

Since an encoder has to be in wasm anyway, putting the decoder there too keeps the whole layer in `bliti-core`, identical on both ends, with no JS boundary in the middle of an `AsyncWrite` pipeline.
Candidates are `flate2` on its pure-Rust `miniz_oxide` backend, or `zlib-rs`; both build for `wasm32-unknown-unknown`.
Encoder-plus-decoder code size is the number to measure before committing. The browser is the tight constraint; the Pi has room to spare.

Compression level is not wire content: any level decodes the same, so it stays out of the spec and is a per-end choice.
Deflate at the full 32 KiB window costs a few hundred kilobytes of state per context, two per connection, which neither end will notice.

### A framing divergence to fix on the way past

CHN requires a Noise message to be prefixed with its length as two bytes, big-endian.
`framing.rs` uses four, with a 65535-byte maximum, so the top two bytes are always zero.
The spec's own note argues for two specifically, since a prefix expressing exactly the range a Noise message can occupy needs no rule refusing an over-large one, so the code is what moves.
It is the same area of code and the same kind of change as narrowing the message prefix, so it rides along on this card.

### yamux flow control now sits above compression

yamux counts uncompressed bytes, because it is above the compressor.
Its default receive window is far smaller than the 16 MiB a message may now be, so a large message only moves if the receiver consumes and credits as it arrives, and the layer reassembling a message accepts it in pieces rather than waiting for the whole.
Worth checking against the current wiring, which was written when no message could exceed 128 KiB.

## Trade-offs

Compression must not become a way of hiding bad shape.
Gzip alone would have masked D1's original bug: the old message compressed 22.2x precisely because what it squeezed out was reading descriptions repeated against every point, which is the redundancy reshaping removed structurally.
The 865 kB window would have crossed the link at ~19 kB and nobody would have found out.
The case here is the 4.1x that remains on a message already shaped properly, and shaping still comes first in every case.

### Why no preset dictionary

A preset dictionary only ever helps the first message.
After that the real content is in the 32 KiB window and the dictionary is dead weight.
So its value is bounded by how much of the first message's vocabulary it contains, and a dictionary safe to fix in the protocol contains almost none of it.

Measured on a synthesised `system-sample` shaped as `readings.rs` defines it, at nine readings, twenty detail entries and 2,188 bytes framed, with every variant compressing identical bytes:

| dictionary | size | first message | 300-sample session |
| --- | --- | --- | --- |
| none | 0 | 629 B | 58,791 B |
| base envelope vocabulary | 123 B | −7 B | −9 B |
| generic JSON punctuation and envelope | 201 B | −15 B | −17 B |
| feature vocabulary, for contrast | 624 B | −222 B | −224 B |

A generic dictionary saves fifteen bytes once per session, which is not worth an artefact both ends must hold identical copies of.

The contrast row is the one that explains why this is not simply a matter of picking a better dictionary.
The savings live entirely in reading labels and units, which is feature vocabulary.
Feature vocabulary must not go in, because VER's marker covers the transport: a dictionary is version-critical, and features are expressly free to grow without moving the marker.
A dictionary holding only base-protocol vocabulary would be safe, since it churns only when MSG's envelope changes, which already moves the marker, and is worth seven bytes.

These are synthetic figures, and the synthesised sample is smaller than the real one the card's comment measured (2,188 against 2,928 bytes), so treat the magnitudes as indicative.
The conclusion does not rest on them: it rests on the dictionary being confined to the first message and on feature vocabulary being ineligible.

A large compressed message can expand to a much larger one and nothing in the protocol caps that.
This is a consequence to be aware of rather than a way in: reaching the channel at all means completing the handshake of CHN, which means holding the sticker secret, and a peer that holds it is the operator.

## Open questions

- [ ] **The exact shape of the CHN split**, and an id for each new sibling. Sketched above, to be settled when the split is written rather than now.
- [ ] **What a 16 MiB message means for a future upload feature.** Three bytes is a generous per-message bound, but a large upload wants its own stream carrying many messages rather than one enormous one. Nothing here forbids that; it may be worth saying so where the prefix is specified, so the bound does not read as an invitation.
- [ ] **Precision rounding.** The same 300 samples at full `f64` compress to 88,578; rounded to four decimal places, 47,760. Nearly half, and independent of everything here. It belongs to whichever card owns the sample shape rather than this one, but it should not get lost.

## Testing notes

- A session's first sample and a steady-state sample both decode to the same JSON, cold context and warm.
- A message written with nothing following it is readable by the receiver without further input. This is the flush guarantee and the single most important regression test for the design.
- A live feed and a backfill feed running at once share one context, and both decode.
- A message larger than the old 128 KiB ceiling round-trips.
- A message at the three-byte prefix's maximum round-trips, and a sender cannot express one larger.
- A client-to-device message decodes on the device, covering the symmetric direction.
- A truncated or corrupt compressed stream closes the connection, and a malformed message on one stream still closes only that stream. The two faults are distinguishable.
- A Noise message carries a two-byte length prefix, and the reassembler refuses a longer claim structurally.
- The round trip holds in the wasm build, not only in native tests.
- Encoder-plus-decoder contribution to the wasm bundle is measured and recorded.
