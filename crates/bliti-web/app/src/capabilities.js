// Everything this application knows about the shape of a device's capabilities (BLI-NET, BLI-CFG).
//
// The shape is still under review, so this is the only module that reads a capabilities object: the
// screen asks it what to offer, why something is not offered, and whether a document is within what
// the device said it supports, and never looks inside the object itself. A change to the shape is a
// change to this file.
//
// The shape as proposed has three members. `document` mirrors the configuration document: at each
// member, nothing means not supported, `true` any value the document admits, an array exactly those
// values, and an object constrains member by member, or is keyed by a sibling's value where one
// member's values depend on another's (a channel on a band). Attachment and security kinds are keys.
// `radio` is what the hardware is, and `acts` what the device does on request.

import { pathOf } from './path.js'

const KINDS = ['wireless', 'wired-dynamic', 'wired-static']
const SECURITY_KINDS = ['psk-sae', 'sae', 'psk', 'enterprise']

// What an enterprise network may carry. The method decides which of these it needs, so they belong to
// the kind and are not listed in capabilities, where only `eap` is constrained.
const ENTERPRISE = [
	'eap',
	'identity',
	'anonymous-identity',
	'password',
	'phase2',
	'ca-certificate',
	'domain',
	'client-certificate',
	'client-key',
	'client-key-passphrase',
]

// The members each part of a document carries whenever it is present. A kind that is offered supports
// these without listing them; capabilities list only optional members and constrained values.
const REQUIRED = {
	document: ['attachments'],
	wireless: ['kind', 'label', 'ssid', 'security'],
	'wired-dynamic': ['kind', 'label', 'interface'],
	'wired-static': ['kind', 'label', 'interface', 'addresses', 'gateway'],
	hotspot: ['ssid', 'passphrase'],
	psk: ['kind', 'passphrase'],
	sae: ['kind', 'passphrase'],
	'psk-sae': ['kind', 'passphrase'],
	enterprise: ['kind', ...ENTERPRISE],
}

const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value)

function documentOf(capabilities) {
	return isObject(capabilities?.document) ? capabilities.document : {}
}

function kindOf(capabilities, kind) {
	const attachments = documentOf(capabilities).attachments
	if (attachments === true) return true
	return isObject(attachments) ? attachments[kind] : undefined
}

function hotspotOf(capabilities) {
	return documentOf(capabilities).hotspot
}

/// The values a capability admits: null where any value is admitted, the list where only those are,
/// and an empty list where the capability is absent.
function valuesOf(capability) {
	if (capability === true) return null
	if (Array.isArray(capability)) return capability
	return []
}

/// Which kinds of candidate the device accepts, in the order the screen offers them.
export function attachmentKinds(capabilities) {
	return KINDS.filter((kind) => kindOf(capabilities, kind) !== undefined)
}

/// The interfaces a wired candidate of `kind` may name: a list, or null where any name is accepted.
export function interfaces(capabilities, kind) {
	const caps = kindOf(capabilities, kind)
	if (caps === true || !isObject(caps) || caps.interface === undefined) return null
	return valuesOf(caps.interface)
}

/// Whether a candidate of `kind` may carry the optional `member` (`nameservers`, `hidden`).
export function offersMember(capabilities, kind, member) {
	const caps = kindOf(capabilities, kind)
	return caps === true || (isObject(caps) && caps[member] !== undefined)
}

function securityOf(capabilities) {
	const caps = kindOf(capabilities, 'wireless')
	if (caps === true) return true
	return isObject(caps) ? caps.security : undefined
}

/// The security kinds a wireless candidate may use, strongest first.
export function securityKinds(capabilities) {
	const security = securityOf(capabilities)
	if (security === true) return SECURITY_KINDS
	if (!isObject(security)) return []
	return SECURITY_KINDS.filter((kind) => security[kind] !== undefined)
}

/// The EAP methods an enterprise network may use: a list, or null where any is accepted.
export function eapMethods(capabilities) {
	const security = securityOf(capabilities)
	if (security === true) return null
	const enterprise = isObject(security) ? security.enterprise : undefined
	if (enterprise === true) return null
	if (!isObject(enterprise) || enterprise.eap === undefined) return null
	return valuesOf(enterprise.eap)
}

