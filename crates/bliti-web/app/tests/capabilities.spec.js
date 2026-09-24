// The pre-proposal check of NSCR and the paths it names, as plain functions: no page, because none of
// it touches one.

import { readFileSync } from 'node:fs'

import { expect, test } from '@playwright/test'

import {
	absent,
	adapterName,
	adapters,
	acts,
	bands,
	channels,
	check,
	eapMethods,
	joinWith,
	offersMember,
	radioBands,
	radios,
	scanners,
	securityKinds,
	surveyors,
	widths,
	wpsMethods,
	wpsMethodsFor,
	wpsRadios,
} from '../src/capabilities.js'
import { stagesOf, validate } from '../src/network.js'
import { pathOf, segmentsOf } from '../src/path.js'
import { initSync } from '../src/wasm/bliti_web.js'

// The pre-proposal check is the core's own, through wasm, so the module is loaded as the page loads it.
initSync({ module: readFileSync(new URL('../src/wasm/bliti_web_bg.wasm', import.meta.url)) })

const SECURITY = { kind: { psk: {}, sae: {}, 'psk-sae': {}, enterprise: { eap: ['peap', 'ttls', 'tls'] } } }
const WIRED = {
	'wired-dynamic': { interface: ['eth0'], nameservers: true },
	'wired-static': { interface: ['eth0'], nameservers: true },
}

const BAND = {
	band: {
		'2ghz': { channel: [1, 6, 11], 'channel-width': [20] },
		'5ghz': { channel: [36, 40, 44, 48], 'channel-width': [20, 40, 80] },
	},
}

// A Raspberry Pi 5 as NET shapes it: one shared-channel radio, one wall port.
const PI = {
	document: {
		attachments: {
			kind: {
				wireless: { interface: { wlan0: { security: SECURITY, hidden: true } }, nameservers: true },
				...WIRED,
			},
		},
		hotspot: { interface: { wlan0: BAND }, 'share-upstream': true, 'isolate-clients': true, 'dhcp-range': true },
		'regulatory-domain': true,
	},
	radios: { wlan0: { model: 'Cypress CYW43455', bands: ['2ghz', '5ghz'], alongside: 'shared-channel' } },
	acts: {
		scan: { interface: { wlan0: {} } },
		wps: { interface: { wlan0: { method: ['push-button', 'pin'] } } },
	},
}

// One radio running a hotspot on a channel of its own.
const INDEPENDENT = {
	document: {
		attachments: PI.document.attachments,
		hotspot: { interface: { wlan0: BAND } },
	},
	radios: { wlan0: { model: 'MediaTek MT7921AU', bands: ['2ghz', '5ghz'], alongside: 'independent' } },
	acts: {},
}

const ONE_AT_A_TIME = { ...PI, radios: { wlan0: { ...PI.radios.wlan0, alongside: 'one-at-a-time' } } }

// The Pi with a USB adapter: the adapter joins enterprise networks and gives the hotspot a channel.
const TWO_RADIOS = {
	document: {
		attachments: {
			kind: {
				wireless: {
					interface: {
						wlan0: { security: { kind: { psk: {}, sae: {}, 'psk-sae': {} } } },
						wlx00c0caa1b2c3: { security: { kind: { psk: {}, sae: {}, 'psk-sae': {}, enterprise: { eap: ['peap', 'tls'] } } }, hidden: true },
					},
					nameservers: true,
				},
				...WIRED,
			},
		},
		hotspot: { interface: { wlan0: BAND, wlx00c0caa1b2c3: BAND }, 'share-upstream': true },
	},
	radios: {
		wlan0: { model: 'Cypress CYW43455', bands: ['2ghz', '5ghz'], alongside: 'shared-channel' },
		wlx00c0caa1b2c3: { model: 'MediaTek MT7921AU', bands: ['2ghz', '5ghz', '6ghz'], alongside: 'independent' },
	},
	acts: {
		scan: { interface: { wlan0: {}, wlx00c0caa1b2c3: {} } },
		survey: { interface: { wlx00c0caa1b2c3: {} } },
		wps: { interface: { wlan0: { method: ['push-button', 'pin'] }, wlx00c0caa1b2c3: { method: ['push-button'] } } },
	},
}

