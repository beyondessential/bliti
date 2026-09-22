// The pre-proposal check of NSCR and the paths it names, as plain functions: no page, because none of
// it touches one.

import { expect, test } from '@playwright/test'

import { absent, check, joinWith } from '../src/capabilities.js'
import { stagesOf, validate } from '../src/network.js'
import { pathOf } from '../src/path.js'

const PI = {
	document: {
		attachments: {
			wireless: {
				security: { psk: true, sae: true, 'psk-sae': true, enterprise: { eap: ['peap', 'ttls', 'tls'] } },
				hidden: true,
				nameservers: true,
			},
			'wired-dynamic': { interface: ['eth0'], nameservers: true },
			'wired-static': { interface: ['eth0'], nameservers: true },
		},
		hotspot: { 'share-upstream': true, 'isolate-clients': true, 'dhcp-range': true },
		'regulatory-domain': true,
	},
	radio: { alongside: 'shared-channel' },
	acts: { scan: true, wps: ['push-button', 'pin'] },
}

const INDEPENDENT = {
	document: {
		attachments: PI.document.attachments,
		hotspot: {
			band: ['2.4ghz', '5ghz'],
			channel: { '2.4ghz': [1, 6, 11], '5ghz': [36, 40, 44, 48] },
			'channel-width': { '2.4ghz': [20], '5ghz': [20, 40, 80] },
		},
	},
	radio: { alongside: 'independent' },
}

const ONE_AT_A_TIME = { ...PI, radio: { alongside: 'one-at-a-time' } }

const wireless = {
	kind: 'wireless',
	label: 'Clinic',
	ssid: 'Clinic',
	security: { kind: 'psk-sae', passphrase: 'correct horse battery' },
	hidden: false,
	nameservers: ['1.1.1.1'],
}
const staticPort = { kind: 'wired-static', label: 'Office', interface: 'eth0', addresses: ['192.168.60.20/24'], gateway: '192.168.60.1' }
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
})

