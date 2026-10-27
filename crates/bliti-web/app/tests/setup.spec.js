// The simple states of NSET and the moves between them, as plain functions: no page, because none of
// it touches one.

import { readFileSync } from 'node:fs'

import { expect, test } from '@playwright/test'

import { validate } from '../src/network.js'
import {
	freshHotspot,
	joinFailure,
	joinWifi,
	namedSecurity,
	networksHeard,
	setHotspotPassphrase,
	signalOf,
	ssidOf,
	startHotspot,
	stopHotspot,
	useCable,
	waysOf,
	waysOn,
	wifiCandidate,
} from '../src/setup.js'
import { initSync } from '../src/wasm/bliti_web.js'
import { PI, TWO_RADIOS, WIRED_ONLY, ap } from './network-fixtures.js'

// The capabilities readers resolve the hotspot's channel choice through the core, so the module is
// loaded as the page loads it.
initSync({ module: readFileSync(new URL('../src/wasm/bliti_web_bg.wasm', import.meta.url)) })

const dynamic = (enabled = true) => ({ kind: 'wired-dynamic', label: 'eth0', enabled, verify: true, interface: 'eth0' })
const fixed = (enabled = true) => ({
	kind: 'wired-static',
	label: 'Wall port',
	enabled,
	verify: true,
	interface: 'eth0',
	addresses: ['10.4.2.20/24'],
	gateway: '10.4.2.1',
})
const wireless = (enabled = true, ssid = 'Clinic-Staff') => ({
	kind: 'wireless',
	label: ssid,
	enabled,
	verify: true,
	ssid,
	security: { kind: 'sae', passphrase: 'correct horse battery' },
	nameservers: ['1.1.1.1'],
})
const hotspot = (enabled = true) => ({ enabled, ssid: 'clinic-iti-07', passphrase: 'reef-otter-glass-71', 'share-upstream': true })
const doc = (attachments, extra = {}) => ({ attachments, 'regulatory-domain': 'VU', ...extra })

const FRESH = { enabled: false, ssid: 'clinic-iti-07', passphrase: 'amber-basil-cedar-dune-42' }

const CABLED = doc([dynamic(), fixed(), wireless(false)])
const SHARING = doc([dynamic(), fixed(), wireless(false)], { hotspot: hotspot() })
const ON_WIFI = doc([dynamic(false), fixed(false), wireless()], { hotspot: hotspot(false) })
const BOTH = doc([dynamic(), fixed(), wireless()], { hotspot: hotspot(false) })

// What is on, as `waysOn` gives it, in a line.
const on = (document) => {
	const ways = waysOn(document)
	return ways && ['cable', 'wifi', 'hotspot'].filter((way) => ways[way]).join(' and ')
}

const JOINING = wifiCandidate(PI, { ssid: 'Clinic-Guest', kind: 'psk', passphrase: 'guest-pass-1', named: false })

test.describe('the simple states', () => {
	test('take the cable, wifi and the hotspot on or off, the cable and wifi together', () => {
		expect(on(CABLED)).toBe('cable')
		expect(on(SHARING)).toBe('cable and hotspot')
		expect(on(ON_WIFI)).toBe('wifi')
		expect(on(BOTH)).toBe('cable and wifi')
		expect(on(doc([dynamic(false), wireless(false)]))).toBe('')
	})

	test('take a configuration never saved as the cable on', () => {
		expect(on({ attachments: [dynamic()] })).toBe('cable')
	})

	test('take an absent wifi and hotspot as off', () => {
		expect(on(doc([dynamic()]))).toBe('cable')
		expect(on(doc([dynamic(false), wireless()]))).toBe('wifi')
	})

	test('leave out a configuration a technician set up', () => {
		const outside = {
			'two wireless candidates': doc([dynamic(false), wireless(), wireless(false, 'BackupLink')]),
			'wifi and the hotspot both on': doc([dynamic(false), wireless()], { hotspot: hotspot() }),
			'wired candidates partly on': doc([dynamic(), fixed(false)]),
			'no wired candidate': doc([wireless()]),
			'a kind this build does not know': doc([dynamic(), { kind: 'cellular', label: 'SIM', enabled: true }]),
		}
		for (const [what, document] of Object.entries(outside)) expect(waysOn(document), what).toBeNull()
	})
})