const wireless = {
	kind: 'wireless',
	label: 'Clinic',
	verify: true,
	ssid: 'Clinic',
	security: { kind: 'psk-sae', passphrase: 'correct horse battery' },
	hidden: false,
	nameservers: ['1.1.1.1'],
}
const staticPort = { kind: 'wired-static', label: 'Office', verify: true, interface: 'eth0', addresses: ['192.168.60.20/24'], gateway: '192.168.60.1' }
const hotspot = { ssid: 'iti-setup', passphrase: 'ripe-anchor-glass-77', 'share-upstream': true }

test.describe('paths', () => {
	test('are RFC 9535 Normalized Paths', () => {
		expect(pathOf(['attachments', 1, 'gateway'])).toBe("$['attachments'][1]['gateway']")
		expect(pathOf(['hotspot', 'share-upstream'])).toBe("$['hotspot']['share-upstream']")
		expect(pathOf([])).toBe('$')
		expect(pathOf(["it's", 'a\\b', 'tab\there', '\u0001', '\b\f\n\r'])).toBe(
			"$['it\\'s']['a\\\\b']['tab\\there']['\\u0001']['\\b\\f\\n\\r']",
		)
	})

	test('read back into the segments they were written from', () => {
		const segments = ['attachments', 1, "it's", 'a\\b', 'tab\there', '\u0001']
		expect(segmentsOf(pathOf(segments))).toEqual(segments)
		expect(segmentsOf('$')).toEqual([])
		expect(segmentsOf('$.attachments')).toBeNull()
	})
})

