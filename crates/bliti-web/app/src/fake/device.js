// The fake device of WEB, "The fake device": an Iti every build carries, which the page reaches with
// no channel and no Bluetooth, and which answers as a device does.
//
// It stands in at the messages of MSG, the same seam the test harness fakes at, so the handshake, the
// streams and the reading of messages are not exercised by it. What it says is canned: the same
// facts and readings each time, every act accepted, and nothing it reports changes because of one,
// except that it goes away on the acts that take a device away.

import { CLIENT_NAME, CLIENT_VERSION } from '../link.js'
import { tailscaleActSaid } from '../services.js'
import { FAKE_SUFFIX } from './name.js'
import {
	CAPABILITIES,
	CURVES,
	DOCUMENT,
	NETCHECK,
	NETWORKS,
	POWER_ACTS,
	TAILSCALE_ACTS,
	TAMANU_ACTS,
	facts,
	readings,
	state,
} from './canned.js'

/// The text of the fake device's code. It reads to a code with no wasm, so it is remembered and held
/// across a reload as any code is, and ends in what the application shows in place of a payload's last
/// four characters.
export const FAKE_CODE = `BLITI:${FAKE_SUFFIX}`

export const isFakeCode = (text) => text?.trim().toUpperCase() === FAKE_CODE

/// How long after an act is accepted the fake device goes away, enough for the application to say
/// what is happening.
const GOES = 300

const message = (body) => ({ kind: 'message', message: body })

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
		for (const body of [...facts(now), ...readings()]) deliver(message({ ...body, at: now }))
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
			if (feed) feed.open = false
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
				qr: { text: FAKE_CODE },
				fake: true,
				text: FAKE_CODE,
				suffix: FAKE_SUFFIX,
				svg: '',
				version: 1,
				localName: FAKE_SUFFIX,
				localNames: [FAKE_SUFFIX],
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
			if (feed) feed.open = false
			feed = null
			for (const each of was?.streams ?? []) each.open = false
		},
	}
}
