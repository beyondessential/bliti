// What the fake device of WEB says: its facts and readings, the configuration it holds and what it
// can do, and its answers to the acts it is asked for. The same each time, and plainly fake.

const passed = { status: { is: 'passed' } }
const interfaceOf = (name, extra) => ({ interface: { name, ...extra } })

const fact = (name, value, kind = 'text', traits = {}) => ({
	type: 'fact',
	at: 0,
	fact: name,
	kind,
	value,
	traits: { ...passed, ...traits },
})
const reading = (measurement, kind, value, traits = {}, unit) => ({
	type: 'reading',
	at: 0,
	measurement,
	kind,
	value,
	...(unit ? { unit } : {}),
	traits: { ...passed, ...traits },
})

// The device is wired to a wall port and on Tailscale, with Tamanu running, so the Status screen's
// rows all read well.
const HOSTNAME = 'fake-iti'
export const facts = (now) => [
	fact('hostname', HOSTNAME),
	fact('product', 'Fake Tamanu Iti'),
	fact('board', 'Raspberry Pi 5 Model B Rev 1.0', 'text', { short: 'Raspberry Pi 5' }),
	fact('os', 'Debian GNU/Linux 12 (bookworm)', 'text', { short: 'Debian 12' }),
	fact('kernel', '6.6.31'),
	fact('network-address', '10.0.0.20', 'ipv4', interfaceOf('eth0', { route: 'default' })),
	fact('wired-carrier', true, 'boolean', interfaceOf('eth0', { route: 'default' })),
	fact('tailscale-state', 'running'),
	fact('tailscale-name', HOSTNAME),
	fact('tailnet', 'fake.example.ts.net'),
	fact('tailscale-key-expiry', new Date(now + 90 * 86_400_000).toISOString(), 'datetime'),
	fact('tamanu-state', 'running'),
	fact('tamanu-web-version', '0.0.0-fake'),
	fact('tamanu-facility-server-version', '0.0.0-fake'),
	fact('memory-total', 8_589_934_592, 'quantity', {}),
	fact('filesystem-total', 127_000_000_000, 'quantity', { filesystem: { mount: '/', device: '/dev/mmcblk0p2' } }),
	fact('last-boot', new Date(now - 3 * 3_600_000).toISOString(), 'datetime'),
]

export const readings = () => [
	reading('cpu-usage', 'fraction', 0.12),
	reading('memory-usage', 'fraction', 0.34),
	reading('filesystem-usage', 'fraction', 0.41, { filesystem: { mount: '/', device: '/dev/mmcblk0p2' } }),
	reading('temperature', 'quantity', 48.5, { sensor: 'cpu' }, 'celsius'),
	reading('temperature', 'quantity', 31.2, { sensor: 'case' }, 'celsius'),
]

export const DOCUMENT = {
	attachments: [
		{ kind: 'wired-dynamic', label: 'eth0 automatic', enabled: true, verify: true, interface: 'eth0' },
		{ kind: 'wireless', label: 'Fake-Staff', enabled: true, verify: true, ssid: 'Fake-Staff', security: { kind: 'psk', passphrase: 'fake-passphrase' } },
	],
	'regulatory-domain': 'AU',
}

const BAND = {
	band: {
		'2ghz': { channel: [1, 6, 11], 'channel-width': [20] },
		'5ghz': { channel: [36, 40, 44, 48], 'channel-width': [20, 40, 80] },
	},
}

export const CAPABILITIES = {
	document: {
		attachments: {
			kind: {
				wireless: {
					interface: {
						wlan0: { security: { kind: { psk: {}, sae: {}, 'psk-sae': {}, enterprise: { eap: ['peap', 'ttls', 'tls'] } } }, hidden: true },
					},
					nameservers: true,
				},
				'wired-dynamic': { interface: ['eth0'], nameservers: true },
				'wired-static': { interface: ['eth0'], nameservers: true },
			},
		},
		hotspot: { interface: { wlan0: BAND }, 'share-upstream': true, 'isolate-clients': true, 'dhcp-range': true },
		'regulatory-domain': true,
	},
	radios: { wlan0: { model: 'Cypress CYW43455', bands: ['2ghz', '5ghz'], alongside: 'shared-channel' } },
	acts: { scan: { interface: { wlan0: {} } } },
}

/// What each candidate of `document` is doing, as `state` carries it, in the document's order: the
/// first enabled carries the default route.
function statesOf(document) {
	const attachments = document.attachments ?? []
	const routing = attachments.findIndex((each) => each.enabled !== false)
	return attachments.map((each, index) => (each.enabled === false ? { is: 'off' } : index === routing ? { is: 'default-route' } : { is: 'standby' }))
}

export const state = (document) => ({ type: 'state', busy: false, outcome: 'applied', attachments: statesOf(document) })

export const NETWORKS = [
	{ interface: 'wlan0', ssid: 'Fake-Staff', hidden: false, security: ['psk'], band: '5ghz', channel: 36, 'channel-width': 80, signal: -48 },
	{ interface: 'wlan0', ssid: 'Fake-Guest', hidden: false, security: ['psk', 'sae'], band: '2ghz', channel: 6, 'channel-width': 20, signal: -67 },
]

export const CURVES = {
	type: 'curves',
	document: {
		discharging: { points: [[2.571, 0], [2.8, 0.035], [4.2, 1]], 'learnt-from': 4, error: 0.05, duration: 19800 },
		charging: { points: [[3.3, 0], [4.2, 1]], 'learnt-from': 3, error: 0.1, duration: 13200 },
	},
	lasts: { duration: 18600, margin: 720 },
	recharge: { duration: 13200, margin: 1200 },
}

export const NETCHECK = {
	udp: true,
	ipv4: true,
	ipv6: false,
	'nat-varies': false,
	'captive-portal': false,
	relays: [
		{ id: 5, name: 'Sydney', latency: 0.032 },
		{ id: 12, name: 'Auckland', latency: 0.041 },
	],
}

export const POWER_ACTS = ['restart', 'reboot', 'power-off']
export const TAILSCALE_ACTS = ['turn-off', 'log-out', 'netcheck', 'ping']
export const TAMANU_ACTS = ['restart']