test.describe('check', () => {
	test('passes a document within the capabilities', () => {
		expect(check({ attachments: [wireless, staticPort], hotspot, 'regulatory-domain': 'VU' }, PI)).toBeNull()
	})

	test('names a kind of candidate the device does not take', () => {
		const wiredOnly = { document: { attachments: { kind: { 'wired-dynamic': {} } } } }
		expect(check({ attachments: [wireless] }, wiredOnly)).toEqual({
			at: "$['attachments'][0]['kind']",
			reason: 'This device does not take a connection of this kind.',
		})
	})

	test('names a value outside the listed ones, in our words', () => {
		expect(check({ attachments: [{ ...staticPort, interface: 'eth1' }] }, PI)).toEqual({
			at: "$['attachments'][0]['interface']",
			reason: 'This device does not support eth1.',
		})
	})

	test('names an optional member the capabilities do not carry', () => {
		const noHidden = structuredClone(PI)
		delete noHidden.document.attachments.kind.wireless.interface.wlan0.hidden
		expect(check({ attachments: [wireless] }, noHidden)?.at).toBe("$['attachments'][0]['hidden']")
	})

	test('takes the required members of an offered kind without their being listed', () => {
		expect(check({ attachments: [staticPort] }, { document: { attachments: { kind: { 'wired-static': {} } } } })).toBeNull()
	})

	test('names a security kind the device cannot join', () => {
		const saeOnly = { document: { attachments: { kind: { wireless: { security: { kind: { sae: {} } } } } } } }
		expect(check({ attachments: [{ kind: 'wireless', label: 'a', verify: true, ssid: 'a', security: { kind: 'psk', passphrase: '12345678' } }] }, saeOnly)).toEqual({
			at: "$['attachments'][0]['security']['kind']",
			reason: 'This device cannot join a network secured this way.',
		})
	})

	test('holds an EAP method to the listed ones, and takes the credentials that method carries', () => {
		const enterprise = (eap) => ({
			kind: 'wireless',
			label: 'Staff',
			verify: true,
			ssid: 'Staff',
			security: { kind: 'enterprise', eap, identity: 'tech', password: 'x', 'ca-certificate': 'PEM', domain: 'radius.example' },
		})
		expect(check({ attachments: [enterprise('peap')] }, PI)).toBeNull()
		expect(check({ attachments: [enterprise('fast')] }, PI)?.at).toBe("$['attachments'][0]['security']['eap']")
	})

	test('holds a candidate pinned to an adapter to what that adapter offers', () => {
		const { hidden: _, ...unhidden } = wireless
		const staff = { ...unhidden, security: { kind: 'enterprise', eap: 'tls', identity: 'tech', 'ca-certificate': 'PEM' } }
		expect(check({ attachments: [{ ...staff, interface: 'wlx00c0caa1b2c3' }] }, TWO_RADIOS)).toBeNull()
		expect(check({ attachments: [{ ...staff, interface: 'wlan0' }] }, TWO_RADIOS)?.at).toBe("$['attachments'][0]['security']['kind']")
		// Unpinned, it passes where any adapter admits it.
		expect(check({ attachments: [staff] }, TWO_RADIOS)).toBeNull()
	})

	test('rules out band, channel and width on a shared-channel radio a wireless network could share', () => {
		expect(check({ attachments: [staticPort], hotspot: { ...hotspot, band: '5ghz', channel: 44 } }, PI)).toBeNull()
		expect(check({ attachments: [wireless], hotspot: { ...hotspot, band: '5ghz' } }, PI)).toEqual({
			at: "$['hotspot']['band']",
			reason: 'The radio runs the hotspot on the same channel as its wireless connection.',
		})
		expect(check({ attachments: [wireless], hotspot: { ...hotspot, channel: 6 } }, PI)?.at).toBe("$['hotspot']['channel']")
		// Pinned to the other adapter, the network leaves the built-in radio's hotspot its choice.
		const { hidden: _, ...network } = wireless
		const pinned = { attachments: [{ ...network, interface: 'wlx00c0caa1b2c3' }], hotspot: { ...hotspot, interface: 'wlan0', channel: 6 } }
		expect(check(pinned, TWO_RADIOS)).toBeNull()
	})

	test('holds a channel to the list for the band it sits on', () => {
		const base = { ssid: 'a', passphrase: '12345678' }
		expect(check({ attachments: [], hotspot: { ...base, band: '5ghz', channel: 40, 'channel-width': 80 } }, INDEPENDENT)).toBeNull()
		expect(check({ attachments: [], hotspot: { ...base, band: '2ghz', channel: 40 } }, INDEPENDENT)?.at).toBe("$['hotspot']['channel']")
		expect(check({ attachments: [], hotspot: { ...base, band: '2ghz', 'channel-width': 40 } }, INDEPENDENT)).toEqual({
			at: "$['hotspot']['channel-width']",
			reason: 'This device does not support 40 MHz here.',
		})
		// With no band set, a channel usable on some band is one the device can pick a band for.
		expect(check({ attachments: [], hotspot: { ...base, channel: 11 } }, INDEPENDENT)).toBeNull()
		expect(check({ attachments: [], hotspot: { ...base, channel: 13 } }, INDEPENDENT)?.at).toBe("$['hotspot']['channel']")
	})

	test('names a setting absent from the capabilities at the top of the document', () => {
		expect(check({ attachments: [], 'regulatory-domain': 'VU' }, { document: { attachments: { kind: {} } } })?.at).toBe("$['regulatory-domain']")
		expect(check({ attachments: [], hotspot }, { document: { attachments: { kind: {} } } })?.at).toBe("$['hotspot']")
	})

	test('rules out a hotspot beside a wireless network on a radio that runs one at a time', () => {
		expect(check({ attachments: [wireless], hotspot }, ONE_AT_A_TIME)).toEqual({
			at: "$['hotspot']",
			reason: 'The radio cannot run a hotspot while joined to a wireless network.',
		})
		expect(check({ attachments: [staticPort], hotspot }, ONE_AT_A_TIME)).toBeNull()
	})

	test('lets a radio that runs one at a time carry the hotspot where another carries the network', () => {
		const both = structuredClone(TWO_RADIOS)
		both.radios.wlan0.alongside = 'one-at-a-time'
		both.radios.wlx00c0caa1b2c3.alongside = 'one-at-a-time'
		const { hidden: _, ...network } = wireless
		expect(check({ attachments: [network], hotspot }, both)).toBeNull()
		expect(check({ attachments: [{ ...network, interface: 'wlan0' }], hotspot: { ...hotspot, interface: 'wlan0' } }, both)?.at).toBe(
			"$['hotspot']['interface']",
		)
	})
})

