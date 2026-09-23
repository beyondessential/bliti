// The network configuration screen's logic (BLI-NSCR), kept apart from its rendering so the stages,
// the checks and the wording can be read and tested without a page.
//
// The operator's edits are held here and nowhere else until they ask for them to be applied. The
// session a device serves (BLI-CFG) sees a proposal, its answer, and a confirm or a discard; it never
// sees the typing that came before.

import { pathOf, within } from './path.js'
import { securityName } from './wireless.js'

/// A document is JSON, so a deep copy is a structured clone.
export const clone = (value) => structuredClone(value)

/// Whether two JSON values are the same, whatever order their members are in.
export function same(a, b) {
	if (a === b) return true
	if (Array.isArray(a) || Array.isArray(b)) {
		return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((item, i) => same(item, b[i]))
	}
	if (a && b && typeof a === 'object' && typeof b === 'object') {
		const keys = Object.keys(a).filter((key) => a[key] !== undefined)
		const others = Object.keys(b).filter((key) => b[key] !== undefined)
		return keys.length === others.length && keys.every((key) => same(a[key], b[key]))
	}
	return false
}

// Edits

/// A document held for editing, with a key per candidate that follows it through reordering. Keys are
/// how a candidate is matched to the one it was read as, and so to the state the device reports of it.
export function fromDocument(document, prefix = 'f') {
	const held = clone(document ?? { attachments: [] })
	if (!Array.isArray(held.attachments)) held.attachments = []
	return { document: held, keys: held.attachments.map((_, index) => `${prefix}${index}`) }
}

let added = 0

/// A key for a candidate the operator adds, which no candidate read from the device carries.
export function newKey() {
	added += 1
	return `n${added}`
}

/// Add a candidate at the end of the ordering under `key`.
export function addCandidate(edit, candidate, key) {
	return {
		document: { ...edit.document, attachments: [...edit.document.attachments, candidate] },
		keys: [...edit.keys, key],
	}
}

export function updateCandidate(edit, key, change) {
	const index = edit.keys.indexOf(key)
	if (index === -1) return edit
	const attachments = [...edit.document.attachments]
	attachments[index] = change(clone(attachments[index]))
	return { ...edit, document: { ...edit.document, attachments } }
}

export function removeCandidate(edit, key) {
	const index = edit.keys.indexOf(key)
	if (index === -1) return edit
	return {
		document: { ...edit.document, attachments: edit.document.attachments.filter((_, i) => i !== index) },
		keys: edit.keys.filter((_, i) => i !== index),
	}
}

/// Move the candidate at `from` to `to` in the ordering.
export function moveCandidate(edit, from, to) {
	const count = edit.keys.length
	if (from === to || from < 0 || to < 0 || from >= count || to >= count) return edit
	const move = (list) => {
		const next = [...list]
		const [item] = next.splice(from, 1)
		next.splice(to, 0, item)
		return next
	}
	return {
		document: { ...edit.document, attachments: move(edit.document.attachments) },
		keys: move(edit.keys),
	}
}

/// Set a member of the document, or remove it where `value` is undefined: an absent setting is one the
/// device supplies its own behaviour for (BLI-NET).
export function setMember(edit, member, value) {
	const document = { ...edit.document }
	if (value === undefined) delete document[member]
	else document[member] = value
	return { ...edit, document }
}

/// How many changes the edit holds against the configuration in force: each candidate added, removed
/// or altered, a changed order, and each other setting that differs.
export function changes(edit, inForce, inForceKeys) {
	if (!inForce) return 0
	const before = new Map(inForceKeys.map((key, index) => [key, inForce.attachments?.[index]]))
	let count = 0
	for (const [index, key] of edit.keys.entries()) {
		if (!before.has(key) || !same(before.get(key), edit.document.attachments[index])) count += 1
	}
	count += inForceKeys.filter((key) => !edit.keys.includes(key)).length
	const kept = edit.keys.filter((key) => before.has(key))
	const order = inForceKeys.filter((key) => edit.keys.includes(key))
	if (kept.some((key, index) => key !== order[index])) count += 1
	const members = new Set([...Object.keys(edit.document), ...Object.keys(inForce)])
	members.delete('attachments')
	for (const member of members) {
		if (!same(edit.document[member], inForce[member])) count += 1
	}
	return count
}

// The session

