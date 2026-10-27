// The network configuration screen's logic (BLI-NSCR), kept apart from its rendering so the stages,
// the checks and the wording can be read and tested without a page.
//
// The operator's edits are held here and nowhere else until they ask for them to be applied. The
// session a device serves (BLI-CFG) sees a proposal and what the device then reports of the
// configuration it holds; it never sees the typing that came before.

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

/// The candidate under `key` with checking turned off or back on: one not checked cannot fail a
/// proposal, and is otherwise brought up as any other (BLI-CFG).
export function checking(edit, key, verify) {
	return updateCandidate(edit, key, (candidate) => ({ ...candidate, verify }))
}

/// The candidate under `key` turned off or back on: one turned off keeps what it carries, and is not
/// brought up (BLI-LINK).
export function turning(edit, key, enabled) {
	return updateCandidate(edit, key, (candidate) => ({ ...candidate, enabled }))
}

/// The hotspot turned off or back on: one turned off keeps what it carries, and is not run
/// (BLI-HOT). One turned off with settings that could not work on any device is removed instead, so
/// what is kept off is a hotspot that could run (NSCR).
export function turningHotspot(edit, enabled) {
	const hotspot = edit.document.hotspot
	if (!enabled && hotspotProblem(hotspot)) return setMember(edit, 'hotspot', undefined)
	return setMember(edit, 'hotspot', { ...hotspot, enabled })
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
		// The document last proposed, or the held one while its failure is shown, which a failure may
		// be applied again from (NSCR).
		proposed: null,
		// Whether a proposal is sent and the device has not yet sent a configuration after it.
		awaiting: false,
		runningKeys: [],
		busy: false,
		// What the device reports of the configuration it holds: how it is faring, and why not where it
		// failed.
		outcome: null,
		held: null,
		// What the screen renders as a failure, and whether the device refused the proposal rather than
		// saving it.
		failure: null,
		refused: false,
		problem: null,
		states: null,
		networks: null,
		spectrum: null,
		act: null,
		selected: null,
		notice: null,
		why: null,
		// The operator's edits, kept while the session opens again on a reconnected channel.
		kept: null,
	}
}

/// Whether the operator may edit. Never while a proposal is being verified, nor while another session
/// holds the configuration.
export function writable(state) {
	return state.status === 'open' && (state.stage === 'editing' || state.stage === 'errored')
}

/// Whether the fields hold edits the device does not hold, which leaving the screen loses (NSCR).
export function unapplied(state) {
	return writable(state) && changes(state.edit, state.inForce, state.inForceKeys) > 0
}

/// The key of the candidate whose checking failed the last proposal, which the operator may apply
/// again without checking (NSCR). Null for a failure that is not a candidate's checking.
export function uncheckable(state) {
	if (!writable(state) || state.stage !== 'errored' || !state.failure?.reached || !state.proposed) return null
	return candidateAt(state.failure.at, state.proposed.keys)
}

/// Whether the fields still hold the document that failed, which is all that may be applied again
/// without checking; an edit since is applied as any other.
export function unedited(state) {
	return !!state.proposed && same(state.edit.document, state.proposed.document)
}

/// The stage the device's outcome puts the operator in: applying while it verifies, errored where it
/// failed, editing once it applied, and watching whatever the outcome while another session holds it.
function settled(state) {
	if (state.busy) return { ...state, stage: 'watching', failure: null, refused: false, proposed: null }
	switch (state.outcome) {
		case 'verifying':
			return { ...state, stage: 'applying', failure: null, refused: false }
		case 'failed':
			return {
				...state,
				stage: 'errored',
				failure: state.held,
				refused: false,
				proposed: state.proposed ?? clone(state.edit),
				selected: candidateAt(state.held?.at, state.edit.keys) ?? state.selected,
			}
		default:
			return { ...state, stage: 'editing', failure: null, refused: false, proposed: null }
	}
}

/// Fill the fields from the configuration the device holds, and take the stage from its outcome.
function editing(state) {
	const edit = fromDocument(state.inForce)
	return settled({
		...state,
		edit,
		inForceKeys: edit.keys,
		runningKeys: edit.keys,
		awaiting: false,
		proposed: null,
		problem: null,
		selected: null,
	})
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
			return { ...opening(), retried: true }
		case 'resume':
			return resumed(state)
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
				edit: action.edit,
				proposed: clone(action.edit),
				awaiting: true,
				failure: null,
				refused: false,
				problem: null,
				// What a scan or survey failed with belongs to before this attempt.
				act: null,
			}
		case 'act':
			return { ...state, act: { type: action.act, interface: action.interface ?? null, failure: null } }
		case 'closed':
			return {
				...state,
				status: 'closed',
				why: action.why ?? null,
				// Starting again that fails before a session opens says the channel under it is gone.
				unreachable: state.status === 'opening' && !!state.retried,
			}
		default:
			return state
	}
}

/// A session opening again on a channel reconnected under the screen, keeping the operator's edits
/// to hold against what the device holds once it is open (NSCR, WEB).
function resumed(state) {
	const edited =
		(state.stage === 'editing' || state.stage === 'errored') &&
		state.inForce !== null &&
		changes(state.edit, state.inForce, state.inForceKeys) > 0
	const kept = edited ? { edit: state.edit, inForceKeys: state.inForceKeys, selected: state.selected } : null
	return { ...opening(), kept }
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
		case 'state':
			return reported(state, message)
		case 'cannot-configure':
			return refusal(state, message)
		case 'cannot-act':
			return actFailed(state, message)
		case 'networks':
			return { ...state, networks: Array.isArray(message['access-points']) ? message['access-points'] : [], act: null }
		case 'spectrum':
			return { ...state, spectrum: message.spectrum ?? null, act: null }
		default:
			return state
	}
}