/// The security kind to join a scanned network with, given what its access point advertises, or null
/// where the device cannot join it. Transitional where the network offers both and the device does.
export function joinWith(capabilities, advertised) {
	const offered = securityKinds(capabilities)
	const has = (kind) => Array.isArray(advertised) && advertised.includes(kind)
	const candidates = [
		has('psk') && has('sae') && 'psk-sae',
		has('sae') && 'sae',
		has('psk') && 'psk',
		has('enterprise') && 'enterprise',
	]
	return candidates.find((kind) => kind && offered.includes(kind)) ?? null
}

/// Whether the device can run a hotspot at all.
export function offersHotspot(capabilities) {
	return hotspotOf(capabilities) !== undefined
}

/// Whether the hotspot may carry the optional `member`: `share-upstream`, `isolate-clients`,
/// `dhcp-range`, `band`, `channel` or `channel-width`.
export function offersHotspotSetting(capabilities, member) {
	const caps = hotspotOf(capabilities)
	return caps === true || (isObject(caps) && caps[member] !== undefined)
}

/// The bands a hotspot may be put on.
export function bands(capabilities) {
	const caps = hotspotOf(capabilities)
	if (!isObject(caps)) return []
	return valuesOf(caps.band) ?? []
}

/// The values of a hotspot setting keyed by band (`channel`, `channel-width`) usable on `band`: a list,
/// or null where any value is accepted.
function onBand(capabilities, member, band) {
	const caps = hotspotOf(capabilities)
	if (caps === true) return null
	const setting = isObject(caps) ? caps[member] : undefined
	if (setting === true) return null
	if (Array.isArray(setting)) return setting
	if (isObject(setting)) return band && Array.isArray(setting[band]) ? setting[band] : []
	return []
}

/// The channels a hotspot may use on `band`.
export function channels(capabilities, band) {
	return onBand(capabilities, 'channel', band)
}

/// The channel widths a hotspot may use on `band`, in megahertz.
export function widths(capabilities, band) {
	return onBand(capabilities, 'channel-width', band)
}

/// Whether the country can be set, and to what: null where it cannot, `'any'` where any code is
/// accepted, or the list of codes.
export function countries(capabilities) {
	const caps = documentOf(capabilities)['regulatory-domain']
	if (caps === undefined) return null
	if (caps === true) return 'any'
	return valuesOf(caps)
}

/// What the device will do on request: scan, survey, and the WPS methods it offers.
export function acts(capabilities) {
	const caps = isObject(capabilities?.acts) ? capabilities.acts : {}
	return {
		scan: caps.scan === true,
		survey: caps.survey === true,
		wps: Array.isArray(caps.wps) ? caps.wps : [],
	}
}

function alongside(capabilities) {
	return isObject(capabilities?.radio) ? capabilities.radio.alongside : undefined
}

function hasRadio(capabilities) {
	return isObject(capabilities?.radio)
}

const SENTENCES = {
	'no-radio': 'This device has no wireless radio.',
	'shared-channel': 'The radio runs the hotspot on the same channel as its wireless connection.',
	'one-at-a-time': 'The radio cannot run a hotspot while joined to a wireless network.',
	unreported: 'Not offered by this device.',
}

const RADIO_SETTINGS = new Set(['wireless', 'hotspot', 'regulatory-domain', 'scan', 'survey', 'wps'])
const CHANNEL_SETTINGS = new Set(['hotspot.band', 'hotspot.channel', 'hotspot.channel-width'])

/// Why a setting is not offered, or null where it is. `setting` is one of `wireless`, `hotspot`,
/// `hotspot.<member>`, `regulatory-domain`, `scan`, `survey` or `wps`. The document being edited is
/// what decides a conflict: a radio that runs one thing at a time rules out a hotspot only while a
/// wireless network is in the ordering.
///
/// Returns `{ reason, sentence }`, where `reason` is `no-radio`, `shared-channel`, `one-at-a-time` or
/// `unreported`, and `sentence` is what the screen shows in the setting's place.
export function absent(capabilities, setting, document = null) {
	const why = (reason) => ({ reason, sentence: SENTENCES[reason] })
	if (!offered(capabilities, setting)) {
		if (RADIO_SETTINGS.has(setting) && !hasRadio(capabilities)) return why('no-radio')
		if (CHANNEL_SETTINGS.has(setting) && alongside(capabilities) === 'shared-channel') {
			return why('shared-channel')
		}
		return why('unreported')
	}
	if (alongside(capabilities) === 'one-at-a-time') {
		const wireless = document?.attachments?.some?.((candidate) => candidate?.kind === 'wireless')
		if (setting === 'hotspot' && wireless && !document?.hotspot) return why('one-at-a-time')
		if (setting === 'wireless' && document?.hotspot) return why('one-at-a-time')
	}
	return null
}