/// A session not yet answered.
export function opening() {
	return {
		status: 'opening',
		stage: 'editing',
		inForce: null,
		inForceKeys: [],
		capabilities: null,
		edit: fromDocument(null),
		proposed: null,
		verify: true,
		runningKeys: [],
		failure: null,
		problem: null,
		confirming: false,
		awaiting: 0,
		wps: null,
		pin: null,
		states: null,
		networks: null,
		spectrum: null,
		act: null,
		selected: null,
		notice: null,
		why: null,
	}
}

/// Whether the operator may edit. Never while a proposal is being verified or is running unconfirmed.
export function writable(state) {
	return state.status === 'open' && (state.stage === 'editing' || state.stage === 'errored')
}

/// Whether the operator may apply unverified: after a failure, while the fields still hold the
/// document that failed (NSCR). An edit is applied as any other, verified.
export function unverifiable(state) {
	return writable(state) && state.stage === 'errored' && !!state.proposed && same(state.edit.document, state.proposed.document)
}

/// Enter editing, filled from the configuration in force.
function editing(state) {
	const edit = fromDocument(state.inForce)
	return {
		...state,
		stage: 'editing',
		edit,
		inForceKeys: edit.keys,
		runningKeys: edit.keys,
		proposed: null,
		verify: true,
		failure: null,
		problem: null,
		confirming: false,
		wps: null,
		pin: null,
		selected: null,
	}
}

/// The key of the candidate a path falls within, where it falls within one.
export function candidateAt(at, keys) {
	return keys.find((_, index) => within(at, pathOf(['attachments', index]))) ?? null
}

export function reduce(state, action) {
	switch (action.type) {
		case 'event':
			return fromDevice(state, action.event)
		case 'restart':
			return opening()
		case 'edit':
			if (!writable(state)) return state
			return { ...state, edit: action.change(state.edit), problem: null }
		case 'select':
			return { ...state, selected: action.key }
		case 'reset':
			return writable(state) ? editing(state) : state
		case 'problem':
			return {
				...state,
				problem: action.problem,
				selected: candidateAt(action.problem.at, state.edit.keys) ?? state.selected,
			}
		case 'proposed':
			return {
				...state,
				stage: 'applying',
				proposed: clone(state.edit),
				verify: action.verify,
				awaiting: state.awaiting + 1,
				failure: null,
				problem: null,
			}
		case 'wps':
			return {
				...state,
				stage: 'applying',
				proposed: null,
				verify: true,
				awaiting: state.awaiting + 1,
				failure: null,
				problem: null,
				wps: action.method,
				pin: null,
				act: null,
			}
		case 'cancelled':
			return editing(state)
		case 'confirming':
			return state.stage === 'applied' ? { ...state, confirming: true } : state
		case 'act':
			return { ...state, act: { type: action.act, interface: action.interface ?? null, failure: null } }
		case 'closed':
			return { ...state, status: state.status === 'busy' ? 'busy' : 'closed', why: action.why ?? null }
		default:
			return state
	}
}

/// One outcome off the session stream, as the protocol half described it.
function fromDevice(state, event) {
	if (event.kind === 'refused') {
		return { ...state, notice: `The device sent something this version of the app cannot act on: ${event.detail}` }
	}
	if (event.kind === 'fault') {
		return { ...state, notice: `The device is not speaking the protocol: ${event.detail}` }
	}
	if (event.kind !== 'message') return state

	const message = event.message
	switch (message.type) {
		case 'configuration':
			return configuration(state, message)
		case 'applied':
		case 'invalid':
			return answer(state, message)
		case 'busy':
			return { ...state, status: 'busy' }
		case 'pin':
			return state.stage === 'applying' && state.wps === 'pin' ? { ...state, pin: String(message.pin) } : state
		case 'networks':
			return { ...state, networks: Array.isArray(message['access-points']) ? message['access-points'] : [], act: null }
		case 'spectrum':
			return { ...state, spectrum: message.spectrum ?? null, act: null }
		// Returning to the recorded configuration can change what the device supports back, and the state
		// after it says so (BLI-CFG).
		case 'state':
			return { ...state, states: readState(message), capabilities: message.capabilities ?? state.capabilities }
		default:
			return state
	}
}

