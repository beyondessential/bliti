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

Open:

- Send rate: 100 KiB across 200 notifications implies about 512 bytes each, which only holds when the negotiated ATT_MTU is that large. On a smaller MTU the notification ceiling binds first. Restate against the negotiated size?
- If write without response is the intended fast path, the spec should say so rather than permitting both equally.
- Rejecting an over-length Noise length prefix happens below yamux, so there is no stream to close. Abort the connection?
- The security properties in `channel.md`'s authentication note overlap BLI's "Where the guarantees stop". Decide one home.
- BLI-ADV depends on the 31-byte legacy advertising budget. Whether that is a floor a device must fit, or a consequence of targeting legacy controllers, is not stated.

## Other findings

- Em-dashes violated the house rule in `overview.md` only (two, now fixed). The other eight were clean.
- The link checker runs `--offline`, so external URLs are not checked and CI stays deterministic. It guards local links and heading anchors, which is the breakage the deep-link convention introduces. External link rot would want a separate scheduled job.
- No anchor links existed anywhere in the specs before this card, so the convention and its guard arrive together.
