// The rows of the Status screen of STAT: whether the device is on a network, and how Tailscale and
// Tamanu stand, read from the facts and readings the feed holds.
//
// Each row is a mark, a headline naming the thing and how it stands, the device's `reason` where the
// row calls for it, and lines of detail beneath. A line is a list of parts, each a short value with
// an icon or a label where it has one.

import { hasValue, reasonOf, statusOf } from './readings.js'

/// The marks a row carries (STAT).
export const WELL = 'well'
export const WARNING = 'warning'
export const NOT_WELL = 'not-well'
export const IN_PROGRESS = 'in-progress'

/// The three rows, in the order the screen shows them. `settled` is whether the feed's first batch
/// has ended. The Tailscale row is null where the device does not report Tailscale.
export function statusRows(entries, settled, now = Date.now()) {
	const single = (name) => entries.find((entry) => entry.name === name)
	return {
		network: settled ? networkRow(entries) : checking('network'),
		tailscale: tailscaleRow(single, settled, now),
		tamanu: tamanuRow(single),
	}
}

function checking(name) {
	return { mark: IN_PROGRESS, headline: `Checking ${name}`, reason: null, lines: [] }
}

// The network.

/// The ways the device is on a network: each interface in use for the cable and for wifi, by name,
/// and the hotspot where it is up (STAT, "The network row").
export function waysInUse(entries) {
	const facts = entries.filter((entry) => entry.fact && hasValue(entry))
	const interfaceOf = (entry) => entry.traits?.interface?.name
	const addressed = new Set(
		facts
			.filter((entry) => entry.name === 'network-address' && !isLinkLocal(String(entry.value)))
			.map(interfaceOf),
	)
	const byName = (a, b) => (a.interface < b.interface ? -1 : a.interface > b.interface ? 1 : 0)
	const plugged = facts
		.filter((entry) => entry.name === 'wired-carrier' && entry.value === true)
		.map((entry) => ({ interface: interfaceOf(entry) }))
		.sort(byName)
	const joined = facts
		.filter((entry) => entry.name === 'wireless-network')
		.map((entry) => ({ interface: interfaceOf(entry), network: String(entry.value) }))
		.sort(byName)
	const hotspot = facts.find((entry) => entry.name === 'hotspot')
	const clients = entries.find((entry) => !entry.fact && entry.name === 'hotspot-clients')
	// The default route is a trait of every entry about the interface, so whichever carries it says.
	const routed = new Set(
		facts.filter((entry) => entry.traits?.interface?.route === 'default').map(interfaceOf),
	)
	const inUse = (each) => addressed.has(each.interface)
	const withRoute = (each) => ({ ...each, route: routed.has(each.interface) })
	return {
		cable: plugged.filter(inUse).map(withRoute),
		wifi: joined.filter(inUse).map(withRoute),
		hotspot: hotspot ? { clients: hasValue(clients) ? Number(clients.value) : null } : null,
		cableWithoutAddress: plugged.filter((each) => !inUse(each)),
		wifiWithoutAddress: joined.filter((each) => !inUse(each)),
	}
}

/// Whether an address is link-local: 169.254.0.0/16 or fe80::/10.
export function isLinkLocal(address) {
	const bare = address.split('/')[0].toLowerCase()
	if (bare.includes(':')) return /^fe[89ab]/.test(bare)
	return bare.startsWith('169.254.')
}

/// The one way of cable or wifi the row names: the one carrying the default route, and otherwise
/// the first cable by interface name, then the first wifi.
export function uplinkOf(ways) {
	const cable = ways.cable.map((each) => ({ ...each, way: 'cable' }))
	const wifi = ways.wifi.map((each) => ({ ...each, way: 'wifi' }))
	return [...cable, ...wifi].find((each) => each.route) ?? cable[0] ?? wifi[0] ?? null
}