function offered(capabilities, setting) {
	if (setting === 'wireless') return kindOf(capabilities, 'wireless') !== undefined
	if (setting === 'hotspot') return offersHotspot(capabilities)
	if (setting === 'regulatory-domain') return countries(capabilities) !== null
	if (setting === 'scan' || setting === 'survey') return acts(capabilities)[setting]
	if (setting === 'wps') return acts(capabilities).wps.length > 0
	if (setting.startsWith('hotspot.')) return offersHotspotSetting(capabilities, setting.slice(8))
	return false
}

/// Whether a document is within what the device said it supports, checked before it is proposed
/// (BLI-NSCR). Null where it is, or the first part that is not: `at` is its Normalized Path, the form a
/// device names the part it rejects by, and `reason` is ours.
export function check(document, capabilities) {
	const caps = documentOf(capabilities)
	const found =
		members(document ?? {}, caps, [], REQUIRED.document, { skip: ['attachments'] }) ??
		attachments(document?.attachments ?? [], caps.attachments)
	if (found) return found

	if (alongside(capabilities) === 'one-at-a-time' && document?.hotspot) {
		if ((document.attachments ?? []).some((candidate) => candidate?.kind === 'wireless')) {
			return fault(['hotspot'], SENTENCES['one-at-a-time'])
		}
	}
	return null
}

function fault(segments, reason) {
	return { at: pathOf(segments), reason }
}

const UNSUPPORTED = 'This device does not support this setting.'

function attachments(candidates, caps) {
	if (caps === true) return null
	for (const [index, candidate] of candidates.entries()) {
		const at = ['attachments', index]
		const kind = candidate?.kind
		const kindCaps = isObject(caps) ? caps[kind] : undefined
		if (kindCaps === undefined) {
			return fault([...at, 'kind'], 'This device does not take a connection of this kind.')
		}
		if (kindCaps === true) continue
		const found = members(candidate, kindCaps, at, REQUIRED[kind], { skip: ['security'] })
		if (found) return found
		if (kind === 'wireless') {
			const found = security(candidate.security, kindCaps.security, [...at, 'security'])
			if (found) return found
		}
	}
	return null
}

function security(value, caps, at) {
	if (caps === true) return null
	const kindCaps = isObject(caps) ? caps[value?.kind] : undefined
	if (kindCaps === undefined) {
		return fault([...at, 'kind'], 'This device cannot join a network secured this way.')
	}
	if (kindCaps === true) return null
	return members(value, kindCaps, at, REQUIRED[value.kind] ?? ['kind'])
}

/// Check each member of `value` against `caps`, which constrains it member by member. A member the
/// capabilities do not name is unsupported unless the part always carries it.
function members(value, caps, at, required = [], { skip = [] } = {}) {
	if (!isObject(value)) return null
	for (const [name, inner] of Object.entries(value)) {
		if (skip.includes(name) || inner === undefined) continue
		if (isObject(caps) && caps[name] !== undefined) {
			const found = cover(inner, caps[name], [...at, name], value, REQUIRED[name])
			if (found) return found
		} else if (!required.includes(name)) {
			return fault([...at, name], UNSUPPORTED)
		}
	}
	return null
}

/// Check one value against the capability at its place in the document.
function cover(value, caps, at, siblings, required) {
	if (caps === true) return null
	if (Array.isArray(caps)) {
		if (Array.isArray(value)) {
			const index = value.findIndex((item) => !caps.includes(item))
			return index === -1 ? null : fault([...at, index], `This device does not support ${value[index]}.`)
		}
		return caps.includes(value) ? null : fault(at, `This device does not support ${value}.`)
	}
	if (isObject(caps)) {
		if (isObject(value)) return members(value, caps, at, required)
		// Keyed by a sibling's value: a channel is checked against the list for the band it sits on.
		const key = Object.values(siblings ?? {}).find(
			(sibling) => typeof sibling === 'string' && caps[sibling] !== undefined,
		)
		if (key !== undefined) return cover(value, caps[key], at, siblings)
		const anywhere = Object.values(caps).some((each) => cover(value, each, at, siblings) === null)
		return anywhere ? null : fault(at, `This device does not support ${value}.`)
	}
	return fault(at, UNSUPPORTED)
}
