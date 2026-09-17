// Making sense of the self-describing readings of BLI-SYS, with no list of names to match against.
//
// Nothing here asks what a reading is called. A reading says what it is, what unit it is in, and
// where its limits sit, and everything below works from that alone. The consequence is the one that
// matters: a device that gains a reading appears in an application that has never heard of it, with
// no release in between.

// How far back the view keeps samples. The device sends about this much on subscribing, and the
// window is trimmed to it as more arrive.
export const WINDOW_MS = 5 * 60 * 1000

/// The number behind a value, where it has one. Null for text, and for a kind this build does not
/// know.
export function numberOf(value) {
	if (!value) return null
	switch (value.kind) {
		case 'fraction':
			return value.number
		case 'quantity':
			return value.number
		case 'duration':
			return value.seconds
		default:
			return null
	}
}

/// A value as an operator reads it.
export function formatValue(value) {
	if (!value) return null
	switch (value.kind) {
		case 'fraction':
			return `${Math.round(value.number * 100)}%`
		case 'quantity':
			return `${trim(value.number)} ${value.unit}`
		case 'duration':
			return formatDuration(value.seconds)
		case 'text':
			return value.text
		default:
			// A kind this build has never heard of. The reading's label already said what it is, and
			// saying nothing further beats guessing at a number whose meaning we do not know.
			return null
	}
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

/// Where a value sits on its scale, from 0 to 1, or null where it has no scale to be drawn against.
///
/// Only a fraction and a quantity that named its ceiling have one. A quantity without a ceiling is
/// never drawn against one, because the bar would imply a maximum the device never claimed.
export function scaleOf(value) {
	if (!value) return null
	if (value.kind === 'fraction') return clamp(value.number)
	if (value.kind === 'quantity' && typeof value.max === 'number' && value.max > 0) {
		return clamp(value.number / value.max)
	}
	return null
}

function clamp(fraction) {
	return Math.min(1, Math.max(0, fraction))
}

/// Whether a reading should draw an operator's attention. A state this build does not know is not
/// treated as trouble: a newer device must not make an older application cry wolf.
export function isTrouble(reading) {
	return reading.state === 'warn' || reading.state === 'fault'
}

/// Readings arranged for display: those sharing a group become one entry, the rest stand alone.
///
/// An application that ignored grouping would show the members separately and still be correct, so
/// this is an improvement on the floor rather than part of it.
export function groupReadings(readings) {
	const entries = []
	const byGroup = new Map()

	for (const reading of readings) {
		if (!reading.group) {
			entries.push({ key: reading.name, label: reading.label, readings: [reading] })
			continue
		}
		let entry = byGroup.get(reading.group)
		if (!entry) {
			entry = { key: reading.group, label: titleCase(reading.group), readings: [] }
			byGroup.set(reading.group, entry)
			entries.push(entry)
		}
		entry.readings.push(reading)
	}
	return entries
}

function titleCase(key) {
	const words = key.replace(/-/g, ' ')
	return words.charAt(0).toUpperCase() + words.slice(1)
}

/// The two readings of a group that are opposed flows, where it has exactly that.
///
/// This is what lets a pair be drawn mirrored about one time axis rather than as two charts. A group
/// that is not a pair, or whose directions are not opposed, gets no special treatment.
export function opposedPair(entry) {
	if (entry.readings.length !== 2) return null
	const [first, second] = entry.readings
	const opposed =
		(first.direction === 'in' && second.direction === 'out') ||
		(first.direction === 'out' && second.direction === 'in')
	if (!opposed) return null
	return first.direction === 'in' ? [first, second] : [second, first]
}

/// Add a sample to the window, keeping it ordered and trimmed.
///
/// Times are milliseconds since the device booted, so they are comparable only against each other.
/// They are never treated as wall time: a device in the field may have no set clock.
export function pushSample(window, sample) {
	const next = [...window, sample]
	next.sort((a, b) => a.at - b.at)
	const newest = next[next.length - 1].at
	return next.filter((each) => newest - each.at <= WINDOW_MS)
}

/// Merge the buffered window a device sends on subscribing.
export function mergeHistory(window, samples) {
	return samples.reduce((held, sample) => pushSample(held, sample), window)
}

/// The history of one reading, as points a graph can be drawn from.
///
/// Spaced by the time each sample was taken rather than evenly, so a gap in sampling shows as a gap
/// rather than being smoothed away.
export function seriesOf(window, name) {
	const points = []
	for (const sample of window) {
		const reading = sample.readings.find((each) => each.name === name)
		if (!reading) continue
		const number = numberOf(reading.value)
		if (number === null) continue
		points.push({ at: sample.at, number })
	}
	return points
}

/// The most recent reading of each name, from the window and whatever arrived outside it.
export function latest(window, extra = []) {
	const held = new Map()
	for (const sample of window) {
		for (const reading of sample.readings) held.set(reading.name, reading)
	}
	for (const reading of extra) held.set(reading.name, reading)
	return [...held.values()]
}

/// Points as an SVG polyline, scaled to their own peak within the box.
///
/// Each series is scaled to its own peak rather than to a peak shared with another: two directions of
/// throughput routinely differ by an order of magnitude, and a shared scale flattens the quieter one
/// to a line. The peak is stated alongside so the asymmetry is a number rather than lost geometry.
export function polyline(points, { width, height, flip = false, peak = null }) {
	if (points.length === 0) return { points: '', peak: 0 }
	const top = peak ?? Math.max(...points.map((each) => each.number), 0)
	const first = points[0].at
	const span = points[points.length - 1].at - first || 1

	const coords = points.map((point) => {
		const x = ((point.at - first) / span) * width
		const height_ = top > 0 ? (point.number / top) * height : 0
		const y = flip ? height_ : height - height_
		return `${round(x)},${round(y)}`
	})
	return { points: coords.join(' '), peak: top }
}

function round(value) {
	return Math.round(value * 10) / 10
}
