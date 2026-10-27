// The simple network screens' logic (BLI-NSET, BLI-WIFI, BLI-SHARE), kept apart from their rendering
// so the states, the moves between them and the wording can be read and tested without a page.
//
// A simple screen never edits a document field by field. Each thing the operator does is a move: it
// takes the configuration the device holds and returns the one to propose, within the simple states.

import { joinWith, offersHotspotSetting, offersMember, securityKinds } from './capabilities.js'
import { pathOf, within } from './path.js'
import { hasValue, isEnded } from './readings.js'
import { waysInUse } from './status.js'

const WIRED = new Set(['wired-dynamic', 'wired-static'])

/// The simple states of NSET.
export const CABLE = 'cable'
export const CABLE_HOTSPOT = 'cable-hotspot'
export const WIFI = 'wifi'

const isOn = (thing) => !!thing && thing.enabled === true

/// The simple state `document` is in, or null where it is outside them. A configuration carrying no
/// wired candidate is outside them, as none can use the cable.
export function simpleState(document) {
	const attachments = document?.attachments ?? []
	if (attachments.some((each) => !WIRED.has(each?.kind) && each?.kind !== 'wireless')) return null
	const wired = attachments.filter((each) => WIRED.has(each.kind))
	const wireless = attachments.filter((each) => each.kind === 'wireless')
	if (wired.length === 0 || wireless.length > 1) return null
	const on = wired.filter(isOn).length
	if (on !== 0 && on !== wired.length) return null
	const cable = on > 0
	const wifi = isOn(wireless[0])
	const hotspot = isOn(document.hotspot)
	if (cable && !wifi) return hotspot ? CABLE_HOTSPOT : CABLE
	if (!cable && wifi && !hotspot) return WIFI
	return null
}

/// Whether the cable, wifi and the hotspot are on in `document`, as NSET's terms have them.
export function waysOn(document) {
	const state = simpleState(document)
	return { cable: state === CABLE || state === CABLE_HOTSPOT, wifi: state === WIFI, hotspot: state === CABLE_HOTSPOT }
}

/// The one wireless candidate of `document`, and where it sits in the ordering. Index -1 where there
/// is none.
export function wifiOf(document) {
	const index = (document?.attachments ?? []).findIndex((each) => each.kind === 'wireless')
	return { index, candidate: index === -1 ? null : document.attachments[index] }
}

// Moves. Each turns a thing off by `enabled` false, keeping it, and leaves every other member as it is.

const turned = (thing, enabled) => (thing.enabled === enabled ? thing : { ...thing, enabled })

function withWays(document, { cable, wifi, hotspot }) {
	const attachments = (document.attachments ?? []).map((each) => {
		if (cable !== undefined && WIRED.has(each.kind)) return turned(each, cable)
		if (wifi !== undefined && each.kind === 'wireless') return turned(each, wifi)
		return each
	})
	const next = { ...document, attachments }
	if (hotspot !== undefined && document.hotspot) next.hotspot = turned(document.hotspot, hotspot)
	return next
}

/// Join a wireless network: the cable and the hotspot off, and `candidate` in wifi's place, or at the
/// end where there is no wifi (WIFI).
export function joinWifi(document, candidate) {
	const next = withWays(document, { cable: false, hotspot: false })
	const { index } = wifiOf(next)
	const attachments = [...next.attachments]
	if (index === -1) attachments.push(candidate)
	else attachments[index] = candidate
	return { ...next, attachments }
}

/// Use the cable: the cable on and wifi off, the hotspot as it is.
export function useCable(document) {
	return withWays(document, { cable: true, wifi: false })
}

/// Turn the hotspot on, from `fresh` where the configuration carries none: the cable on and wifi off.
export function startHotspot(document, fresh) {
	const next = withWays(document, { cable: true, wifi: false })
	return { ...next, hotspot: turned(next.hotspot ?? fresh, true) }
}

export function stopHotspot(document) {
	return withWays(document, { hotspot: false })
}

/// The hotspot with a new passphrase, on or off as it is, set up from `fresh` and off where the
/// configuration carries none (SHARE).
export function setHotspotPassphrase(document, passphrase, fresh) {
	const hotspot = document.hotspot ?? turned(fresh, false)
	return { ...document, hotspot: { ...hotspot, passphrase } }
}

// What a move proposes

/// The wireless candidate joining `ssid` carries (WIFI): `named` where the operator typed the name
/// rather than picking it from a scan.
export function wifiCandidate(capabilities, { ssid, kind, passphrase, named }) {
	const candidate = { kind: 'wireless', label: ssid, enabled: true, verify: true, ssid, security: { kind, passphrase } }
	if (offersMember(capabilities, 'wireless', 'hidden')) candidate.hidden = named
	return candidate
}

