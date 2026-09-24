// Everything this application knows about the shape of a device's capabilities (BLI-NET, BLI-CFG).
//
// This is the only module that reads a capabilities object: the screen asks it what to offer, why
// something is not offered, and whether a document is within what the device said it supports, and
// never looks inside the object itself. A change to the shape is a change to this file.
//
// Capabilities carry three members. `document` mirrors the configuration document: at each member,
// nothing means not supported, `true` any value the document admits, an array exactly those values,
// and an object constrains member by member. A member whose value decides what its siblings may
// carry is a selector, mirrored as an object keyed by its values under its own name: an attachment's
// `kind`, a security `kind`, a wireless candidate's or the hotspot's `interface`, and the hotspot's
// `band`. `radios` is what the hardware is, keyed by interface, and `acts` what the device does on
// request, mirroring each act's message as `document` mirrors the document.
//
// Whether a document is within capabilities is answered by the checker the device rejects with,
// compiled in through wasm, so the two cannot disagree. The readers here resolve selectors the same
// way it does: a selector the document sets picks its key, and one left unset admits whatever any of
// its keys admits.

import { pathOf, segmentsOf } from './path.js'
import { check_capabilities as sharedCheck } from './wasm/bliti_web.js'
import { bandName, widthName } from './wireless.js'

const KINDS = ['wireless', 'wired-dynamic', 'wired-static']
const SECURITY_KINDS = ['psk-sae', 'sae', 'psk', 'enterprise']
const WPS_METHODS = ['push-button', 'pin']
const BANDS = ['2.4ghz', '5ghz', '6ghz']
const SELECTORS = ['kind', 'interface', 'band']

const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value)
const unique = (values) => [...new Set(values)]
const unset = (value) => value === undefined || value === null

function documentOf(capabilities) {
	return isObject(capabilities?.document) ? capabilities.document : {}
}

/// Every way of resolving the selectors in `constraints` against `value`, as the checker does: each
/// selector `value` sets picks its key, and each it leaves unset yields one view per key. A view is
/// the constraints left once its selectors are consumed, or `true` where anything goes, with the key
/// each selector took.
function views(constraints, value = {}, consumed = [], chosen = {}) {
	if (constraints === true) return [{ caps: true, chosen }]
	if (!isObject(constraints)) return []
	const selector = SELECTORS.find((name) => !consumed.includes(name) && isObject(constraints[name]))
	if (!selector) return [{ caps: constraints, chosen }]
	const { [selector]: keyed, ...rest } = constraints
	const picked = value?.[selector]
	const keys = unset(picked) ? Object.keys(keyed) : [String(picked)].filter((key) => key in keyed)
	return keys.flatMap((key) =>
		isObject(keyed[key])
			? views({ ...rest, ...keyed[key] }, value, [...consumed, selector], { ...chosen, [selector]: key })
			: [],
	)
}

/// The keys `selector` may take where the rest of `value` stands as it is.
function keysOf(constraints, value, selector) {
	const found = views(constraints, { ...value, [selector]: undefined })
	if (found.some((view) => view.caps === true)) return null
	return unique(found.map((view) => view.chosen[selector]).filter((key) => key !== undefined))
}

/// Whether any view carries `member` at all.
function carries(found, member) {
	return found.some((view) => view.caps === true || view.caps[member] !== undefined)
}

/// The values `member` may take across views: null where any value is admitted, else the union of the
/// lists they carry.
function valuesAcross(found, member) {
	const out = []
	for (const view of found) {
		if (view.caps === true || view.caps[member] === true) return null
		if (Array.isArray(view.caps[member])) out.push(...view.caps[member])
	}
	return unique(out)
}

// What each part of a document is held to

function attachmentsOf(capabilities) {
	return documentOf(capabilities).attachments
}

/// The constraints on a candidate of `kind`, its own selectors not yet resolved.
function kindOf(capabilities, kind) {
	const caps = attachmentsOf(capabilities)
	if (caps === true) return true
	if (!isObject(caps) || !isObject(caps.kind) || !isObject(caps.kind[kind])) return undefined
	const { kind: _, ...rest } = caps
	return { ...rest, ...caps.kind[kind] }
}

function hotspotOf(capabilities) {
	return documentOf(capabilities).hotspot
}

// Radios

/// The device's radios, in the order it reported them: each one's interface, what the adapter is,
/// the bands it can use, and how it runs a hotspot beside a wireless client where it can run one.
export function radios(capabilities) {
	const caps = isObject(capabilities?.radios) ? capabilities.radios : {}
	return Object.entries(caps)
		.filter(([, radio]) => isObject(radio))
		.map(([name, radio]) => ({
			interface: name,
			model: typeof radio.model === 'string' ? radio.model : name,
			bands: Array.isArray(radio.bands) ? radio.bands : [],
			alongside: radio.alongside ?? null,
		}))
}