/// An `applied` or an `invalid`. A device answers every proposal exactly once and in order, one
/// interrupted by a discard or a newer proposal with `invalid` at `$` (BLI-CFG), so answers are paired
/// with proposals by counting, and only the answer to the latest counts, and only while it is still
/// wanted. An `invalid` with no proposal awaiting answers an act.
///
/// Whatever it answers, capabilities an `applied` carries are the latest the device sent, and the next
/// proposal is checked against those.
function answer(received, message) {
	const state = { ...received, capabilities: message.capabilities ?? received.capabilities }
	if (state.awaiting === 0) return message.type === 'invalid' ? actFailed(state, message) : state
	const awaiting = state.awaiting - 1
	if (awaiting > 0 || state.stage !== 'applying') return { ...state, awaiting }
	if (message.type === 'invalid') return invalid({ ...state, awaiting }, message)
	return {
		...state,
		awaiting,
		stage: 'applied',
		runningKeys: state.proposed?.keys ?? state.edit.keys,
	}
}

function failureOf(message) {
	return { at: message.at, reason: message.reason, reached: message.reached ?? null }
}

/// An act refused outside a proposal: a survey or a scan the device could not carry out.
function actFailed(state, message) {
	return state.act ? { ...state, act: { ...state.act, failure: failureOf(message) } } : state
}

function configuration(state, message) {
	// The first answer opens the session, and the answer to a confirm is what is now in force. Either
	// way the operator starts again from the configuration in force, and what the device supports is
	// what it said of the configuration it runs.
	const capabilities = message.capabilities ?? state.capabilities
	if (state.status === 'opening' || state.confirming) {
		return editing({ ...state, status: 'open', inForce: message.document, capabilities, states: null })
	}
	// Joining by WPS learns a network, which the device proposes on the operator's behalf.
	if (state.stage === 'applying' && state.wps && !state.proposed) {
		const proposed = fromDocument(message.document, 'w')
		return { ...state, proposed, edit: clone(proposed), capabilities }
	}
	// Anything else is the device telling us what is in force now. An operator with no edits in hand is
	// moved onto it; one with edits keeps them, and they are compared against the new configuration.
	if (state.stage === 'editing' && changes(state.edit, state.inForce, state.inForceKeys) === 0) {
		return editing({ ...state, inForce: message.document, capabilities })
	}
	return { ...state, inForce: message.document, capabilities }
}

function invalid(state, message) {
	const failure = failureOf(message)
	if (!state.proposed) {
		// Joining by WPS failed before the device had a network to propose.
		return { ...editing(state), act: { type: 'wps', failure } }
	}
	const edit = clone(state.proposed)
	return {
		...state,
		stage: 'errored',
		edit,
		failure,
		runningKeys: state.inForceKeys,
		selected: candidateAt(failure.at, edit.keys) ?? state.selected,
	}
}

const STATES = new Set(['default-route', 'up', 'verifying', 'standby', 'unavailable'])

/// A `state` message: what the device observes of each candidate, matched by position to the
/// attachments of the configuration it is running (BLI-CFG). An entry whose `is` this build does not
/// know is kept in its place, so the rest still line up, and left unsaid.
export function readState(message) {
	if (!Array.isArray(message.attachments)) return null
	return message.attachments.map((each) => {
		const is = STATES.has(each?.is) ? each.is : null
		const unavailable = is === 'unavailable'
		return {
			is,
			reached: unavailable && typeof each.reached === 'string' ? each.reached : null,
			reason: unavailable && typeof each.reason === 'string' ? each.reason : null,
		}
	})
}

/// The state the device reports of the candidate held under `key`, or null.
export function stateFor(state, key) {
	const index = state.runningKeys.indexOf(key)
	return index === -1 ? null : (state.states?.[index] ?? null)
}

// Wording

/// A candidate's state as the operator reads it, by what the device observed. Null for a state this
/// build does not know, which is left unsaid rather than guessed at.
export function stateWording(observed, kind) {
	if (!observed) return null
	switch (observed.is) {
		case 'default-route':
			return { text: 'Default route', tone: 'good' }
		case 'up':
			return { text: 'Up', tone: 'good' }
		case 'verifying':
			return { text: 'Checking', tone: 'muted' }
		case 'standby':
			return { text: 'Standby', tone: 'muted' }
		case 'unavailable':
			return { text: unavailable(observed.reached, kind), tone: 'muted' }
		default:
			return null
	}
}

