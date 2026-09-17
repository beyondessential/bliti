# Rewrite the older specs in the standards voice

Working notes for L1. `channel.md` is the pilot; the voice and conventions settled there roll out to the other eight.

## The target voice

Grounded in RFC 9000 and the Noise Protocol Framework, not in BLI-MSG or BLI-SYS. Those two carry RFC 2119 keywords but remain essay prose, so they are not the target.

One requirement per sentence, subject-verb-object. The keywords carry the force. Non-normative material is set apart so the binding text reads top to bottom uninterrupted.

### Keeping a justification

A justification survives only where it constrains a re-implementer's choice. The test: if cutting it would let someone build a plausible non-interoperable variant, keep it. Otherwise cut it, including most UX and design motivation.

Load-bearing, so kept: the argon2id parameters in BLI-KEY, where changing any of them orphans every code already printed. Without that, a re-implementer tunes them.

Not load-bearing, so cut: BLI-SYS's reason for sending readings before the history window. A client renders correctly whatever the motive.

Survivors go in GFM alert blocks (`> [!NOTE]`), never inline on the normative sentence. The blocks render distinctly on GitHub and are machine-detectable, so a lint can assert that no normative keyword ever appears inside one.

### The spec does not assert its own authority

Deleted from `channel.md`: "Everything this spec states about the wire is contract. An independently written client that follows it interoperates with a device that follows it."

A spec that announces it is binding is compensating for prose that does not read as normative. The keywords and the precision are the claim. Needing to state it was itself the symptom.

The same logic removes sentences written *about* other sentences in the spec. "The two ceilings are independent, and a device MUST satisfy both" existed only because the two ceilings shared one sentence; written as two requirements, it evaporates.

The class also points outward. `channel.md` closed by enumerating what BLI-MSG specifies, which duplicates that spec's headings and rots when they change. Replaced by a pointer naming the subject rather than the contents: a cross-reference says what the target is about, and does not summarise what it says.

The same enumeration also sat inside `messages.md` as self-description, so the list existed in two places and was load-bearing in neither. Where a cross-reference and its target both recite the same list, the headings are the real list and both copies go.

Outstanding instances of the class: `messages.md` 8, 11-12; `system-info.md` 10, 11, 13-14, 205; `overview.md` 46. Four flavours — contract boasts, table-of-contents openers restating the headings, defensive commentary ("properties of the design rather than gaps in it", "This note is required"), and cross-references reciting their target. In each case the buried real requirement is already stated normatively elsewhere, so they delete cleanly.

### A rationale must outlive the capability it cites

`channel.md` grounded its peripheral-only requirement in "a browser cannot drive pairing at all". The requirement is architectural: pairing is irrelevant because authentication comes from the handshake, not from the link. Tying it to what browsers can do today gives the rationale an expiry date, and a reader who meets it after browsers gain pairing, or after a native client ships, concludes a still-correct rule is obsolete and removes it. Replaced with the architectural reason plus the failure mode, which hold either way.

Two cases, and only the first is a defect:

- Architectural requirement, contingent rationale. The rationale is wrong and is replaced.
- Contingent requirement, contingent rationale. Honest, but usually a durable mechanism sits underneath and the platform framing is decoration. `discovery.md` justifies putting the service UUID in the advertisement by "the only filtering some client platforms offer"; the durable reason is that a scan filter applies to advertisement data rather than to the scan response, which is a property of BLE and not of anyone's platform.

Sweep results: `discovery.md` 15 and `sticker.md` 18 restate durably as above. `system-info.md` 8 and `web-app.md` 8 are feature motivation rather than constraint, and go under the justification test. `web-app.md` 30 is a real requirement and only needs the terminology fix.

## Conventions settled

### Requirements notation lives in BLI

BCP 14 keyword notation sits in `overview.md`, worded "in bliti's specifications" rather than the boilerplate "in this document", because it governs all nine files from the root instead of being restated in each.

### Two tiers of glossary

**Global, in BLI.** Terms bliti coins: board ID, presence secret, advertised handle, QR code. One `###` per term so each has a stable anchor. Uses elsewhere deep-link to it, e.g. `[presence secret](overview.md#presence-secret)`. Convention is to link the first occurrence per spec.

**Per-spec, in the spec that borrows them.** Terms imported from other standards, as a table near the top so a reader meets them before use. A borrowed term used in one spec does not belong in the root glossary.

The division is coined versus imported.

`overview.md`'s chain section keeps wayfinding and the reproducibility property, and no longer defines the terms.

### BLI is the map, SEC is the threat model

BLI becomes a terse entryway and a meta spec: what the project is, the requirement keywords, the external documents, the global glossary, and the map of how the specs relate. A casual reader can start and stop there.

