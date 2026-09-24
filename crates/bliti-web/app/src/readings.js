// Making sense of the facts and readings of NFO, and rendering them the way VIEW asks ours to.
//
// The wire carries data; the intelligence is here. This module holds our application's own order, its
// wording for the catalogue it recognises, and its aggregation rules. Everything it does not
// recognise still renders, from the entry's own name, kind and traits, so a device ahead of this
// build degrades to a plain reading card rather than vanishing (VIEW).

/// One fact or reading, normalised from a message. `fact` is true for a fact, false for a reading;
/// the two catalogues are separate and a name may appear in both.
export function entryOf(message) {
	return {
		fact: message.type === 'fact',
		name: message.fact ?? message.measurement,
		at: message.at,
		traits: message.traits ?? {},
		kind: message.kind,
		unit: message.unit,
		value: message.value,
	}
}

/// The status trait's `is`, or null. A status this build does not know is left as it arrived, and
/// treated as passed rather than crying wolf.
export function statusOf(entry) {
	return entry.traits?.status?.is ?? null
}

export function reasonOf(entry) {
	return entry.traits?.status?.reason ?? null
}

/// Whether a reading should draw an operator's attention. Only the two states with a notion of
/// difficulty do; skipped and broken say nothing is wrong with the thing measured, only that there is
/// no value.
export function isTrouble(entry) {
	return statusOf(entry) === 'warning' || statusOf(entry) === 'failed'
}

/// Whether the entry has a value at all. Skipped and broken carry none (NFO). Tolerates a missing
/// entry, so a fold that names one the device did not send is simply empty.
export function hasValue(entry) {
	return entry != null && entry.value !== undefined && entry.value !== null
}

// The order our application renders the catalogue it recognises in, and the wording it gives each
// name. The order is fixed and does not move when a reading goes into difficulty (VIEW).
export const HEADER = ['hostname', 'board', 'board-revision', 'os', 'kernel']

export const TILE_ORDER = [
	'network-address',
	'wireless-network',
	'hotspot',
	'hotspot-clients',
	'cpu-usage',
	'memory-usage',
	'filesystem-usage',
	'network-throughput',
	'temperature',
	'cpu-frequency',
	'fan-speed',
	'power-source',
	'battery-charge',
	'last-boot',
]

// Whether the network runs what was recorded or a proposal being tried. It marks the tiles below
// rather than getting one of its own (VIEW).
export const NETWORK_CONFIGURATION = 'network-configuration'
export const PROVISIONAL_TILES = new Set(['network-address', 'wireless-network', 'hotspot', 'hotspot-clients'])

// Entries rendered inside another's reveal rather than as a tile of their own.
export const IN_REVEAL = new Set([
	'memory-total',
	'filesystem-total',
	'cpu-frequency-max',
	'battery-voltage',
	'battery-direction',
])

// Our wording for each catalogue name. A name not here is title-cased from the name itself.
const LABELS = {
	'network-address': 'Address',
	'wireless-network': 'Wireless',
	hotspot: 'Hotspot',
	'hotspot-clients': 'Hotspot clients',
	'cpu-usage': 'Processor',
	'memory-usage': 'Memory',
	'filesystem-usage': 'Storage',
	'network-throughput': 'Network',
	temperature: 'Temperature',
	'cpu-frequency': 'Processor speed',
	'fan-speed': 'Fan',
	'power-source': 'Power',
	'battery-charge': 'Battery',
	'last-boot': 'Uptime',
}

// Our wording for the values of the catalogue's descriptive text readings.
const VALUES = {
	'power-source': { 'via-backup': 'On backup', battery: 'On battery', 'bypassing-backup': 'Backup bypassed' },
	'battery-direction': { charging: 'Charging', discharging: 'Discharging', idle: 'Idle' },
}

export function labelOf(name) {
	return LABELS[name] ?? titleCase(name)
}

function titleCase(name) {
	return name
		.split(/[-_]/)
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
		.join(' ')
}

/// A value as an operator reads it: by its kind, in our own wording and at our own magnitude, or the
/// value stringified where the kind is one this build does not know (VIEW).
export function formatValue(entry) {
	if (!hasValue(entry)) return null
	const { kind, value, unit, name } = entry
	switch (kind) {
		case 'fraction':
			return `${Math.round(value * 100)}%`
		case 'quantity':
			return formatQuantity(value, unit)
		case 'duration':
			return formatDuration(value)
		case 'datetime':
			return name === 'last-boot' ? elapsedSince(value) : String(value)
		case 'text':
			return VALUES[name]?.[value] ?? String(value)
		case 'ipv4':
		case 'ipv6':
			return String(value)
		default:
			// A kind this build does not know: the value stringified, with the unit where there is one.
			return unit ? `${stringify(value)} ${unit}` : stringify(value)
	}
}

function stringify(value) {
	return typeof value === 'object' ? JSON.stringify(value) : String(value)
}