test.describe('check', () => {
	test('passes a document within the capabilities', () => {
		expect(check({ attachments: [wireless, staticPort], hotspot, 'regulatory-domain': 'VU' }, PI)).toBeNull()
	})

	test('names a kind of candidate the device does not take', () => {
		const wiredOnly = { document: { attachments: { 'wired-dynamic': true } } }
		expect(check({ attachments: [wireless] }, wiredOnly)).toEqual({
			at: "$['attachments'][0]['kind']",
			reason: 'This device does not take a connection of this kind.',
		})
	})

	test('names a value outside the listed ones', () => {
		expect(check({ attachments: [{ ...staticPort, interface: 'eth1' }] }, PI)?.at).toBe("$['attachments'][0]['interface']")
	})

	test('names an optional member the capabilities do not carry', () => {
		const noHidden = structuredClone(PI)
		delete noHidden.document.attachments.wireless.hidden
		expect(check({ attachments: [wireless] }, noHidden)?.at).toBe("$['attachments'][0]['hidden']")
	})

	test('takes the required members of an offered kind without their being listed', () => {
		expect(check({ attachments: [staticPort] }, { document: { attachments: { 'wired-static': {} } } })).toBeNull()
	})

	test('names a security kind the device cannot join', () => {
		const saeOnly = { document: { attachments: { wireless: { security: { sae: true } } } } }
		expect(check({ attachments: [{ kind: 'wireless', label: 'a', ssid: 'a', security: { kind: 'psk', passphrase: '12345678' } }] }, saeOnly)?.at).toBe(
			"$['attachments'][0]['security']['kind']",
		)
	})

	test('holds an EAP method to the listed ones, and takes the credentials that method carries', () => {
		const enterprise = (eap) => ({
			kind: 'wireless',
			label: 'Staff',
			ssid: 'Staff',
			security: { kind: 'enterprise', eap, identity: 'tech', password: 'x', 'ca-certificate': 'PEM', domain: 'radius.example' },
		})
		expect(check({ attachments: [enterprise('peap')] }, PI)).toBeNull()
		expect(check({ attachments: [enterprise('fast')] }, PI)?.at).toBe("$['attachments'][0]['security']['eap']")
	})

	test('rules out band, channel and width on a shared-channel radio', () => {
		expect(check({ attachments: [], hotspot: { ...hotspot, band: '5ghz' } }, PI)?.at).toBe("$['hotspot']['band']")
		expect(check({ attachments: [], hotspot: { ...hotspot, channel: 6 } }, PI)?.at).toBe("$['hotspot']['channel']")
	})

	test('holds a channel to the list for the band it sits on', () => {
		const base = { ssid: 'a', passphrase: '12345678' }
		expect(check({ attachments: [], hotspot: { ...base, band: '5ghz', channel: 40, 'channel-width': 80 } }, INDEPENDENT)).toBeNull()
		expect(check({ attachments: [], hotspot: { ...base, band: '2.4ghz', channel: 40 } }, INDEPENDENT)?.at).toBe("$['hotspot']['channel']")
		expect(check({ attachments: [], hotspot: { ...base, band: '2.4ghz', 'channel-width': 40 } }, INDEPENDENT)?.at).toBe(
			"$['hotspot']['channel-width']",
		)
		// With no band set, a channel usable on some band is one the device can pick a band for.
		expect(check({ attachments: [], hotspot: { ...base, channel: 11 } }, INDEPENDENT)).toBeNull()
		expect(check({ attachments: [], hotspot: { ...base, channel: 13 } }, INDEPENDENT)?.at).toBe("$['hotspot']['channel']")
	})

	test('names a setting absent from the capabilities at the top of the document', () => {
		expect(check({ attachments: [], 'regulatory-domain': 'VU' }, { document: { attachments: {} } })?.at).toBe("$['regulatory-domain']")
		expect(check({ attachments: [], hotspot }, { document: { attachments: {} } })?.at).toBe("$['hotspot']")
	})

	test('rules out a hotspot beside a wireless network on a radio that runs one at a time', () => {
		expect(check({ attachments: [wireless], hotspot }, ONE_AT_A_TIME)).toEqual({
			at: "$['hotspot']",
			reason: 'The radio cannot run a hotspot while joined to a wireless network.',
		})
		expect(check({ attachments: [staticPort], hotspot }, ONE_AT_A_TIME)).toBeNull()
	})
})

test.describe('absent', () => {
	test('says why a setting is not offered', () => {
		expect(absent(PI, 'hotspot.channel')?.reason).toBe('shared-channel')
		expect(absent(INDEPENDENT, 'hotspot.channel')).toBeNull()
		expect(absent(INDEPENDENT, 'hotspot.dhcp-range')?.reason).toBe('unreported')
		expect(absent({ document: { attachments: {} } }, 'hotspot')?.reason).toBe('no-radio')
		expect(absent(ONE_AT_A_TIME, 'hotspot', { attachments: [wireless] })?.reason).toBe('one-at-a-time')
		expect(absent(ONE_AT_A_TIME, 'hotspot', { attachments: [staticPort] })).toBeNull()
		expect(absent(PI, 'survey')?.reason).toBe('unreported')
	})
})

test.describe('joining a scanned network', () => {
	test('picks the strongest security both ends offer, and none for one the device will not join', () => {
		expect(joinWith(PI, ['psk', 'sae'])).toBe('psk-sae')
		expect(joinWith(PI, ['psk'])).toBe('psk')
		expect(joinWith(PI, ['open'])).toBeNull()
		expect(joinWith({ document: { attachments: { wireless: { security: { psk: true } } } } }, ['psk', 'sae'])).toBe('psk')
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
		const dynamic = { kind: 'wired-dynamic', label: 'a', interface: 'eth0' }
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
