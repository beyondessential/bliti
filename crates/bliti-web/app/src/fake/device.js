// The fake device of WEB, "The fake device": an Iti every build carries, which the page reaches with
// no channel and no Bluetooth, and which answers as a device does.
//
// It stands in at the messages of MSG, the same seam the test harness fakes at, so the handshake, the
// streams and the reading of messages are not exercised by it. What it says is canned: the same
// facts and readings each time, every act accepted, and nothing it reports changes because of one,
// except that it goes away on the acts that take a device away.

import { CLIENT_NAME, CLIENT_VERSION } from '../link.js'
import { tailscaleActSaid } from '../services.js'

/// The text of the fake device's code. It reads to a code with no wasm, so it is remembered and held
/// across a reload as any code is, and ends in what the application shows in place of a payload's last
/// four characters.
export const FAKE_CODE = 'BLITI:FAKE'

export const isFakeCode = (text) => text?.trim().toUpperCase() === FAKE_CODE

/// How long after an act is accepted the fake device goes away, enough for the application to say
/// what is happening.
const GOES = 300

const passed = { status: { is: 'passed' } }
const message = (body) => ({ kind: 'message', message: body })
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
const facts = (now) => [
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

const readings = () => [
	reading('cpu-usage', 'fraction', 0.12),
	reading('memory-usage', 'fraction', 0.34),
	reading('filesystem-usage', 'fraction', 0.41, { filesystem: { mount: '/', device: '/dev/mmcblk0p2' } }),
	reading('temperature', 'quantity', 48.5, { sensor: 'cpu' }, 'celsius'),
	reading('temperature', 'quantity', 31.2, { sensor: 'case' }, 'celsius'),
]

const DOCUMENT = {
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

const CAPABILITIES = {
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

const state = (document) => ({ type: 'state', busy: false, outcome: 'applied', attachments: statesOf(document) })

const NETWORKS = [
	{ interface: 'wlan0', ssid: 'Fake-Staff', hidden: false, security: ['psk'], band: '5ghz', channel: 36, 'channel-width': 80, signal: -48 },
	{ interface: 'wlan0', ssid: 'Fake-Guest', hidden: false, security: ['psk', 'sae'], band: '2ghz', channel: 6, 'channel-width': 20, signal: -67 },
]

const CURVES = {
	type: 'curves',
	document: {
		discharging: { points: [[2.571, 0], [2.8, 0.035], [4.2, 1]], 'learnt-from': 4, error: 0.05, duration: 19800 },
		charging: { points: [[3.3, 0], [4.2, 1]], 'learnt-from': 3, error: 0.1, duration: 13200 },
	},
	lasts: { duration: 18600, margin: 720 },
	recharge: { duration: 13200, margin: 1200 },
}

const NETCHECK = {
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

const POWER_ACTS = ['restart', 'reboot', 'power-off']
const TAILSCALE_ACTS = ['turn-off', 'log-out', 'netcheck', 'ping']
const TAMANU_ACTS = ['restart']

export function createFakeClient() {
	// The channel as opened: the handlers it reports to, and the streams open on it. Null while there
	// is none.
	let channel = null
	// The feed, `open` while the page is not declining it.
	let feed = null
	// The configuration the device holds in this session, which is the canned one again once it ends.
	let held = DOCUMENT

	const say = (handlers, direction, text) => handlers?.onActivity?.(direction, text)
	const later = (run) => setTimeout(run, 0)

	// Everything of the fake device the feed carries: its facts and readings, and the end of the batch
	// that holds them all.
	function pushed(deliver) {
		const now = Date.now()
		for (const body of [...facts(now), ...readings().map((each) => ({ ...each, at: now }))]) {
			deliver(message({ ...body, at: now }))
		}
		deliver(message({ type: 'end-of-batch' }))
	}

	function open(handlers) {
		say(handlers, 'note', 'notifications on, opening the session')
		say(handlers, 'out', `hello  name ${CLIENT_NAME}, version ${CLIENT_VERSION}`)
		channel = { handlers, streams: new Set() }
		const mine = channel
		const live = { open: true }
		feed = live
		// The device says hello once, as the channel opens, and the feed it pushes follows.
		later(() => {
			if (channel !== mine) return
			handlers.onEvent(message({ type: 'hello', name: 'bliti-fake', version: '0.0.0-fake' }))
			pushed((event) => live.open && handlers.onEvent(event))
		})
	}

	// Take the device away, as a restart, a reboot or a power-off does: it says so on the feed, once
	// what accepted the act has been said, and the channel ends a moment later, with every stream on it.
	function goAway(act) {
		const mine = channel
		if (!mine) return
		later(() => channel === mine && mine.handlers.onEvent(message({ type: 'going-away', act, cause: 'manual-control' })))
		setTimeout(() => {
			if (channel !== mine) return
			channel = null
			feed = null
			for (const stream of mine.streams) stream.end()
			mine.handlers.onDisconnected?.()
		}, GOES)
	}

	// A stream the page opens with its first message, which the device answers as `answer` has it for
	// each message the page sends on it. Closing it from either end ends it. Each message is a copy, as
	// one read off the wire is, so nothing the page does to one reaches what the device holds.
	function stream({ onEvent, onClosed, onActivity }, { first, ended, answer }) {
		if (!channel) throw new Error('Not connected to a device.')
		const mine = channel
		const live = { open: true }
		const emit = (...bodies) =>
			later(() => {
				for (const body of bodies) if (live.open) onEvent(message(structuredClone(body)))
			})
		const end = () => {
			if (!live.open) return
			live.open = false
			mine.streams.delete(live)
			onClosed?.(null)
		}
		live.end = end
		mine.streams.add(live)
		const out = (text) => onActivity?.('out', text)
		out(first)
		answer?.(null, emit)
		return {
			emit,
			out,
			send(text, sent) {
				if (!live.open) throw new Error(`The ${ended} has ended.`)
				out(text)
				answer?.(sent, emit)
			},
			close(text) {
				if (!live.open) return
				live.open = false
				mine.streams.delete(live)
				out(text)
			},
		}
	}

	return {
		async readCode(text) {
			if (!isFakeCode(text)) throw new Error("This isn't the fake device's code.")
			return {
				qr: { fake: true },
				fake: true,
				text: FAKE_CODE,
				suffix: 'FAKE',
				svg: '',
				version: 1,
				localName: 'FAKE',
				localNames: ['FAKE'],
			}
		},

		async connect(qr, handlers) {
			say(handlers, 'note', 'the fake device has no chooser')
			handlers.onPicked?.()
			open(handlers)
		},

		async reconnect(qr, handlers) {
			say(handlers, 'note', 'reconnecting to the fake device')
			open(handlers)
		},

		pauseFeed() {
			if (feed) feed.open = false
		},

		async resumeFeed({ onEvent, onActivity }) {
			if (!channel || feed?.open) return
			const live = { open: true }
			feed = live
			onActivity?.('out', 'subscribe  default')
			later(() => pushed((event) => live.open && onEvent(event)))
		},

		async configure(handlers) {
			held = DOCUMENT
			const session = stream(handlers, {
				first: 'configure',
				ended: 'configuration session',
				answer: (sent, emit) => {
					if (!sent) emit({ type: 'configuration', document: held, capabilities: CAPABILITIES }, state(held))
					else if (sent.type === 'configuration') {
						held = sent.document
						emit({ type: 'configuration', document: held }, state(held))
					} else if (sent.type === 'scan') emit({ type: 'networks', 'access-points': NETWORKS })
					else if (sent.type === 'survey') emit({ type: 'spectrum', spectrum: { channels: [] } })
				},
			})
			return {
				propose: (document) => session.send('configuration  document', { type: 'configuration', document: JSON.parse(JSON.stringify(document)) }),
				scan: (iface) => session.send(iface ? `scan  interface ${iface}` : 'scan', { type: 'scan' }),
				survey: (iface) => session.send(iface ? `survey  interface ${iface}` : 'survey', { type: 'survey' }),
				close: () => {
					session.close('end of the configuration session')
					held = DOCUMENT
				},
			}
		},

		async power(handlers) {
			const power = stream(handlers, {
				first: 'power',
				ended: 'power stream',
				answer: (sent, emit) => {
					if (!sent) emit({ type: 'acts', acts: POWER_ACTS })
					else {
						emit({ type: 'accepted' })
						goAway(sent.act)
					}
				},
			})
			return {
				act: (act) => power.send(`act  ${act}`, { act }),
				close: () => power.close('end of the power stream'),
			}
		},

		async curve(handlers) {
			const curve = stream(handlers, {
				first: 'curve',
				ended: 'curve stream',
				answer: (sent, emit) => emit(sent ? { type: 'accepted' } : CURVES),
			})
			return {
				load: () => curve.send('load  document', { type: 'load' }),
				reset: () => curve.send('reset', { type: 'reset' }),
				close: () => curve.close('end of the curve stream'),
			}
		},

		async tailscale(handlers) {
			const tailscale = stream(handlers, {
				first: 'tailscale',
				ended: 'Tailscale stream',
				answer: (sent, emit) => {
					if (!sent) return emit({ type: 'tailscale-acts', acts: TAILSCALE_ACTS })
					const result = sent.act === 'netcheck' ? NETCHECK : sent.act === 'ping' ? { peer: sent.peer ?? 'fake-peer', reachable: true, path: 'direct', latency: 0.012 } : null
					emit({ type: 'tailscale-accepted' }, ...(result ? [{ type: 'tailscale-result', result }] : [{ type: 'tailscale-acts', acts: TAILSCALE_ACTS }]))
				},
			})
			return {
				act: (act, { method, name, peer } = {}) => tailscale.send(tailscaleActSaid(act, { method, name, peer }), { act, peer }),
				close: () => tailscale.close('end of the Tailscale stream'),
			}
		},

		async tamanu(handlers) {
			const tamanu = stream(handlers, {
				first: 'tamanu',
				ended: 'Tamanu stream',
				answer: (sent, emit) => emit(sent ? { type: 'tamanu-accepted' } : { type: 'tamanu-acts', acts: TAMANU_ACTS }, ...(sent ? [{ type: 'tamanu-acts', acts: TAMANU_ACTS }] : [])),
			})
			return {
				act: (act) => tamanu.send(`tamanu-act  ${act}`, { act }),
				close: () => tamanu.close('end of the Tamanu stream'),
			}
		},

		disconnect() {
			const was = channel
			channel = null
			feed = null
			for (const each of was?.streams ?? []) each.open = false
		},
	}
}