/// A quantity written in our own abbreviation and magnitude. An unrecognised unit is written out as
/// it was sent (VIEW).
function formatQuantity(number, unit) {
	switch (unit) {
		case 'bytes':
			return magnitude(number, 'B', 'kB', 'MB', 'GB')
		case 'bytes/second':
			return magnitude(number, 'B/s', 'kB/s', 'MB/s', 'GB/s')
		case 'hertz':
			return magnitude(number, 'Hz', 'kHz', 'MHz', 'GHz')
		case 'celsius':
			return `${trim(number)} °C`
		case 'volts':
			return `${trim(number)} V`
		case 'revolutions/minute':
			return `${Math.round(number)} rpm`
		case 'clients':
			return number === 1 ? '1 client' : `${trim(number)} clients`
		default:
			return `${trim(number)} ${unit}`
	}
}

function magnitude(number, ...units) {
	let value = number
	let index = 0
	while (value >= 1000 && index < units.length - 1) {
		value /= 1000
		index += 1
	}
	return `${trim(Math.round(value * 100) / 100)} ${units[index]}`
}

function trim(number) {
	return Number.isInteger(number) ? String(number) : String(Math.round(number * 100) / 100)
}

function formatDuration(seconds) {
	const whole = Math.floor(seconds)
	const days = Math.floor(whole / 86400)
	const hours = Math.floor((whole % 86400) / 3600)
	const minutes = Math.floor((whole % 3600) / 60)
	if (days > 0) return `${days}d ${hours}h`
	if (hours > 0) return `${hours}h ${minutes}m`
	return `${minutes}m`
}

/// An RFC 3339 instant rendered as how long ago it was. `last-boot` is shown as an elapsed time
/// (VIEW); a clock the device could not answer for is not sent, so this only runs on a real instant.
function elapsedSince(rfc3339) {
	const then = Date.parse(rfc3339)
	if (Number.isNaN(then)) return String(rfc3339)
	return formatDuration(Math.max(0, (Date.now() - then) / 1000))
}

/// Where a value sits on its scale, from 0 to 1, or null where it has none to be drawn against. Only
/// a fraction, or a quantity with a total or a `limits` trait, has a scale (VIEW).
export function scaleOf(entry, total = null) {
	if (!hasValue(entry)) return null
	if (entry.kind === 'fraction') return clamp(entry.value)
	if (entry.kind === 'quantity') {
		const top = total ?? limitTop(entry)
		if (typeof top === 'number' && top > 0) return clamp(entry.value / top)
	}
	return null
}

/// The largest limit on a reading, which its scale is drawn against where it has no total.
function limitTop(entry) {
	const limits = entry.traits?.limits
	if (!Array.isArray(limits) || limits.length === 0) return null
	return Math.max(...limits.map((limit) => limit.at))
}

function clamp(fraction) {
	return Math.min(1, Math.max(0, fraction))
}

// Series keying. A reading's history is keyed by its catalogue name together with every trait NFO
// does not name as descriptive; an unrecognised trait is part of the key, so a device that gains a
// trait splitting one series into several is left two series it cannot fully tell apart rather than
// one merged wrong one (VIEW).

// The descriptive traits, which do not distinguish one thing measured from another: the whole
// `status`, `limits`, `security` and `channel` traits, and the members named here within the traits
// that hold them. A battery's serial, model and vendor describe the cell; only its name says which
// battery it is. A wireless link's channel moves whenever a shared-channel hotspot follows the client
// onto a new one, and that is the same link, not a second.
const DESCRIPTIVE = new Set(['status', 'limits', 'security', 'channel'])
const DESCRIPTIVE_MEMBERS = {
	interface: new Set(['route', 'overlay']),
	battery: new Set(['serial', 'model', 'vendor']),
}

// The entries NFO's catalogue lists as one entry per value held. Several are held at once with the
// same name, kind and traits, as an interface holds an IPv4 address and more than one IPv6 address,
// so the value is what tells them apart, and one that ends carries it.
const TOLD_APART_BY_VALUE = new Set(['network-address'])

/// The key a reading's history is held under: its name, kind and distinguishing traits, canonicalised
/// so member order does not matter, and its value where that is what tells it apart (NFO).
export function seriesKey(entry) {
	const value = TOLD_APART_BY_VALUE.has(entry.name) ? canonical(entry.value) : ''
	return `${entry.name}\u001f${entry.kind}\u001f${canonical(distinguishing(entry.traits))}\u001f${value}`
}

/// The identity of one entry instance for the tile grid: the same key, so two readings alike but for
/// a distinguishing trait are two entries and the same one re-sampled is one.
export function identityKey(entry) {
	return `${entry.fact ? 'fact' : 'reading'}\u001f${seriesKey(entry)}`
}

/// The traits with the descriptive ones stripped: the whole `status` and `limits`, and the
/// descriptive members of any trait that holds some.
function distinguishing(traits) {
	const kept = {}
	for (const [name, value] of Object.entries(traits ?? {})) {
		if (DESCRIPTIVE.has(name)) continue
		const descriptive = DESCRIPTIVE_MEMBERS[name]
		if (descriptive && value && typeof value === 'object' && !Array.isArray(value)) {
			const trimmed = {}
			for (const [member, inner] of Object.entries(value)) {
				if (!descriptive.has(member)) trimmed[member] = inner
			}
			kept[name] = trimmed
		} else {
			kept[name] = value
		}
	}
	return kept
}