test.describe('reading capabilities', () => {
	test('radios are listed with their models, and named by model', () => {
		expect(radios(TWO_RADIOS).map((radio) => radio.interface)).toEqual(['wlan0', 'wlx00c0caa1b2c3'])
		expect(adapterName(TWO_RADIOS, 'wlx00c0caa1b2c3')).toBe('MediaTek MT7921AU')
		const twins = { radios: { wlan0: { model: 'X', bands: [] }, wlan1: { model: 'X', bands: [] } } }
		expect(adapterName(twins, 'wlan1')).toBe('X (wlan1)')
		expect(radioBands(TWO_RADIOS)).toEqual(['2ghz', '5ghz', '6ghz'])
	})

	test('an adapter is offered only where there is more than one', () => {
		expect(adapters(PI, 'wireless')).toEqual([])
		expect(adapters(TWO_RADIOS, 'wireless')).toEqual(['wlan0', 'wlx00c0caa1b2c3'])
		expect(adapters(TWO_RADIOS, 'hotspot')).toEqual(['wlan0', 'wlx00c0caa1b2c3'])
	})

	test('what a candidate is offered follows its adapter, and is everything any offers where unset', () => {
		expect(securityKinds(TWO_RADIOS, { interface: 'wlan0' })).toEqual(['psk-sae', 'sae', 'psk'])
		expect(securityKinds(TWO_RADIOS, { interface: 'wlx00c0caa1b2c3' })).toEqual(['psk-sae', 'sae', 'psk', 'enterprise'])
		expect(securityKinds(TWO_RADIOS, {})).toEqual(['psk-sae', 'sae', 'psk', 'enterprise'])
		expect(eapMethods(TWO_RADIOS, { interface: 'wlx00c0caa1b2c3' })).toEqual(['peap', 'tls'])
		expect(offersMember(TWO_RADIOS, 'wireless', 'hidden', { interface: 'wlan0' })).toBe(false)
		expect(offersMember(TWO_RADIOS, 'wireless', 'hidden', {})).toBe(true)
		expect(offersMember(TWO_RADIOS, 'wireless', 'nameservers', { interface: 'wlan0' })).toBe(true)
	})

	test('the hotspot is offered what its adapter and band carry', () => {
		expect(bands(TWO_RADIOS, { interface: 'wlan0' })).toEqual(['2ghz', '5ghz'])
		expect(bands(TWO_RADIOS, { interface: 'wlan0' }, { attachments: [wireless] })).toEqual([])
		expect(bands(TWO_RADIOS, {}, { attachments: [wireless] })).toEqual(['2ghz', '5ghz'])
		expect(bands(TWO_RADIOS, {})).toEqual(['2ghz', '5ghz'])
		expect(channels(TWO_RADIOS, { band: '2ghz' })).toEqual([1, 6, 11])
		expect(channels(TWO_RADIOS, {})).toEqual([1, 6, 11, 36, 40, 44, 48])
		expect(widths(TWO_RADIOS, { interface: 'wlx00c0caa1b2c3', band: '5ghz' })).toEqual([20, 40, 80])
		expect(channels(TWO_RADIOS, { interface: 'wlan0' }, { attachments: [wireless] })).toEqual([])
	})

	test('acts name the radios that do each, and the WPS methods each offers', () => {
		expect(acts(TWO_RADIOS)).toEqual({ scan: true, survey: true, wps: ['push-button', 'pin'] })
		expect(acts(PI).survey).toBe(false)
		expect(scanners(TWO_RADIOS)).toEqual(['wlan0', 'wlx00c0caa1b2c3'])
		expect(surveyors(TWO_RADIOS)).toEqual(['wlx00c0caa1b2c3'])
		expect(wpsRadios(TWO_RADIOS)).toEqual(['wlan0', 'wlx00c0caa1b2c3'])
		expect(wpsMethods(TWO_RADIOS, 'wlx00c0caa1b2c3')).toEqual(['push-button'])
	})

	test('WPS for a named network is offered only where the device takes one, on the radio it may name', () => {
		expect(wpsMethodsFor(PI, 'Clinic')).toEqual([])
		const named = structuredClone(TWO_RADIOS)
		named.acts.wps.interface.wlan0.ssid = true
		expect(wpsMethodsFor(named, 'Clinic')).toEqual(['push-button', 'pin'])
		expect(wpsMethodsFor(named, 'Clinic', 'wlan0')).toEqual(['push-button', 'pin'])
		expect(wpsMethodsFor(named, 'Clinic', 'wlx00c0caa1b2c3')).toEqual([])
		named.acts.wps.interface.wlan0.ssid = ['Office']
		expect(wpsMethodsFor(named, 'Clinic')).toEqual([])
		expect(wpsMethodsFor(named, 'Office')).toEqual(['push-button', 'pin'])
	})
})