/// What an operator calls a radio: its model, with the interface added only where two radios share
/// one, which is the one case the model does not tell them apart.
export function adapterName(capabilities, name) {
	const all = radios(capabilities)
	const radio = all.find((each) => each.interface === name)
	if (!radio) return name
	return all.filter((each) => each.model === radio.model).length > 1 ? `${radio.model} (${name})` : radio.model
}

/// The bands any of the device's radios can use, in band order.
export function radioBands(capabilities) {
	const heard = new Set(radios(capabilities).flatMap((radio) => radio.bands))
	return [...BANDS.filter((band) => heard.has(band)), ...[...heard].filter((band) => !BANDS.includes(band))]
}

/// The radios `part` (`wireless` or `hotspot`) may be pinned to, where the device has more than one to
/// choose between; otherwise none, and the device's own choice is the only one.
export function adapters(capabilities, part) {
	if (radios(capabilities).length < 2) return []
	const caps = part === 'hotspot' ? hotspotOf(capabilities) : kindOf(capabilities, 'wireless')
	const keys = keysOf(caps, {}, 'interface')
	return keys ?? radios(capabilities).map((radio) => radio.interface)
}

function radio(capabilities, name) {
	return radios(capabilities).find((each) => each.interface === name) ?? null
}

// Candidates

/// Which kinds of candidate the device accepts, in the order the screen offers them.
export function attachmentKinds(capabilities) {
	const keys = keysOf(attachmentsOf(capabilities), {}, 'kind')
	return KINDS.filter((kind) => keys === null || keys.includes(kind))
}

/// The interfaces a wired candidate of `kind` may name: a list, or null where any name is accepted.
export function interfaces(capabilities, kind) {
	const found = views(kindOf(capabilities, kind))
	if (!carries(found, 'interface') || found.some((view) => view.caps === true)) return null
	return valuesAcross(found, 'interface')
}

/// Whether a candidate of `kind` may carry the optional `member` (`nameservers`, `hidden`), on the
/// adapter `candidate` names or on any where it names none.
export function offersMember(capabilities, kind, member, candidate = {}) {
	return carries(views(kindOf(capabilities, kind), candidate), member)
}

/// The bands a wireless candidate may be held to on the adapter it names, or on any where it names
/// none: empty where the device offers no `bands`, as a device that cannot hold a connection to chosen
/// bands does not (WLAN).
export function wirelessBands(capabilities, candidate = {}) {
	const found = views(kindOf(capabilities, 'wireless'), candidate)
	if (!carries(found, 'bands')) return []
	const listed = valuesAcross(found, 'bands')
	return listed === null ? BANDS : BANDS.filter((band) => listed.includes(band))
}

/// The security constraints a wireless candidate is held to, one view per way its adapter resolves.
function securityViews(capabilities, candidate, security = {}) {
	return views(kindOf(capabilities, 'wireless'), candidate).flatMap((view) =>
		views(view.caps === true ? true : view.caps.security, security),
	)
}

/// The security kinds a wireless candidate may use, strongest first.
export function securityKinds(capabilities, candidate = {}) {
	const found = securityViews(capabilities, candidate)
	if (found.some((view) => view.caps === true)) return SECURITY_KINDS
	const keys = new Set(found.map((view) => view.chosen.kind))
	return SECURITY_KINDS.filter((kind) => keys.has(kind))
}

/// The EAP methods an enterprise network may use: a list, or null where any is accepted.
export function eapMethods(capabilities, candidate = {}) {
	return valuesAcross(securityViews(capabilities, candidate, { kind: 'enterprise' }), 'eap')
}

/// The security kind to join a scanned network with, given what its access points advertise, or null
/// where the device cannot join it. Transitional where the network offers both and the device does.
export function joinWith(capabilities, advertised, candidate = {}) {
	const offered = securityKinds(capabilities, candidate)
	const has = (kind) => Array.isArray(advertised) && advertised.includes(kind)
	const candidates = [
		has('psk') && has('sae') && 'psk-sae',
		has('sae') && 'sae',
		has('psk') && 'psk',
		has('enterprise') && 'enterprise',
	]
	return candidates.find((kind) => kind && offered.includes(kind)) ?? null
}

// The hotspot

/// Whether the device can run a hotspot at all.
export function offersHotspot(capabilities) {
	return views(hotspotOf(capabilities)).length > 0
}