/// A stable string for a JSON value, with object keys sorted, so member order does not change a key.
function canonical(value) {
	if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`
	if (value && typeof value === 'object') {
		return `{${Object.keys(value)
			.sort()
			.map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
			.join(',')}}`
	}
	return JSON.stringify(value)
}

/// Add a reading to its history, keeping the points ordered and trimmed to the window. Facts have no
/// history and are never added (VIEW).
export function pushHistory(history, entry) {
	if (entry.fact || !hasValue(entry)) return history
	const number = numberOf(entry)
	if (number === null) return history
	const key = seriesKey(entry)
	const next = new Map(history)
	const points = [...(next.get(key) ?? []), { at: entry.at, number }]
	points.sort((a, b) => a.at - b.at)
	const newest = points[points.length - 1].at
	next.set(
		key,
		points.filter((point) => newest - point.at <= WINDOW_MS),
	)
	return next
}

/// Drop the history of an entry that has ended, along with its tile (VIEW).
export function forgetHistory(history, entry) {
	const key = seriesKey(entry)
	if (!history.has(key)) return history
	const next = new Map(history)
	next.delete(key)
	return next
}

/// Whether the device has said this entry no longer applies (NFO).
export function isEnded(entry) {
	return statusOf(entry) === 'ended'
}

/// How far back the graph keeps points. A graph fills forward from connection; no history is sent, so
/// this is only the trimming of what accumulates (U1 carries bringing sent history back).
export const WINDOW_MS = 5 * 60 * 1000

/// The number behind a value, where it has one. Null for text and for a kind with no number.
export function numberOf(entry) {
	if (!entry || !hasValue(entry)) return null
	switch (entry.kind) {
		case 'fraction':
		case 'quantity':
			return typeof entry.value === 'number' ? entry.value : null
		case 'duration':
			return typeof entry.value === 'number' ? entry.value : null
		default:
			return null
	}
}

/// Points as an SVG polyline, scaled to their own peak within the box. Each series is scaled to its
/// own peak, and the peak is stated alongside, because two directions of throughput routinely differ
/// by an order of magnitude and a shared scale flattens the quieter one to a line (VIEW).
export function polyline(points, { width, height, flip = false, peak = null }) {
	if (points.length === 0) return { points: '', peak: 0 }
	const top = peak ?? Math.max(...points.map((point) => point.number), 0)
	const first = points[0].at
	const span = points[points.length - 1].at - first || 1

	const coords = points.map((point) => {
		const x = ((point.at - first) / span) * width
		const drawn = top > 0 ? (point.number / top) * height : 0
		const y = flip ? drawn : height - drawn
		return `${round(x)},${round(y)}`
	})
	return { points: coords.join(' '), peak: top }
}

function round(value) {
	return Math.round(value * 10) / 10
}

/// A trait value written for display: a string as-is, an object as its members joined. Used as the
/// qualifier on an entry this build does not otherwise recognise (VIEW).
/// The addresses the Address tile headlines: on the interface carrying the default route, its first
/// IPv4, else global IPv6, else unique local IPv6 address; and on an overlay interface, its first IPv4,
/// else global IPv6 address. At most two, the default route's first (VIEW).
export function headlineAddresses(addresses) {
	const firstOf = (held, classes) => {
		for (const wanted of classes) {
			const found = held.find((entry) => addressClass(entry) === wanted)
			if (found) return found
		}
		return null
	}
	const iface = (entry) => entry.traits?.interface ?? {}
	const onDefault = addresses.filter((entry) => iface(entry).route === 'default' && !iface(entry).overlay)
	const onOverlay = addresses.filter((entry) => Boolean(iface(entry).overlay))
	return [
		firstOf(onDefault, ['ipv4', 'global', 'unique-local']),
		firstOf(onOverlay, ['ipv4', 'global']),
	].filter(Boolean)
}

/// Which of the classes the headline chooses among an address falls in: `ipv4`, `global` IPv6
/// (2000::/3), `unique-local` IPv6 (fc00::/7), or `other`.
function addressClass(entry) {
	if (entry.kind === 'ipv4') return 'ipv4'
	if (entry.kind !== 'ipv6') return 'other'
	const first = parseInt(String(entry.value).split(':')[0] || '0', 16)
	if (first >= 0x2000 && first <= 0x3fff) return 'global'
	if ((first & 0xfe00) === 0xfc00) return 'unique-local'
	return 'other'
}

export function qualifierOf(entry) {
	const parts = []
	for (const [name, value] of Object.entries(entry.traits ?? {})) {
		if (DESCRIPTIVE.has(name)) continue
		parts.push(traitText(value))
	}
	return parts.filter(Boolean).join(' · ')
}

function traitText(value) {
	if (value === null || value === undefined) return ''
	if (typeof value === 'object' && !Array.isArray(value)) {
		return Object.values(value)
			.map((inner) => (typeof inner === 'object' ? JSON.stringify(inner) : String(inner)))
			.join(' ')
	}
	return String(value)
}