function unavailable(reached, kind) {
	switch (reached) {
		case 'carrier':
			return kind === 'wireless' ? 'Out of range' : 'No link'
		case 'association':
			return 'Could not join'
		case 'addressing':
			return kind === 'wired-static' ? 'No address' : 'No lease'
		case 'gateway':
			return 'No gateway'
		default:
			return 'Unavailable'
	}
}

const STAGES = {
	wireless: ['carrier', 'association', 'addressing', 'gateway'],
	'wired-dynamic': ['carrier', 'addressing', 'gateway'],
	'wired-static': ['carrier', 'addressing', 'gateway'],
}

const STAGE_NAMES = { carrier: 'Link', association: 'Joined', addressing: 'Address', gateway: 'Gateway' }

/// The verification stages of BLI-LINK for a candidate of `kind`, marked by how far an attempt got:
/// passed up to the stage it stopped at, failed there, and not tried after. None where nothing was
/// applied, or the stage is not one this build knows.
export function stagesOf(kind, reached) {
	const order = STAGES[kind] ?? STAGES.wireless
	const stop = order.indexOf(reached)
	if (stop === -1) return []
	return order.map((stage, index) => ({
		stage,
		label: STAGE_NAMES[stage],
		mark: index < stop ? 'done' : index === stop ? 'failed' : 'untried',
	}))
}

/// The line heading a failure, by how far it got.
export function failureHeadline(reached, kind) {
	switch (reached) {
		case null:
		case undefined:
			return 'Not accepted. Nothing was changed.'
		case 'carrier':
			return kind === 'wireless' ? 'Out of range. Reverted.' : 'No link. Reverted.'
		case 'association':
			return 'Could not join. Reverted.'
		case 'addressing':
			return kind === 'wired-static' ? 'No address. Reverted.' : 'No lease. Reverted.'
		case 'gateway':
			return 'Applied, then could not be reached. Reverted.'
		default:
			return 'Applied, then failed. Reverted.'
	}
}

/// The second line of a candidate's row: its kind, and what tells it apart from its neighbours.
export function describeCandidate(candidate) {
	switch (candidate?.kind) {
		case 'wireless':
			return ['Wireless', candidate.security?.kind && securityName(candidate.security.kind)].filter(Boolean).join(', ')
		case 'wired-dynamic':
			return ['Wired', candidate.interface, 'DHCP'].filter(Boolean).join(', ')
		case 'wired-static': {
			const address = candidate.addresses?.[0]?.split('/')[0]
			return ['Wired', candidate.interface, address ? `static ${address}` : 'static'].filter(Boolean).join(', ')
		}
		default:
			return 'Not known to this version of the app'
	}
}

// Checks made before proposing

const IPV4 = /^(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}$/

export function isIpv4(text) {
	return IPV4.test(text)
}

/// An IPv6 address, by whether the browser's own URL parser takes it as a host.
export function isIpv6(text) {
	if (typeof text !== 'string' || !text.includes(':') || /[[\]/]/.test(text)) return false
	try {
		new URL(`http://[${text}]/`)
		return true
	} catch {
		return false
	}
}

export function isAddress(text) {
	return isIpv4(text) || isIpv6(text)
}

/// An address with its prefix length, such as `192.168.1.20/24`.
export function isPrefixed(text, { v6 = true } = {}) {
	const slash = String(text).lastIndexOf('/')
	if (slash === -1) return false
	const address = text.slice(0, slash)
	const prefix = text.slice(slash + 1)
	if (!/^\d{1,3}$/.test(prefix)) return false
	if (isIpv4(address)) return Number(prefix) <= 32
	return v6 && isIpv6(address) && Number(prefix) <= 128
}