SEC owns the security properties and their limits. Each property names the mechanism that upholds it, and the mechanism specs carry the requirements. CHN keeps only what a re-implementer of the handshake needs.

This resolves a status conflict the note convention created. The same security claims were body text in BLI and non-normative `[!NOTE]` commentary in CHN, so one claim was both binding and disclaimed depending on which file you read.

It also fixed an omission rather than only moving text. That a recorded handshake serves the offline board-ID search as well as a recorded advertisement does existed only inside a CHN note, so anyone reading BLI's guarantees-stop section as the threat model got an incomplete answer.

Still to fold into SEC when those specs are rewritten: `key-schedule.md`'s "What the cost achieves", which is a security-properties discussion sitting inside a derivation spec, and `discovery.md`'s "Address privacy".

SEC takes the unprefixed id now, ahead of the re-ID sweep, so cross-references to it read `[SEC](security.md)` while the rest still read `[BLI-CHN](channel.md)`. The inconsistency resolves itself when the sweep lands.

### VER and DEV take BLI's remaining requirements

Making BLI non-normative left two sections with nowhere to sit, both carrying real requirements.

**VER** takes the base protocol version, and absorbs `key-schedule.md`'s Versioning section so one marker has one description of its scope. KEY now defers to it in a line.

That absorption resolved a contradiction. BLI said the marker covers every layer including the handshake and the message encoding; KEY said "Nothing else is a new version" beyond what changes the secret. A handshake change does not change the secret, so the two rules disagreed about whether it moves the marker. VER resolves toward the broad scope, because the marker is the only version signal that exists before a connection does: were it to cover only the secret, two peers running incompatible handshakes would derive matching handles, recognise each other, and fail with nothing to tell an operator. KEY's narrower claim was written from the key schedule's vantage and did not account for the layers above it.

**DEV** takes reporting to standard error. It is thin at one requirement, and will grow: `key-schedule.md`'s memory-headroom check and dead-cache reporting, `discovery.md`'s bound on recording failed attempts, and `channel.md`'s Bluetooth stack prerequisite are all device-operation requirements currently embedded in mechanism specs. They fold in as those specs are rewritten, the same way SEC's remaining sources do.

### BLI does not index the specs

The chain and the layer table were both emergent: properties a reader obtains by reading the specs, restated in BLI for no reason. The chain was a table of contents for the whole corpus, which is the cross-reference-reciting-its-target defect scaled up. The layer table carried normative language about layer independence that CHN already establishes by specifying the stack.

Both deleted. BLI is the intro, the requirement keywords, the external documents, and the global glossary, and nothing else.

This reverses an earlier finding of mine. `system-info.md` being linked from no other spec looked like a hole in the map, and a reachability check looked worth adding alongside the link checker. With no map there is nothing for a spec to be missing from: specs cross-reference each other where one requirement depends on another, and the directory is the list. A hand-maintained index rots; the filesystem does not.

One line in the deleted chain was a real property rather than a restatement, and moved to SEC: there is no fleet key and no per-device record, so compromising one device yields nothing about any other.

### A term is defined before it is used

VER opened by restating a system property with no normative force, and used "marker" throughout without anything defining it. The version marker is a coined term appearing in STK, ADV, KEY, WEB and VER, so it joined the global glossary, and VER now opens with the requirement instead: a client and a device MUST NOT act on any version other than the marker.

The general rule: a term the specs coin is defined in BLI's glossary before any spec leans on it, and a term they import is defined in the borrowing spec's own terms table.

### presence secret, not sticker secret

Named by role, not by the artefact carrying it. Holding it proves the holder read the code on the device.

"presence key" was considered; "secret" avoids collision with the session key the handshake produces.

### The QR code is the invariant, the substrate is not

The QR code binds three things and so belongs in the specs: the client needs a camera, manufacturing needs to print at sufficient resolution, and the toolkit needs to generate one. What it is printed on binds nothing.

`key-schedule.md` already reasoned this way, calling the URL and the human-readable rendering "carriers rather than payload", while naming the payload after the adhesive. The rename resolves that contradiction.

The substrate is left unconstrained by saying nothing about it, rather than by stating a non-requirement. Any genuine optical constraint (contrast, minimum module size) belongs in BLI-STK as a positive requirement.

### External normative references are cited by name and link

Pinned where the revision is the stability guarantee: Noise Protocol Framework revision 34, because its PSK token semantics are revision-specific.

Unpinned where the standard is layered and stable across versions: Bluetooth Core Specification Volume 3 Part G (GATT) and Part F (ATT). Pinning 6.3 would imply a device must implement 6.3, excluding essentially all existing hardware.

