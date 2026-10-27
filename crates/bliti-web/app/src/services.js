// The acts the Status screen's Tailscale and Tamanu rows offer (STAT) and the Tailscale screen
// offers (TSCR), what our application calls each, and how it reads and shares what a network check
// or a ping found. Plain functions, so none of it needs a page.

/// The act each row offers, by the state the row is rendered from, where the device lists it (STAT).
const TAILSCALE_ROW = {
	'needs-login': 'sign-in',
	expired: 'sign-in',
	stopped: 'turn-on',
	running: 'netcheck',
	broken: 'netcheck',
	'needs-approval': 'log-out',
}

const TAMANU_ROW = { stopped: 'start', running: 'restart', degraded: 'restart' }

/// What a row's act is called, and for one confirmed, what the confirmation asks and its buttons
/// (STAT). `keep` is the choice the confirmation prefers, filled where it is given.
export const ROW_ACTS = {
	'sign-in': { button: 'Connect' },
	'turn-on': { button: 'Turn on' },
	netcheck: { button: 'Network check' },
	'log-out': { button: 'Cancel', question: 'Cancel the sign-in?', confirm: 'Cancel sign-in', keep: 'Keep waiting' },
	start: { button: 'Start', question: 'Start Tamanu?', confirm: 'Start' },
	restart: {
		button: 'Restart',
		question: 'Restart Tamanu?',
		warning: 'Tamanu may be unavailable while it restarts.',
		confirm: 'Restart',
	},
}

/// The act the Tailscale row offers in `state`, or null where it offers none: until the device has
/// listed its acts, where it does not list the state's, and in any state the table leaves out.
export const tailscaleRowAct = (state, listed) => rowAct(TAILSCALE_ROW, state, listed)

/// The act the Tamanu row offers in `state`, as [`tailscaleRowAct`] chooses it.
export const tamanuRowAct = (state, listed) => rowAct(TAMANU_ROW, state, listed)

function rowAct(table, state, listed) {
	const act = state ? table[state] : undefined
	return act && listed?.includes(act) ? act : null
}

/// The acts of the Tailscale screen, in the order it offers them, and what it calls each (TSCR).
export const TAILSCALE_ACTS = [
	{ act: 'sign-in', name: 'Connect', hint: 'Sign in to a tailnet.', button: 'Connect' },
	{ act: 'turn-on', name: 'Turn on', hint: 'Reconnects to the tailnet.', button: 'Turn on' },
	{
		act: 'turn-off',
		name: 'Turn off',
		hint: 'Stays signed in.',
		button: 'Turn off',
		question: 'Turn off Tailscale?',
		warning: "The fleet can't reach the Iti through Tailscale until it is turned on again.",
	},
	{
		act: 'log-out',
		name: 'Log out',
		hint: 'Removes the sign-in.',
		button: 'Log out',
		question: 'Log out?',
		warning: "The fleet can't reach the Iti through Tailscale until it is signed in again.",
		danger: true,
	},
]

/// What a network check's result says of whether the device can reach its tailnet (STAT): that it
/// can where any relay answered, and otherwise that it is blocked, with why as far as the result says.
export function verdict(result) {
	if (result.relays?.length > 0) return { connects: true, text: 'Can connect' }
	let why = "Tailscale can't be reached from this network"
	if (result['captive-portal'] === true) why = 'the network asks for a sign-in'
	else if (result.ipv4 === false && result.ipv6 === false) why = 'no internet'
	return { connects: false, text: `Blocked: ${why}` }
}

const yes = (value) => (value ? 'Yes' : 'No')

/// A round trip in seconds, in whole milliseconds.
export const millis = (seconds) => `${Math.round(seconds * 1000)} ms`

/// A relay by its name, or its number where the device gives no name.
export const relayName = (relay) => relay.name ?? `Relay ${relay.id}`

/// What the Tailscale screen shows beneath a network check's verdict (TSCR): each figure labelled,
/// the two the test may not tell only where it did, and the relays nearest first.
export function netcheckFigures(result) {
	const figures = [
		['UDP', yes(result.udp)],
		['IPv4', yes(result.ipv4)],
		['IPv6', yes(result.ipv6)],
	]
	if (typeof result['nat-varies'] === 'boolean') figures.push(['Address varies', yes(result['nat-varies'])])
	if (typeof result['captive-portal'] === 'boolean') figures.push(['Asks for sign-in', yes(result['captive-portal'])])
	const relays = [...(result.relays ?? [])]
		.sort((a, b) => a.latency - b.latency)
		.map((relay) => ({ id: relay.id, name: relayName(relay), latency: millis(relay.latency) }))
	return { figures, relays }
}

/// What a ping found (TSCR): the machine tested and whether it answered, then how and how fast.
export function pingSays(result) {
	if (!result.reachable) return { text: `${result.peer} did not answer`, figures: [] }
	const path = result.path === 'relayed' ? (result.relay ? `Through a relay, ${relayName(result.relay)}` : 'Through a relay') : 'Direct'
	const figures = [['Path', path]]
	if (typeof result.latency === 'number') figures.push(['Round trip', millis(result.latency)])
	return { text: `${result.peer} answered`, figures }
}

/// The text a network check is shared as (TSCR): the device, when the check ran, the result as the
/// device sent it but for `report`, then `report` itself, untouched.
export function netcheckShare(hostname, at, result) {
	const { report, ...rest } = result
	const parts = [`Network check of ${hostname} at ${at.toISOString()}`, JSON.stringify(rest, null, 2)]
	if (typeof report === 'string') parts.push(report)
	return parts.join('\n\n')
}

/// What the activity record says of a Tailscale act sent: the act and its members, but never the
/// auth key, which it has no way to be given (TSC).
export function tailscaleActSaid(act, { method, name, peer } = {}) {
	const members = [method && `method ${method}`, name && `name ${name}`, peer && `peer ${peer}`].filter(Boolean)
	return `tailscale-act  ${[act, ...members].join(', ')}`
}