/// The first thing in a document that could not work whatever the device supports: a half-typed
/// address, a passphrase too short to be one, a candidate with no name. Null where there is none. The
/// device would reject each of these too, but for a reason that is not about the network.
export function validate(document) {
	const problem = (segments, reason) => ({ at: pathOf(segments), reason })
	const dynamic = new Set()
	for (const [index, candidate] of (document.attachments ?? []).entries()) {
		const at = (...rest) => ['attachments', index, ...rest]
		if (!candidate?.label?.trim()) return problem(at('label'), 'Give it a name.')
		if (candidate.kind === 'wireless') {
			if (!candidate.ssid) return problem(at('ssid'), 'Enter the SSID.')
			if (new TextEncoder().encode(candidate.ssid).length > 32) {
				return problem(at('ssid'), 'An SSID is at most 32 bytes.')
			}
			const found = securityProblem(candidate.security ?? {}, at('security'), problem)
			if (found) return found
		} else if (candidate.kind === 'wired-dynamic' || candidate.kind === 'wired-static') {
			if (!candidate.interface) return problem(at('interface'), 'Choose the interface.')
		}
		if (candidate.kind === 'wired-dynamic') {
			if (dynamic.has(candidate.interface)) {
				return problem(at('interface'), `${candidate.interface} already takes DHCP above this.`)
			}
			dynamic.add(candidate.interface)
		}
		if (candidate.kind === 'wired-static') {
			const addresses = candidate.addresses ?? []
			if (addresses.length === 0) return problem(at('addresses'), 'Enter an address with its prefix, like 192.168.1.20/24.')
			const bad = addresses.findIndex((address) => !isPrefixed(address))
			if (bad !== -1) return problem(at('addresses', bad), `${addresses[bad]} is not an address with its prefix.`)
			if (!isAddress(candidate.gateway ?? '')) return problem(at('gateway'), 'Enter the gateway address.')
		}
		const nameservers = candidate.nameservers ?? []
		const bad = nameservers.findIndex((server) => !isAddress(server))
		if (bad !== -1) return problem(at('nameservers', bad), `${nameservers[bad]} is not an address.`)
	}

	const hotspot = document.hotspot
	if (hotspot) {
		if (!hotspot.ssid) return problem(['hotspot', 'ssid'], 'Enter the SSID.')
		if (!passphraseFits(hotspot.passphrase)) return problem(['hotspot', 'passphrase'], 'A passphrase is 8 to 63 characters.')
		if (hotspot['dhcp-range'] !== undefined && !isPrefixed(hotspot['dhcp-range'], { v6: false })) {
			return problem(['hotspot', 'dhcp-range'], 'Enter a subnet, like 10.42.0.0/24.')
		}
	}
	return null
}

function passphraseFits(passphrase) {
	return typeof passphrase === 'string' && passphrase.length >= 8 && passphrase.length <= 63
}

function securityProblem(security, at, problem) {
	switch (security.kind) {
		case 'psk':
		case 'psk-sae':
			return passphraseFits(security.passphrase) ? null : problem([...at, 'passphrase'], 'A passphrase is 8 to 63 characters.')
		case 'sae':
			return security.passphrase ? null : problem([...at, 'passphrase'], 'Enter the passphrase.')
		case 'enterprise':
			if (!security.eap) return problem([...at, 'eap'], 'Choose the EAP method.')
			if (!security.identity) return problem([...at, 'identity'], 'Enter the identity.')
			// Without the CA the access point is not authenticated, and BLI-WLAN forbids joining it.
			if (!security['ca-certificate']) return problem([...at, 'ca-certificate'], 'Add the CA certificate.')
			return null
		default:
			return problem([...at, 'kind'], 'Choose how the network is secured.')
	}
}

// Passphrases

const WORDS = (
	'acorn amber anchor apple arch atlas autumn badge basil beacon berry birch blade bloom bolt brass ' +
	'breeze brick bridge brook cabin cactus camel candle canyon carbon cedar chalk cherry cider cliff ' +
	'clover cobalt comet copper coral cotton crane creek crystal dawn delta desert dune eagle echo ' +
	'ember falcon fern field flint forest fossil fox frost garden garnet ginger glacier glass granite ' +
	'grape gravel harbor hazel heron hollow honey island ivory jade jasper kettle kiwi lagoon lantern ' +
	'lemon lichen lily linen lotus maple marble meadow mango mint moss nectar needle nutmeg oak oasis ' +
	'ocean olive onyx orbit orchid otter owl paddle palm pebble pepper pine plum pollen poppy prairie ' +
	'quartz quill rain raven reef ridge ripe river robin rocket rose saddle sage salt sand satin ' +
	'shell silver slate smoke sparrow spruce stone storm summit swan thistle thunder tide timber ' +
	'topaz trail tulip valley velvet violet walnut willow wind winter wren yarrow zephyr'
).split(' ')

/// A passphrase meant to be read aloud: four words and a number, drawn from the browser's generator.
export function generatePassphrase() {
	const picks = crypto.getRandomValues(new Uint32Array(5))
	const words = [...picks.slice(0, 4)].map((pick) => WORDS[pick % WORDS.length])
	return `${words.join('-')}-${10 + (picks[4] % 90)}`
}