test.describe('moving between states', () => {
	const STATES = { cable: CABLED, 'cable and hotspot': SHARING, wifi: ON_WIFI, 'cable and wifi': BOTH }
	const LANDS = {
		join: { cable: 'cable and wifi', 'cable and hotspot': 'cable and wifi', wifi: 'wifi', 'cable and wifi': 'cable and wifi' },
		cable: { cable: 'cable', 'cable and hotspot': 'cable and hotspot', wifi: 'cable and wifi', 'cable and wifi': 'cable and wifi' },
		'hotspot on': { cable: 'cable and hotspot', 'cable and hotspot': 'cable and hotspot', wifi: 'cable and hotspot', 'cable and wifi': 'cable and hotspot' },
		'hotspot off': { cable: 'cable', 'cable and hotspot': 'cable', wifi: 'wifi', 'cable and wifi': 'cable and wifi' },
	}
	const MOVES = {
		join: (document) => joinWifi(document, JOINING),
		cable: useCable,
		'hotspot on': (document) => startHotspot(document, FRESH),
		'hotspot off': stopHotspot,
	}

	test('lands every move from every simple state where NSET has it', () => {
		for (const [move, lands] of Object.entries(LANDS)) {
			for (const [from, document] of Object.entries(STATES)) {
				expect(on(MOVES[move](document)), `${move} from ${from}`).toBe(lands[from])
			}
		}
	})

	test('turns things off by enabled, keeping their settings', () => {
		const moved = startHotspot(BOTH, FRESH)
		expect(moved.attachments[2]).toEqual({ ...wireless(), enabled: false })
		expect(stopHotspot(SHARING).hotspot).toEqual(hotspot(false))
		expect(useCable(ON_WIFI).attachments[1]).toEqual(fixed())
	})

	test('leaves the other uplink as it is', () => {
		expect(joinWifi(CABLED, JOINING).attachments.slice(0, 2)).toEqual([dynamic(), fixed()])
		expect(joinWifi(ON_WIFI, JOINING).attachments.slice(0, 2)).toEqual([dynamic(false), fixed(false)])
		expect(useCable(ON_WIFI).attachments[2]).toEqual(wireless())
	})

	test('leaves every other member as it is', () => {
		const moved = joinWifi({ ...CABLED, extra: { kept: true } }, JOINING)
		expect(moved['regulatory-domain']).toBe('VU')
		expect(moved.extra).toEqual({ kept: true })
		expect(useCable(CABLED)).toEqual(CABLED)
	})

	test('replaces wifi in place, dropping the old network', () => {
		const document = doc([dynamic(false), wireless(), fixed(false)])
		const moved = joinWifi(document, JOINING)
		expect(moved.attachments.map((each) => each.kind)).toEqual(['wired-dynamic', 'wireless', 'wired-static'])
		expect(moved.attachments[1]).toEqual(JOINING)
	})

	test('adds wifi at the end where there is none', () => {
		const moved = joinWifi(doc([dynamic(), fixed()]), JOINING)
		expect(moved.attachments).toHaveLength(3)
		expect(moved.attachments[2]).toEqual(JOINING)
	})

	test('sets up a hotspot not yet set up from what the screen shows', () => {
		const moved = startHotspot(CABLED, FRESH)
		expect(moved.hotspot).toEqual({ ...FRESH, enabled: true })
		expect(startHotspot(SHARING, FRESH).hotspot).toEqual(hotspot())
	})

	test('changes the hotspot passphrase on or off as it is', () => {
		expect(setHotspotPassphrase(SHARING, 'new-pass-word', FRESH).hotspot).toEqual({ ...hotspot(), passphrase: 'new-pass-word' })
		expect(setHotspotPassphrase(ON_WIFI, 'new-pass-word', FRESH).hotspot.enabled).toBe(false)
		const fresh = setHotspotPassphrase(CABLED, 'new-pass-word', FRESH)
		expect(fresh.hotspot).toEqual({ ...FRESH, passphrase: 'new-pass-word' })
		expect(on(fresh)).toBe('cable')
	})
})