test.describe('absent', () => {
	test('says why a setting is not offered', () => {
		expect(absent(PI, 'hotspot.channel', { attachments: [staticPort], hotspot })).toBeNull()
		expect(absent(PI, 'hotspot.channel', { attachments: [wireless], hotspot })?.reason).toBe('shared-channel')
		expect(absent(INDEPENDENT, 'hotspot.channel')).toBeNull()
		expect(absent(INDEPENDENT, 'hotspot.dhcp-range')?.reason).toBe('unreported')
		expect(absent({ document: { attachments: { kind: {} } } }, 'hotspot')?.reason).toBe('no-radio')
		expect(absent(ONE_AT_A_TIME, 'hotspot', { attachments: [wireless] })?.reason).toBe('one-at-a-time')
		expect(absent(ONE_AT_A_TIME, 'hotspot', { attachments: [staticPort] })).toBeNull()
		expect(absent(ONE_AT_A_TIME, 'wireless', { attachments: [], hotspot })?.reason).toBe('one-at-a-time')
		expect(absent(PI, 'survey')?.reason).toBe('unreported')
	})

	test('a shared-channel adapter says why it has no band, by its model', () => {
		const onBuiltIn = { attachments: [wireless], hotspot: { ...hotspot, interface: 'wlan0' } }
		expect(absent(TWO_RADIOS, 'hotspot.band', onBuiltIn)).toEqual({
			reason: 'shared-channel',
			sentence: 'The Cypress CYW43455 runs the hotspot on the same channel as its wireless connection.',
		})
		expect(absent(TWO_RADIOS, 'hotspot.band', { attachments: [wireless], hotspot })).toBeNull()
		expect(absent(TWO_RADIOS, 'hotspot.band', { attachments: [], hotspot: { ...hotspot, interface: 'wlan0' } })).toBeNull()
	})
})

test.describe('joining a scanned network', () => {
	test('picks the strongest security both ends offer, and none for one the device will not join', () => {
		expect(joinWith(PI, ['psk', 'sae'])).toBe('psk-sae')
		expect(joinWith(PI, ['psk'])).toBe('psk')
		expect(joinWith(PI, ['open'])).toBeNull()
		expect(joinWith({ document: { attachments: { kind: { wireless: { security: { kind: { psk: {} } } } } } } }, ['psk', 'sae'])).toBe('psk')
		expect(joinWith(TWO_RADIOS, ['enterprise'], { interface: 'wlan0' })).toBeNull()
	})
})

test.describe('validate', () => {
	test('a half-typed address is caught before the device sees it', () => {
		expect(validate({ attachments: [{ ...staticPort, gateway: '192.168.60.' }] })?.at).toBe("$['attachments'][0]['gateway']")
		expect(validate({ attachments: [{ ...staticPort, addresses: ['192.168.60.20'] }] })?.at).toBe("$['attachments'][0]['addresses'][0]")
		expect(validate({ attachments: [{ ...staticPort, gateway: 'fe80::1' }] })).toBeNull()
		expect(validate({ attachments: [{ ...wireless, nameservers: ['1.1.1'] }] })?.at).toBe("$['attachments'][0]['nameservers'][0]")
	})

	test('a second DHCP candidate on one interface is caught', () => {
		const dynamic = { kind: 'wired-dynamic', label: 'a', verify: true, interface: 'eth0' }
		expect(validate({ attachments: [dynamic, { ...dynamic, label: 'b' }] })?.at).toBe("$['attachments'][1]['interface']")
	})

	test('a hotspot passphrase too short to be one is caught', () => {
		expect(validate({ attachments: [], hotspot: { ...hotspot, passphrase: 'short' } })?.at).toBe("$['hotspot']['passphrase']")
	})
})

test.describe('stages', () => {
	test('are marked up to where the attempt stopped', () => {
		expect(stagesOf('wired-static', 'gateway').map((stage) => stage.mark)).toEqual(['done', 'done', 'failed'])
		expect(stagesOf('wireless', 'carrier').map((stage) => stage.mark)).toEqual(['failed', 'untried', 'untried', 'untried'])
		expect(stagesOf('wireless', null)).toEqual([])
	})
})