function networkRow(entries) {
	const ways = waysInUse(entries)
	const uplink = uplinkOf(ways)
	if (uplink || ways.hotspot) {
		const parts = []
		if (uplink) {
			parts.push({ icon: uplink.way, text: uplink.way === 'cable' ? 'Cable' : uplink.network })
		}
		if (ways.hotspot) {
			const { clients } = ways.hotspot
			parts.push({ icon: 'hotspot', text: clients === null ? 'Hotspot' : `Hotspot, ${clients} joined` })
		}
		return { mark: WELL, headline: 'Network connected', reason: null, lines: [parts], action: 'Change' }
	}
	const lines = []
	if (ways.cableWithoutAddress.length > 0) lines.push([{ text: 'Cable plugged in, no address' }])
	for (const each of ways.wifiWithoutAddress) lines.push([{ text: `Joined ${each.network}, no address` }])
	return { mark: NOT_WELL, headline: 'Network not connected', reason: null, lines, action: 'Set up' }
}

// Tailscale and Tamanu, each read from its state by a table of what our application recognises.
// A key of a state and a status is looked up before the state alone.

const TAILSCALE = {
	'running/passed': { mark: WELL, says: 'connected' },
	'running/warning': { mark: WARNING, says: 'connected', reason: true },
	starting: { mark: IN_PROGRESS, says: 'connecting' },
	stopped: { mark: NOT_WELL, says: 'turned off' },
	'needs-login': { mark: NOT_WELL, says: 'not signed in' },
	expired: { mark: NOT_WELL, says: 'sign-in expired' },
	'needs-approval': { mark: NOT_WELL, says: 'waiting for approval' },
}

const TAMANU = {
	running: { mark: WELL, says: 'running' },
	degraded: { mark: WARNING, says: 'partly running', reason: true },
	stopped: { mark: NOT_WELL, says: 'stopped', reason: true },
	absent: { mark: NOT_WELL, says: 'not installed' },
	starting: { mark: IN_PROGRESS, says: 'starting' },
	restarting: { mark: IN_PROGRESS, says: 'restarting' },
}

/// A state as a row's mark, headline and reason, by `table`, or by its `status` where the table does
/// not recognise it (STAT, "The rows").
export function stateRow(name, state, table) {
	if (!hasValue(state)) {
		return { mark: NOT_WELL, headline: `${name} state unknown`, reason: reasonOf(state), lines: [] }
	}
	const value = String(state.value)
	const status = statusOf(state)
	const known = table[`${value}/${status}`] ?? table[value]
	if (known) {
		const reason = known.reason ? reasonOf(state) : null
		return { mark: known.mark, headline: `${name} ${known.says}`, reason, lines: [] }
	}
	const mark = status === 'passed' ? WELL : status === 'warning' ? WARNING : NOT_WELL
	return { mark, headline: `${name} ${value}`, reason: reasonOf(state), lines: [] }
}

function tailscaleRow(single, settled, now) {
	const state = single('tailscale-state')
	if (!state) return settled ? null : checking('Tailscale')
	const row = stateRow('Tailscale', state, TAILSCALE)
	const name = single('tailscale-name')
	if (hasValue(name)) row.lines.push([{ label: 'Device name', text: String(name.value) }])
	const expiry = single('tailscale-key-expiry')
	if (expiry && statusOf(expiry) === 'warning') {
		if (row.mark === WELL) row.mark = WARNING
		if (hasValue(expiry)) {
			row.lines.push([{ label: 'Sign-in expires', text: fromNow(Date.parse(expiry.value), now) }])
		}
	}
	return row
}

function tamanuRow(single) {
	const state = single('tamanu-state')
	if (!state) return checking('Tamanu')
	const row = stateRow('Tamanu', state, TAMANU)
	const versions = [
		['Web', single('tamanu-web-version')],
		['Facility server', single('tamanu-facility-server-version')],
	]
		.filter(([, entry]) => hasValue(entry))
		.map(([label, entry]) => ({ text: `${label} ${entry.value}` }))
	if (versions.length > 0) row.lines.push(versions)
	return row
}

const RELATIVE = new Intl.RelativeTimeFormat('en-AU', { numeric: 'auto' })
const UNITS = [
	['day', 86_400_000],
	['hour', 3_600_000],
	['minute', 60_000],
]

/// An instant as a time from now, in the largest whole unit it spans: "in 3 days", "in 5 hours".
export function fromNow(at, now) {
	const delta = at - now
	for (const [unit, ms] of UNITS) {
		if (Math.abs(delta) >= ms) return RELATIVE.format(Math.trunc(delta / ms), unit)
	}
	return RELATIVE.format(0, 'minute')
}