/// A hotspot not yet set up, named `ssid` and joined by `passphrase`, off, sharing the device's
/// network and keeping its clients apart where the device offers either.
export function freshHotspot(capabilities, ssid, passphrase) {
	const hotspot = { enabled: false, ssid, passphrase }
	if (offersHotspotSetting(capabilities, 'share-upstream')) hotspot['share-upstream'] = true
	if (offersHotspotSetting(capabilities, 'isolate-clients')) hotspot['isolate-clients'] = true
	return hotspot
}

/// A name as an SSID: at most 32 bytes, cut at a character.
export function ssidOf(name) {
	const encoder = new TextEncoder()
	let out = ''
	for (const char of String(name)) {
		if (encoder.encode(out + char).length > 32) break
		out += char
	}
	return out
}

/// The security kind to join a network the operator named by: the first of `psk-sae`, `psk` and
/// `sae` the device offers.
export function namedSecurity(capabilities) {
	const offered = securityKinds(capabilities)
	return ['psk-sae', 'psk', 'sae'].find((kind) => offered.includes(kind)) ?? null
}

// What the screens show

/// The networks a scan heard, one per SSID: the strongest signal among its access points, what they
/// advertise between them, and the kind to join it by: null where the device cannot join it, and
/// `enterprise` where it needs the settings kept under Advanced. Strongest first; access points with
/// no SSID are left out.
export function networksHeard(points, capabilities) {
	const bySsid = new Map()
	for (const point of points ?? []) {
		if (typeof point?.ssid !== 'string' || point.ssid === '') continue
		const held = bySsid.get(point.ssid) ?? { ssid: point.ssid, signal: -Infinity, advertised: new Set() }
		held.signal = Math.max(held.signal, Number(point.signal))
		for (const kind of point.security ?? []) held.advertised.add(kind)
		bySsid.set(point.ssid, held)
	}
	return [...bySsid.values()]
		.map(({ ssid, signal, advertised }) => ({ ssid, signal, security: joinWith(capabilities, [...advertised]) }))
		.sort((a, b) => b.signal - a.signal || (a.ssid < b.ssid ? -1 : a.ssid > b.ssid ? 1 : 0))
}

/// A signal strength in dBm as a word, with how many of four bars it fills.
export function signalOf(dbm) {
	if (dbm >= -55) return { word: 'Strong', bars: 4 }
	if (dbm >= -67) return { word: 'Good', bars: 3 }
	if (dbm >= -75) return { word: 'Fair', bars: 2 }
	return { word: 'Weak', bars: 1 }
}

/// How many devices are joined to the hotspot, from `hotspot-clients`, or null where it carries none.
export function hotspotClients(entries) {
	const clients = entries.find((entry) => !entry.fact && entry.name === 'hotspot-clients' && !isEnded(entry))
	return hasValue(clients) ? Number(clients.value) : null
}

export function devices(count) {
	if (count === 0) return 'No devices'
	return count === 1 ? '1 device' : `${count} devices`
}

/// Whether a wired interface reports a cable plugged in.
export function cablePlugged(entries) {
	return entries.some((entry) => entry.fact && entry.name === 'wired-carrier' && entry.value === true && !isEnded(entry))
}

/// The networks the device's wireless interfaces are joined to, from `wireless-network`.
export function joinedNetworks(entries) {
	return entries
		.filter((entry) => entry.fact && entry.name === 'wireless-network' && hasValue(entry) && !isEnded(entry))
		.map((entry) => String(entry.value))
}

/// Each way of the Network screen: whether it is on, and how it stands now as parts of a line (NSET).
export function waysOf(document, entries) {
	const on = waysOn(document)
	const inUse = waysInUse(entries)
	const plugged = cablePlugged(entries)
	const cable = (() => {
		if (inUse.cable.length > 0) return ['Connected']
		if (on.cable && plugged) return ['Plugged in, no connection']
		if (plugged) return ['Plugged in']
		return ['No cable']
	})()
	const wifi = (() => {
		if (inUse.wifi.length > 0) return ['Connected', inUse.wifi[0].network]
		return on.wifi ? ['Not connected'] : ['Off']
	})()
	const clients = hotspotClients(entries)
	const hotspot = on.hotspot ? ['On', clients !== null && devices(clients)].filter(Boolean) : ['Off']
	return {
		cable: { on: on.cable, says: cable },
		wifi: { on: on.wifi, says: wifi },
		hotspot: { on: on.hotspot, says: hotspot },
	}
}

/// What a failure of joining wifi, held at `index` of the ordering, says to do (WIFI): `passphrase`
/// where the passphrase was refused, `range` where the network was not heard, and `other` otherwise.
export function joinFailure(failure, index) {
	if (!failure) return null
	if (failure.at === pathOf(['attachments', index, 'security', 'passphrase'])) return 'passphrase'
	if (failure.reached === 'carrier' && within(failure.at, pathOf(['attachments', index]))) return 'range'
	return 'other'
}