test.describe('what joining proposes', () => {
	test('carries the members WIFI lists, hidden only where offered', () => {
		expect(JOINING).toEqual({
			kind: 'wireless',
			label: 'Clinic-Guest',
			enabled: true,
			verify: true,
			ssid: 'Clinic-Guest',
			security: { kind: 'psk', passphrase: 'guest-pass-1' },
			hidden: false,
		})
		expect(wifiCandidate(PI, { ssid: 'Clinic-Private', kind: 'psk-sae', passphrase: 'private-pass', named: true }).hidden).toBe(true)
		expect(wifiCandidate(WIRED_ONLY, { ssid: 'x', kind: 'psk', passphrase: 'p', named: true })).not.toHaveProperty('hidden')
		expect(validate(joinWifi(CABLED, JOINING))).toBeNull()
	})

	test('names the security by what a scan heard, and by what the device offers', () => {
		const heard = networksHeard(
			[
				ap({ ssid: 'Both', security: ['psk', 'sae'], signal: -50 }),
				ap({ ssid: 'Three', security: ['sae'], signal: -60 }),
				ap({ ssid: 'Two', security: ['psk'], signal: -70 }),
				ap({ ssid: 'Corp', security: ['enterprise'], signal: -40 }),
				ap({ ssid: 'Cafe', security: ['open'], signal: -45 }),
				ap({ ssid: 'Old', security: ['wep'], signal: -80 }),
			],
			PI,
		)
		expect(Object.fromEntries(heard.map((each) => [each.ssid, each.security]))).toEqual({
			Both: 'psk-sae',
			Three: 'sae',
			Two: 'psk',
			Corp: 'enterprise',
			Cafe: null,
			Old: null,
		})
		expect(namedSecurity(PI)).toBe('psk-sae')
		expect(namedSecurity({ document: { attachments: { kind: { wireless: { security: { kind: { psk: {}, sae: {} } } } } } } })).toBe('psk')
		expect(namedSecurity(WIRED_ONLY)).toBeNull()
	})

	test('hears each network once, by its strongest access point, strongest first', () => {
		const heard = networksHeard(
			[
				ap({ ssid: 'Clinic-Staff', signal: -80, bssid: 'a' }),
				ap({ ssid: 'Clinic-Guest', signal: -60 }),
				ap({ ssid: 'Clinic-Staff', signal: -50, bssid: 'b' }),
				ap({ ssid: null, hidden: true, signal: -30 }),
			],
			TWO_RADIOS,
		)
		expect(heard.map((each) => [each.ssid, each.signal])).toEqual([
			['Clinic-Staff', -50],
			['Clinic-Guest', -60],
		])
	})

	test('words a signal', () => {
		expect([-40, -60, -72, -85].map((dbm) => signalOf(dbm).word)).toEqual(['Strong', 'Good', 'Fair', 'Weak'])
	})

	test('sorts a failure into what to do next', () => {
		expect(joinFailure({ at: "$['attachments'][2]['security']['passphrase']", reason: 'no' }, 2)).toBe('passphrase')
		expect(joinFailure({ at: "$['attachments'][2]", reason: 'gone', reached: 'carrier' }, 2)).toBe('range')
		expect(joinFailure({ at: "$['attachments'][2]", reason: 'no lease', reached: 'addressing' }, 2)).toBe('other')
		expect(joinFailure({ at: "$['attachments'][0]", reason: 'gone', reached: 'carrier' }, 2)).toBe('other')
		expect(joinFailure(null, 2)).toBeNull()
	})
})

