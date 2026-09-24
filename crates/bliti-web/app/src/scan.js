// Two readings of one scan (BLI-NSCR): the networks it heard, for joining one, and the access points
// and channels around the device, for siting a new access point. The device reports one entry per
// access point each radio heard (BLI-CFG); which of them form one network is ours to draw.

const BANDS = ['2ghz', '5ghz', '6ghz']

const bySignal = (a, b) => (b.signal ?? -Infinity) - (a.signal ?? -Infinity)
const named = (point) => typeof point?.ssid === 'string' && point.ssid !== ''

function strongest(points) {
	return points.reduce((best, point) => (typeof point.signal === 'number' && point.signal > best ? point.signal : best), -Infinity)
}

function distinct(points) {
	return new Set(points.map((point) => point.bssid)).size
}

/// The networks a scan heard, strongest first. Each named network groups its access points by SSID,
/// with the strongest signal among them, how many there are, and every security any of them
/// advertises. An access point with no SSID is a network of its own, and is left out unless `hidden`.
export function networksOf(points, { hidden = false } = {}) {
	const list = Array.isArray(points) ? points.filter((point) => point && typeof point === 'object') : []
	const groups = new Map()
	for (const point of list.filter(named)) {
		if (!groups.has(point.ssid)) groups.set(point.ssid, [])
		groups.get(point.ssid).push(point)
	}
	const networks = [...groups].map(([ssid, heard]) => network(ssid, heard))
	if (hidden) networks.push(...list.filter((point) => !named(point)).map((point) => network(null, [point])))
	return networks.sort((a, b) => b.signal - a.signal)
}

function network(ssid, heard) {
	return {
		ssid,
		signal: strongest(heard),
		count: distinct(heard),
		security: [...new Set(heard.flatMap((point) => (Array.isArray(point.security) ? point.security : [])))],
		points: [...heard].sort(bySignal),
	}
}

/// How many access points a scan heard with no SSID.
export function hiddenCount(points) {
	return Array.isArray(points) ? distinct(points.filter((point) => point && !named(point))) : 0
}

// The runs of 20 MHz channels on 5 GHz that wider channels bond within, each from its first channel
// to its last.
const FIVE_GHZ_RUNS = [
	[36, 64],
	[100, 144],
	[149, 177],
]

/// The 20 MHz channels an access point occupies: its primary alone at 20 MHz, and every channel of
/// the bonded group it sits in where it is wider. On 2.4 GHz, where channels do not bond in fixed
/// groups, a wide one takes its primary and the secondary channel it names.
export function spanned(point) {
	const { band, channel } = point
	const width = point['channel-width']
	if (typeof channel !== 'number') return []
	if (typeof width !== 'number' || width <= 20) return [channel]
	if (band === '2ghz') {
		const secondary = point['secondary-channel']
		return typeof secondary === 'number' ? [Math.min(channel, secondary), Math.max(channel, secondary)] : [channel]
	}
	const run = band === '5ghz' ? FIVE_GHZ_RUNS.find(([first, last]) => channel >= first && channel <= last) : band === '6ghz' ? [1, 233] : null
	if (!run || (channel - run[0]) % 4 !== 0) return [channel]
	const count = width / 20
	const first = run[0] + Math.floor((channel - run[0]) / (4 * count)) * 4 * count
	return Array.from({ length: count }, (_, index) => first + index * 4).filter((each) => each <= run[1])
}

/// A scan read for siting: every access point by signal, and for each band `bands` names, the channels
/// taken on it, counting every channel a wide access point spans, each with how many access points
/// take it and the widest of them.
export function siting(points, bands) {
	const list = Array.isArray(points) ? points.filter((point) => point && typeof point === 'object') : []
	const order = [...bands].sort((a, b) => BANDS.indexOf(a) - BANDS.indexOf(b))
	return {
		points: [...list].sort(bySignal),
		bands: order.map((band) => {
			const taken = new Map()
			for (const point of list.filter((each) => each.band === band)) {
				for (const channel of spanned(point)) {
					if (!taken.has(channel)) taken.set(channel, [])
					taken.get(channel).push(point)
				}
			}
			return {
				band,
				channels: [...taken]
					.sort(([a], [b]) => a - b)
					.map(([channel, heard]) => ({
						channel,
						count: distinct(heard),
						width: Math.max(...heard.map((point) => point['channel-width'] ?? 0)) || null,
					})),
			}
		}),
	}
}