/// Whether the hotspot may carry the optional `member`: `share-upstream`, `isolate-clients`,
/// `dhcp-range`, `band`, `channel` or `channel-width`, on the adapter it names or on any.
export function offersHotspotSetting(capabilities, member, hotspot = {}) {
	if (member === 'band') return bands(capabilities, hotspot).length > 0
	return carries(views(hotspotOf(capabilities), hotspot), member)
}

/// The bands a hotspot may be put on, on the adapter it names or on any.
export function bands(capabilities, hotspot = {}) {
	const keys = keysOf(hotspotOf(capabilities), hotspot, 'band')
	return keys ?? BANDS
}

/// The channels a hotspot may use where it stands: on its adapter and band, or on any where either is
/// unset. A list, or null where any is accepted.
export function channels(capabilities, hotspot = {}) {
	return valuesAcross(views(hotspotOf(capabilities), hotspot), 'channel')
}

/// The channel widths a hotspot may use where it stands, in megahertz.
export function widths(capabilities, hotspot = {}) {
	return valuesAcross(views(hotspotOf(capabilities), hotspot), 'channel-width')
}

// The country

/// Whether the country can be set, and to what: null where it cannot, `'any'` where any code is
/// accepted, or the list of codes.
export function countries(capabilities) {
	const caps = documentOf(capabilities)['regulatory-domain']
	if (caps === undefined) return null
	if (caps === true) return 'any'
	return Array.isArray(caps) ? caps : []
}

// Acts

function actOf(capabilities, act) {
	return isObject(capabilities?.acts) ? capabilities.acts[act] : undefined
}

/// The radios able to carry out `act`, where the device keys it by interface.
function actRadios(capabilities, act) {
	const keys = keysOf(actOf(capabilities, act), {}, 'interface')
	return keys ?? radios(capabilities).map((each) => each.interface)
}

/// What the device will do on request: whether it scans and surveys, and the WPS methods any of its
/// radios offers.
export function acts(capabilities) {
	return {
		scan: views(actOf(capabilities, 'scan')).length > 0,
		survey: views(actOf(capabilities, 'survey')).length > 0,
		wps: wpsMethods(capabilities),
	}
}

/// The radios a scan may be addressed to, one at a time.
export function scanners(capabilities) {
	return actRadios(capabilities, 'scan')
}

/// The radios a survey may be addressed to, one at a time.
export function surveyors(capabilities) {
	return actRadios(capabilities, 'survey')
}

/// The WPS methods offered on the radio named, or on any where none is.
export function wpsMethods(capabilities, name) {
	return methodsIn(views(actOf(capabilities, 'wps'), { interface: name }))
}

/// The WPS methods that may join the network `ssid` names, on the radio named or on any where none
/// is: none where the device joins only whichever network the access point hands over (BLI-CFG).
export function wpsMethodsFor(capabilities, ssid, name) {
	const found = views(actOf(capabilities, 'wps'), { interface: name }).filter(
		(view) =>
			view.caps === true || view.caps.ssid === true || (Array.isArray(view.caps.ssid) && view.caps.ssid.includes(ssid)),
	)
	return methodsIn(found)
}

function methodsIn(found) {
	if (found.length === 0) return []
	const methods = valuesAcross(found, 'method')
	return methods === null ? WPS_METHODS : WPS_METHODS.filter((method) => methods.includes(method))
}

// What is not offered, and why

const SENTENCES = {
	'no-radio': 'This device has no wireless radio.',
	'shared-channel': 'The radio runs the hotspot on the same channel as its wireless connection.',
	'one-at-a-time': 'The radio cannot run a hotspot while joined to a wireless network.',
	unreported: 'Not offered by this device.',
}

const RADIO_SETTINGS = new Set(['wireless', 'hotspot', 'regulatory-domain', 'scan', 'survey', 'wps'])
const CHANNEL_SETTINGS = new Set(['hotspot.band', 'hotspot.channel', 'hotspot.channel-width'])

/// The radios a hotspot could be run on: the one it names, or every one able to.
function hotspotRadios(capabilities, hotspot) {
	if (!unset(hotspot?.interface)) return [hotspot.interface]
	const keys = keysOf(hotspotOf(capabilities), {}, 'interface')
	return keys ?? radios(capabilities).filter((each) => each.alongside).map((each) => each.interface)
}