function failureOf(message) {
	return { at: message.at, reason: message.reason, reached: message.reached ?? null }
}

/// An act the device could not carry out: a survey or a scan, named by `act` (CFG).
function actFailed(state, message) {
	return state.act?.type === message.act ? { ...state, act: { ...state.act, failure: failureOf(message) } } : state
}

/// A proposal the device did not take. It is kept in the fields, with the reason beside what is at
/// fault, and the device sends the configuration it holds, unchanged, after it (CFG).
function refusal(state, message) {
	if (!state.awaiting) return state
	return {
		...state,
		stage: 'errored',
		edit: clone(state.proposed),
		awaiting: false,
		failure: failureOf(message),
		refused: true,
		selected: candidateAt(message.at, state.proposed.keys) ?? state.selected,
	}
}

/// A `configuration` the device sent: what it holds, on opening and whenever that changes. A proposal
/// is saved where the first one after it carries the same document, and not saved otherwise (CFG).
function configuration(state, message) {
	const capabilities = message.capabilities ?? state.capabilities
	const held = { ...state, inForce: message.document, capabilities }
	if (state.status === 'opening') return { ...held, edit: fromDocument(message.document) }
	if (state.awaiting) {
		if (!same(message.document, state.proposed.document)) {
			return {
				...held,
				stage: 'errored',
				awaiting: false,
				failure: { at: '$', reason: 'The device did not keep these settings.', reached: null },
				refused: true,
			}
		}
		// What the device holds is what was proposed, so its candidates are the fields', in their order.
		return {
			...held,
			awaiting: false,
			inForceKeys: state.proposed.keys,
			runningKeys: state.proposed.keys,
			states: null,
			outcome: null,
		}
	}
	// Anything else is the device telling us what it holds now. One watching follows it; an operator
	// with no edits in hand is moved onto it; one with edits keeps them, and they are compared
	// against the new configuration.
	if (state.stage === 'watching' || (state.stage === 'editing' && changes(state.edit, state.inForce, state.inForceKeys) === 0)) {
		return editing(held)
	}
	return held
}

/// A `state` the device sent: whether the session is busy, how the configuration it holds is faring,
/// and the state of each candidate.
function reported(received, message) {
	const outcome = ['verifying', 'applied', 'failed'].includes(message.outcome) ? message.outcome : null
	const failure = outcome === 'failed' && message.failure ? failureOf(message.failure) : null
	const state = {
		...received,
		busy: message.busy === true,
		outcome,
		held: failure,
		states: readState(message),
		capabilities: message.capabilities ?? received.capabilities,
	}
	if (state.status === 'opening') {
		if (!state.inForce) return state
		const open = editing({ ...state, status: 'open', kept: null })
		// Edits kept across a reconnect stay where the session lets the operator edit (NSCR).
		const { kept } = state
		return kept && writable(open)
			? { ...open, edit: kept.edit, inForceKeys: kept.inForceKeys, selected: kept.selected }
			: open
	}
	// A state before the configuration after a proposal speaks of the one before it.
	if (state.awaiting) return state
	if (state.busy !== received.busy) return editing(state)
	// The stage follows the outcome as it changes, so a refusal stands through the state that follows
	// the configuration the device sends back after it (CFG).
	return outcome === received.outcome ? state : settled(state)
}

const STATES = new Set(['default-route', 'up', 'verifying', 'standby', 'off', 'unavailable'])

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
		case 'off':
			return { text: 'Off', tone: 'muted' }
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

/// The line heading a failure, by how far it got. A refusal is a proposal the device did not take,
/// so nothing was changed; any other failure is of settings the device holds.
export function failureHeadline(reached, kind, refused = false) {
	if (refused) return 'Not accepted. Nothing was changed.'
	switch (reached) {
		case null:
		case undefined:
			return 'Applied, but part of it could not run.'
		case 'carrier':
			return kind === 'wireless' ? 'Out of range.' : 'No link.'
		case 'association':
			return 'Could not join.'
		case 'addressing':
			return kind === 'wired-static' ? 'No address.' : 'No lease.'
		case 'gateway':
			return 'Applied, then could not be reached.'
		default:
			return 'Applied, then failed.'
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

	return document.hotspot ? hotspotProblem(document.hotspot) : null
}

/// What in a hotspot could not work whatever the device supports, turned on or off, as `validate`
/// has it. Null where there is nothing.
function hotspotProblem(hotspot) {
	const problem = (member, reason) => ({ at: pathOf(['hotspot', member]), reason })
	if (!hotspot.ssid) return problem('ssid', 'Enter the SSID.')
	if (new TextEncoder().encode(hotspot.ssid).length > 32) return problem('ssid', 'An SSID is at most 32 bytes.')
	if (!passphraseFits(hotspot.passphrase)) return problem('passphrase', 'A passphrase is 8 to 63 characters.')
	if (hotspot['dhcp-range'] !== undefined && !isPrefixed(hotspot['dhcp-range'], { v6: false })) {
		return problem('dhcp-range', 'Enter a subnet, like 10.42.0.0/24.')
	}
	return null
}

/// Whether a passphrase has from 8 to 63 characters, as every key-based network and hotspot needs.
export function passphraseFits(passphrase) {
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
