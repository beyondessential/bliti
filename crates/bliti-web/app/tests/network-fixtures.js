// What the network screen's tests share: devices as NET shapes their capabilities, a configuration in
// force, and the ways a test finds its way around the screen.

import { sent } from './fake-client.js'

export const SECURITY = { kind: { psk: {}, sae: {}, 'psk-sae': {}, enterprise: { eap: ['peap', 'ttls', 'tls'] } } }
export const WIRED = {
	'wired-dynamic': { interface: ['eth0'], nameservers: true },
	'wired-static': { interface: ['eth0'], nameservers: true },
}
export const BUILT_IN = { model: 'Cypress CYW43455', bands: ['2.4ghz', '5ghz'], alongside: 'shared-channel' }
export const USB = { model: 'MediaTek MT7921AU', bands: ['2.4ghz', '5ghz', '6ghz'], alongside: 'independent' }
export const BAND = {
	band: {
		'2.4ghz': { channel: [1, 6, 11], 'channel-width': [20] },
		'5ghz': { channel: [36, 40, 44, 48], 'channel-width': [20, 40, 80] },
	},
}

// A Raspberry Pi 5 as NET shapes it: a shared-channel radio, one wall port.
export const PI = {
	document: {
		attachments: {
			kind: {
				wireless: { interface: { wlan0: { security: SECURITY, hidden: true } }, nameservers: true },
				...WIRED,
			},
		},
		hotspot: { interface: { wlan0: {} }, 'share-upstream': true, 'isolate-clients': true, 'dhcp-range': true },
		'regulatory-domain': true,
	},
	radios: { wlan0: BUILT_IN },
	acts: {
		scan: { interface: { wlan0: {} } },
		wps: { interface: { wlan0: { method: ['push-button', 'pin'], ssid: true } } },
	},
}

// One radio running the hotspot on channels of its own.
export const INDEPENDENT = {
	document: {
		...PI.document,
		hotspot: { interface: { wlan0: BAND }, 'share-upstream': true, 'isolate-clients': true },
	},
	radios: { wlan0: { ...BUILT_IN, alongside: 'independent' } },
	acts: {
		scan: { interface: { wlan0: {} } },
		survey: { interface: { wlan0: {} } },
		wps: { interface: { wlan0: { method: ['push-button'] } } },
	},
}

// The Pi with a USB adapter: only the adapter joins enterprise networks, hides, or gives the hotspot
// a channel of its own.
export const TWO_RADIOS = {
	document: {
		attachments: {
			kind: {
				wireless: {
					interface: {
						wlan0: { security: { kind: { psk: {}, sae: {}, 'psk-sae': {} } } },
						wlx00c0caa1b2c3: { security: SECURITY, hidden: true },
					},
					nameservers: true,
				},
				...WIRED,
			},
		},
		hotspot: { interface: { wlan0: {}, wlx00c0caa1b2c3: BAND }, 'share-upstream': true, 'isolate-clients': true },
		'regulatory-domain': true,
	},
	radios: { wlan0: BUILT_IN, wlx00c0caa1b2c3: USB },
	acts: {
		scan: { interface: { wlan0: {}, wlx00c0caa1b2c3: {} } },
		survey: { interface: { wlx00c0caa1b2c3: {} } },
		wps: { interface: { wlan0: { method: ['push-button', 'pin'], ssid: true }, wlx00c0caa1b2c3: { method: ['push-button'] } } },
	},
}

export const WIRED_ONLY = {
	document: {
		attachments: {
			kind: {
				'wired-dynamic': { interface: ['eth0', 'eth1'], nameservers: true },
				'wired-static': { interface: ['eth0', 'eth1'], nameservers: true },
			},
		},
	},
	acts: {},
}

export const IN_FORCE = {
	attachments: [
		{ kind: 'wired-static', label: 'Clinic wall port', verify: true, interface: 'eth0', addresses: ['10.4.2.20/24'], gateway: '10.4.2.1' },
		{ kind: 'wired-static', label: 'North site', verify: true, interface: 'eth0', addresses: ['192.168.60.20/24'], gateway: '192.168.60.1', nameservers: ['192.168.60.1'] },
		{ kind: 'wired-dynamic', label: 'eth0 automatic', verify: true, interface: 'eth0' },
		{ kind: 'wireless', label: 'Clinic-Staff', verify: true, ssid: 'Clinic-Staff', security: { kind: 'sae', passphrase: 'correct horse battery' } },
		{ kind: 'wireless', label: 'BackupLink', verify: true, ssid: 'BackupLink', security: { kind: 'psk', passphrase: 'backup-link-77' } },
	],
	hotspot: { ssid: 'Clinic-Field-04', passphrase: 'ripe-anchor-glass-77', 'share-upstream': true, 'isolate-clients': true },
	'regulatory-domain': 'VU',
}

export const STATES = [
	{ is: 'default-route' },
	{ is: 'unavailable', reached: 'gateway', reason: '192.168.60.1 did not answer' },
	{ is: 'unavailable', reached: 'addressing', reason: 'no DHCP offer on eth0' },
	{ is: 'up' },
	{ is: 'unavailable', reached: 'carrier', reason: 'BackupLink is not in range' },
]

// One access point heard, as `networks` carries it.
export const ap = (fields) => ({
	interface: 'wlan0',
	hidden: false,
	security: ['psk', 'sae'],
	band: '5ghz',
	channel: 36,
	'channel-width': 80,
	...fields,
})

export const bar = (page) => page.locator('.bar-state')
export const addWireless = async (page) => {
	await page.getByRole('button', { name: 'Add' }).click()
	await page.getByRole('button', { name: 'Wireless', exact: true }).click()
}
export const row = (page, name) => page.locator('.order li').filter({ hasText: name })
export const open = (page, name) => row(page, name).locator('.what').click()
export const proposals = async (page) => (await sent(page)).filter((each) => each.type === 'configuration')