Citing the external spec is also what lets restatements go: once yamux is referenced, restating its window defaults is a worse copy of the thing being pointed at.

Cited so far: BCP 14, Noise, yamux, Bluetooth Core.

Still needed: the Core Specification Supplement for BLI-ADV's advertising data formats, which is a separate document from the Core Specification; ISO/IEC 18004 for the QR code, now that the code rather than the substrate is the invariant; RFC 9106 for argon2; RFC 4648 for base32; TPM 2.0 and SMBIOS for the board ID sources.

## Sequencing

- [x] Pilot the voice on `channel.md`
- [x] Requirements notation and global glossary in `overview.md`
- [x] presence secret and QR code terminology in `overview.md` and `channel.md`
- [x] Per-spec terms glossary in `channel.md`
- [x] Link and anchor checking in CI (`.github/workflows/links.yml`)
- [ ] Terminology sweep across the remaining seven specs
- [ ] Per-spec terms glossaries where a spec borrows terms of art, starting with BLI-ADV and BLI-BID
- [ ] Voice rewrite of the remaining eight specs
- [ ] Terminology sweep in code: around 320 sites over 16 files, two modules named `sticker.rs`, the `Sticker` CLI subcommand and its `sticker` argument
- [ ] Re-ID: drop the `BLI-` prefix, keeping `BLI` for the overview. BID, KEY, STK, ADV, CHN, MSG, WEB, SYS, with STK revisited alongside the file rename
- [ ] Rename `sticker.md` once its id is settled

Terminology settles before the voice rewrite of each spec, so prose is not written twice. The re-ID sweep lands as one atomic change across the specs and the code doc-comments that cite them.

## Gaps the rewrite exposed

Stating prose as a MUST is what surfaces these. Several are still open.

Resolved against the implementation:

- The chunk ceiling is the negotiated ATT_MTU less the three-byte ATT header, not the ATT_MTU itself. `gatt.rs` uses a fixed 20 bytes, which is the minimum ATT_MTU of 23 less that header.
- The client transmit characteristic accepts both a write and a write without response (`device.rs`), which the spec had never mentioned in either form.

Resolved by decision:

- Send rate is now a notification ceiling alone. The byte ceiling was redundant: 200 notifications at the 512-byte maximum payload works out to the same 100 KiB, and it is the notification count that overruns a controller. 200 is conservative and could be raised; measuring it against the test device is separate work.
- The transport length prefix is two bytes rather than four. A Noise message maxes at 65535, which two bytes express exactly, so an over-length claim becomes unrepresentable and the rule refusing one disappears. This also dissolved the question of how to refuse one below yamux, where there is no stream to close.

### The two framing layers are one implementation

Making the prefix two bytes exposed that `write_message` and `read_message` in `stream.rs` serve both layers at once: the Noise handshake framing of BLI-CHN, and the application message framing of BLI-MSG inside yamux streams. `messages.md` warns in prose that these are different things and that conflating them reads nonsense; the code conflates them.

Two consequences today, before any change:

- Handshake messages are bounded by MAX_MESSAGE (128 KiB, BLI-MSG's ceiling) rather than by 65535. The bound that applies before a peer has authenticated is twice what BLI-CHN specifies.
- `write_message` frames with the transport helper while `read_message` applies the application ceiling, so the write and read sides are governed by different specs.

Separating them is the prerequisite for the two-byte prefix reaching the code. The transport layer takes the two-byte prefix and the 65535 bound; the application layer keeps a four-byte prefix and its 128 KiB bound. Differing widths also make the two structurally distinguishable, which removes the hazard `messages.md` had to warn about in prose.

A check enforcing a deliberate product ceiling, as the 128 KiB one does, is legitimate. A check enforcing a limit the field could have expressed structurally, as the 65535 one did, is the smell.

Spawned as **Q1**. Until it lands, `channel.md` specifies a two-byte prefix and `framing.rs` still writes four, which is a deliberate and recorded divergence. Measuring the notification ceiling is **R1**.

Open:

- If write without response is the intended fast path, the spec should say so rather than permitting both equally.
- The security properties in `channel.md`'s authentication note overlap BLI's "Where the guarantees stop". Decide one home.
- BLI-ADV depends on the 31-byte legacy advertising budget. Whether that is a floor a device must fit, or a consequence of targeting legacy controllers, is not stated.

## Other findings

- Em-dashes violated the house rule in `overview.md` only (two, now fixed). The other eight were clean.
- The link checker runs `--offline`, so external URLs are not checked and CI stays deterministic. It guards local links and heading anchors, which is the breakage the deep-link convention introduces. External link rot would want a separate scheduled job.
- No anchor links existed anywhere in the specs before this card, so the convention and its guard arrive together.