test.describe('the hotspot not yet set up', () => {
	test('is off, sharing and keeping clients apart where offered', () => {
		expect(freshHotspot(PI, 'clinic-iti-07', 'pass-word-1')).toEqual({
			enabled: false,
			ssid: 'clinic-iti-07',
			passphrase: 'pass-word-1',
			'share-upstream': true,
			'isolate-clients': true,
		})
	})

	test('is named within an SSID', () => {
		expect(ssidOf('clinic-iti-07')).toBe('clinic-iti-07')
		expect(ssidOf('a'.repeat(40))).toBe('a'.repeat(32))
		expect(new TextEncoder().encode(ssidOf('é'.repeat(20))).length).toBe(32)
	})
})

test.describe('the ways of the Network screen', () => {
	const passed = { status: { is: 'passed' } }
	const fact = (name, value, iface, kind = 'text') => ({ fact: true, name, kind, value, traits: { ...passed, interface: { name: iface } } })
	const address = (iface, value) => fact('network-address', value, iface, 'ipv4')
	const carrier = (iface, plugged) => fact('wired-carrier', plugged, iface, 'boolean')
	const joined = (iface, network) => fact('wireless-network', network, iface)
	const clients = (count) => ({ fact: false, name: 'hotspot-clients', kind: 'quantity', value: count, traits: passed })
	const says = (document, ...entries) => {
		const ways = waysOf(document, entries)
		return Object.fromEntries(Object.entries(ways).map(([way, { inUse, says }]) => [way, [inUse, says.join(' · ')]]))
	}
	const route = (fact) => ({ ...fact, traits: { ...fact.traits, interface: { ...fact.traits.interface, route: 'default' } } })

	test('marks the cable in use, and the hotspot beside it with its devices', () => {
		expect(says(SHARING, carrier('eth0', true), address('eth0', '10.4.2.20'), clients(3))).toEqual({
			cable: [true, 'Connected'],
			wifi: [false, 'Off'],
			hotspot: [true, 'On · 3 devices'],
		})
	})

	test('marks only the uplink in use where the cable and wifi are both on, saying the other is on', () => {
		const cable = [carrier('eth0', true), address('eth0', '10.4.2.20')]
		const wifi = [joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4')]
		expect(says(BOTH, route(cable[1]), ...cable.slice(0, 1), ...wifi)).toEqual({
			cable: [true, 'Connected'],
			wifi: [false, 'On · Clinic-Staff'],
			hotspot: [false, 'Off'],
		})
		expect(says(BOTH, ...cable, wifi[0], route(wifi[1]))).toEqual({
			cable: [false, 'On'],
			wifi: [true, 'Connected · Clinic-Staff'],
			hotspot: [false, 'Off'],
		})
	})

	test('says there is no cable while wifi is in use and none is plugged in', () => {
		expect(says(BOTH, joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4')).cable).toEqual([false, 'No cable'])
		expect(says(ON_WIFI, carrier('eth0', false), joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4')).cable).toEqual([false, 'No cable'])
	})

	test('says nothing of a cable plugged in while wifi is in use', () => {
		expect(says(ON_WIFI, carrier('eth0', true), joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4'))).toEqual({
			cable: [false, 'Off'],
			wifi: [true, 'Connected · Clinic-Staff'],
			hotspot: [false, 'Off'],
		})
	})

	test('says how each link stands where neither is in use', () => {
		expect(says(CABLED, carrier('eth0', true)).cable).toEqual([false, 'Plugged in, no connection'])
		expect(says(ON_WIFI, carrier('eth0', true)).cable).toEqual([false, 'Plugged in'])
		expect(says(CABLED, carrier('eth0', false)).cable).toEqual([false, 'No cable'])
		expect(says(ON_WIFI).wifi).toEqual([false, 'Not connected'])
		expect(says(SHARING).hotspot).toEqual([true, 'On'])
		expect(says(SHARING, clients(0)).hotspot).toEqual([true, 'On · No devices'])
	})
})