/// The index of the first wireless candidate that could be carried, with the hotspot, only by one
/// radio running one or the other at a time (BLI-HOT), or null where there is none.
function clash(capabilities, document) {
	const hotspot = document?.hotspot
	const all = radios(capabilities)
	if (!hotspot || all.length === 0) return null
	const aps = hotspotRadios(capabilities, hotspot)
	const alongside = (name) => radio(capabilities, name)?.alongside
	for (const [index, candidate] of (document.attachments ?? []).entries()) {
		if (candidate?.kind !== 'wireless') continue
		const stations = unset(candidate.interface) ? all.map((each) => each.interface) : [candidate.interface]
		const apart = stations.some((station) => aps.some((ap) => ap !== station || alongside(ap) !== 'one-at-a-time'))
		if (!apart) return index
	}
	return null
}

/// Why a setting is not offered, or null where it is. `setting` is one of `wireless`, `hotspot`,
/// `hotspot.<member>`, `regulatory-domain`, `scan`, `survey` or `wps`. The document being edited
/// decides a conflict and which adapter the hotspot is on: a radio that runs one thing at a time rules
/// out a hotspot only while a wireless network needs that radio, and a band is missing for the reason
/// its adapter gives.
///
/// Returns `{ reason, sentence }`, where `reason` is `no-radio`, `shared-channel`, `one-at-a-time` or
/// `unreported`, and `sentence` is what the screen shows in the setting's place.
export function absent(capabilities, setting, document = null) {
	const why = (reason, sentence = SENTENCES[reason]) => ({ reason, sentence })
	const hotspot = document?.hotspot ?? {}
	if (!offered(capabilities, setting, hotspot)) {
		if (RADIO_SETTINGS.has(setting) && radios(capabilities).length === 0) return why('no-radio')
		if (CHANNEL_SETTINGS.has(setting)) {
			const names = hotspotRadios(capabilities, hotspot)
			if (names.length > 0 && names.every((name) => radio(capabilities, name)?.alongside === 'shared-channel')) {
				const sentence =
					radios(capabilities).length > 1 && names.length === 1
						? `The ${adapterName(capabilities, names[0])} runs the hotspot on the same channel as its wireless connection.`
						: SENTENCES['shared-channel']
				return why('shared-channel', sentence)
			}
		}
		return why('unreported')
	}
	if (setting === 'hotspot' && !document?.hotspot && clash(capabilities, { ...document, hotspot: {} }) !== null) {
		return why('one-at-a-time')
	}
	if (setting === 'wireless' && document?.hotspot) {
		const attachments = [...(document.attachments ?? []), { kind: 'wireless' }]
		if (clash(capabilities, { ...document, attachments }) !== null) return why('one-at-a-time')
	}
	return null
}

function offered(capabilities, setting, hotspot) {
	if (setting === 'wireless') return kindOf(capabilities, 'wireless') !== undefined
	if (setting === 'hotspot') return offersHotspot(capabilities)
	if (setting === 'regulatory-domain') return countries(capabilities) !== null
	if (setting === 'scan' || setting === 'survey') return acts(capabilities)[setting]
	if (setting === 'wps') return acts(capabilities).wps.length > 0
	if (setting.startsWith('hotspot.')) return offersHotspotSetting(capabilities, setting.slice(8), hotspot)
	return false
}

// Checking before proposing

/// Whether a document is within what the device said it supports, checked before it is proposed
/// (BLI-NSCR). Null where it is, or the first part that is not: `at` is its Normalized Path, the form a
/// device names the part it rejects by, and `reason` is ours.
///
/// The mirror rule is the device's own checker; a hotspot and a wireless network that only a radio
/// running one at a time could carry together turns on the radios, which that checker does not read.
export function check(document, capabilities) {
	const found = sharedCheck(document ?? { attachments: [] }, documentOf(capabilities))
	if (found) return { at: found.at, reason: wording(found.at, document) }
	if (clash(capabilities, document) !== null) {
		const at = unset(document.hotspot.interface) ? ['hotspot'] : ['hotspot', 'interface']
		return { at: pathOf(at), reason: SENTENCES['one-at-a-time'] }
	}
	return null
}

const UNSUPPORTED = 'This device does not support this setting.'

/// Our sentence for the part of a document the checker named.
function wording(at, document) {
	const segments = segmentsOf(at) ?? []
	const last = segments.at(-1)
	const inSecurity = segments.at(-2) === 'security'
	if (segments[0] === 'attachments' && segments.length === 3 && last === 'kind') {
		return 'This device does not take a connection of this kind.'
	}
	if (inSecurity && last === 'kind') return 'This device cannot join a network secured this way.'
	const value = segments.reduce((node, segment) => node?.[segment], document)
	if (last === 'band' && typeof value === 'string') return `This device does not support ${bandName(value)} here.`
	if (last === 'channel-width' && typeof value === 'number') return `This device does not support ${widthName(value)} here.`
	if (typeof value === 'string' || typeof value === 'number') return `This device does not support ${value}.`
	return UNSUPPORTED
}
