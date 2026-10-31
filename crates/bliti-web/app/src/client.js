// The browser half of the client (BLI-WEB): Web Bluetooth, the camera, and nothing else.
//
// Everything with protocol in it is in the wasm module: reading a QR code, matching an
// advertisement, the handshake, the streams, and reading a message into one of the outcomes of
// BLI-MSG. This file drives the browser APIs and hands their bytes across, and the boundary is the
// same one the prototype drew.
//
// The interface below is also the seam the test harness fakes at: a fake client feeds the interface
// decoded messages with no channel and no Bluetooth in the loop, which is what lets the view and the
// subscription lifecycle be tested without pretending to be a Bluetooth stack.

import { Channel, QrCode, service_uuid } from './wasm/bliti_web.js'
import {
	CLIENT_NAME,
	CLIENT_VERSION,
	NEEDS_CHOOSER,
	NOTHING_PICKED,
	createLink,
	unsupportedReason,
} from './link.js'
import { createFakeClient, isFakeCode } from './fake/device.js'
import { loadProtocol as protocol } from './protocol.js'
import { tailscaleActSaid } from './services.js'

export { CLIENT_NAME, CLIENT_VERSION, NEEDS_CHOOSER, NOTHING_PICKED }

/// The client the application holds: the real one, and the fake device (WEB, "The fake device"),
/// which a code that reads to it is routed to. The rest of the application sees one client.
export function createClient() {
	const real = createRealClient()
	const fake = createFakeClient()
	// Whichever the channel, or the attempt at one, is with.
	let active = real
	const using = (qr) => (qr?.fake ? fake : real)
	return {
		...real,
		readCode: (text) => (isFakeCode(text) ? fake : real).readCode(text),
		connect(qr, handlers) {
			active = using(qr)
			return active.connect(qr, handlers)
		},
		reconnect(qr, handlers) {
			active = using(qr)
			return active.reconnect(qr, handlers)
		},
		pauseFeed: () => active.pauseFeed(),
		resumeFeed: (handlers) => active.resumeFeed(handlers),
		configure: (handlers) => active.configure(handlers),
		power: (handlers) => active.power(handlers),
		curve: (handlers) => active.curve(handlers),
		tailscale: (handlers) => active.tailscale(handlers),
		tamanu: (handlers) => active.tamanu(handlers),
		disconnect: () => active.disconnect(),
	}
}

function createRealClient() {
	const link = createLink()
	// Whatever is currently delivering the feed: the streams the device pushed after connect, or a
	// subscription opened to resume. Null while the feed is declined (the page is hidden).
	let feed = null
	// A resume in flight, tracked so a decline arriving before it resolves still closes the stream it
	// produces. Tracking the resolved handle instead would leave that stream open forever, and the
	// device would go on pushing to a page nobody is looking at (BLI-MSG).
	let resuming = null

	// Open a channel to the device picked, keyed by the QR code, with the feed the device pushes
	// unprompted as the current one.
	async function open(qr, { onEvent, onClosed, onDisconnected, onActivity }) {
		await link.open((write, chunk) => new Channel(qr, write, chunk), {
			onMessage: onEvent,
			onClosed,
			onDisconnected,
			onActivity,
		})
		// The device pushes the default feed unprompted, so it is already the current feed. It is
		// declined by closing it (BLI-MSG); the device keeps sampling across a decline.
		const opened = link.channel
		feed = { close: () => opened.close_feed() }
	}

	return {
		// Why this browser cannot run the client, or null where it can. The real client owns this
		// because the reasons are its own: a secure context and Web Bluetooth are what it needs, and
		// nothing else in the application should have to know that.
		unsupported: unsupportedReason,

		// Both paths a QR code arrives by land here, and the payload is treated identically once read.
		// `localName` is the name for the highest version marker considered, the one shown before the
		// chooser opens.
		async readCode(text) {
			await protocol()
			const qr = new QrCode(text)
			const localNames = qr.local_names
			return { qr, text: qr.text, suffix: qr.suffix, svg: qr.svg, version: qr.version, localName: localNames[0], localNames }
		},

		// Finding the device the QR code belongs to (ADV, "Matching").
		//
		// The browser gives a chooser rather than the advertisements themselves. The names a device may
		// advertise follow from its QR code alone, one for each version marker considered, so the
		// chooser is filtered on those exact names, which leaves the one device the code belongs to.
		// Not on the bliti service as well: the chooser matches the host's record of a device, and a
		// host that once resolved the device's services without bliti's among them goes on reporting
		// those in place of what it hears advertised (WEB). The pick is still checked against the QR
		// code before anything is sent to it.
		async connect(qr, handlers) {
			const say = (direction, text) => handlers.onActivity?.(direction, text)
			await protocol()
			const names = qr.local_names
			say('note', `asking the browser to choose ${names.join(' or ')}`)
			const device = await link.pick({
				filters: names.map((name) => ({ name })),
				optionalServices: [service_uuid()],
			})

			const advertised = device.name ? qr.read_local_name(device.name) : undefined
			if (!advertised) {
				throw new Error('That device is not an Iti.')
			}
			if (!advertised.supported) {
				throw new Error(
					`That Iti speaks version ${advertised.version}, which this app does not read.`,
				)
			}
			if (!advertised.matches) {
				throw new Error('That is a different Iti. Pick the one whose code you read.')
			}

			say('note', `matched the code against ${device.name}`)
			handlers.onPicked?.()
			await open(qr, handlers)
		},

		// Open a channel again to the device picked last, once it has gone away and is coming back
		// (BLI-WEB, "When the device goes away"). The browser keeps the device it was given, so this
		// needs no chooser and no tap. Rejects with NEEDS_CHOOSER where there is no such device to go
		// back to, and with whatever the attempt met otherwise, for the caller to try again.
		async reconnect(qr, handlers) {
			if (!link.reopenable) {
				const chooser = new Error('The device has to be picked again.')
				chooser.name = NEEDS_CHOOSER
				throw chooser
			}
			handlers.onActivity?.('note', `reconnecting to ${link.device.name}`)
			await open(qr, handlers)
		},

		// Decline the feed while the operator is not looking, by closing it. Idempotent, and closes a
		// resume still in flight when it resolves.
		pauseFeed() {
			if (resuming) resuming.cancel()
			if (feed) {
				feed.close()
				feed = null
			}
		},

		// Resume the feed by subscribing to `default`, unless it is already running or being resumed. A
		// subscription is a stream: it begins here and ends when its handle is closed, which is the
		// unsubscribe (BLI-MSG, "Subscribing").
		async resumeFeed({ onEvent, onClosed, onActivity }) {
			if (feed || resuming || !link.channel) return
			let cancelled = false
			const pending = link.channel.subscribe(
				'default',
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			resuming = { cancel: () => (cancelled = true) }
			onActivity?.('out', 'subscribe  default')
			const handle = await pending
			resuming = null

			// The handle is a wasm-bindgen object, so its Rust allocation lives until JS frees it.
			let closed = false
			const wrapped = {
				close: () => {
					if (closed) return
					closed = true
					onActivity?.('out', 'unsubscribe  default')
					handle.close()
					handle.free()
				},
			}
			// The page may have been hidden while the subscribe was in flight; close it if so.
			if (cancelled) wrapped.close()
			else feed = wrapped
		},

		// Open a configuration session (BLI-CFG): one stream carrying the configuration the device
		// holds, how it is faring, and each proposal. Every message the device sends on it reaches
		// onEvent as a feed's messages do; onClosed is called once the stream ends. Closing the
		// returned session ends it, which changes nothing the device holds.
		async configure({ onEvent, onClosed, onActivity }) {
			if (!link.channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('configure')
			const handle = await link.channel.configure(
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			let closed = false
			const sending = (text, send) => {
				if (closed) throw new Error('The configuration session has ended.')
				say(text)
				send()
			}
			return {
				propose: (document) => sending('configuration  document', () => handle.propose(document)),
				scan: (iface) => sending(iface ? `scan  interface ${iface}` : 'scan', () => handle.scan(iface)),
				survey: (iface) => sending(iface ? `survey  interface ${iface}` : 'survey', () => handle.survey(iface)),
				close: () => {
					if (closed) return
					closed = true
					say('end of the configuration session')
					handle.close()
					handle.free()
				},
			}
		},

		// Open a power stream (BLI-CTL): the acts the device can carry out, and its answer to each
		// asked for (`accepted`, or `cannot-act`). Every message the device sends on it reaches onEvent as a feed's messages do;
		// onClosed is called once the stream ends.
		async power({ onEvent, onClosed, onActivity }) {
			if (!link.channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('power')
			const handle = await link.channel.power(
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			let closed = false
			return {
				act: (act) => {
					if (closed) throw new Error('The power stream has ended.')
					say(`act  ${act}`)
					handle.act(act)
				},
				close: () => {
					if (closed) return
					closed = true
					say('end of the power stream')
					handle.close()
					handle.free()
				},
			}
		},

		// Open a curve stream (BLI-CRV): the device's battery curve document and what it gives for a
		// full charge and a full recharge, and its answer to each load and reset asked for. Every
		// message the device sends on it reaches onEvent as a feed's messages do; onClosed is called
		// once the stream ends.
		async curve({ onEvent, onClosed, onActivity }) {
			if (!link.channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('curve')
			const handle = await link.channel.curve(
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			let closed = false
			const sending = (text, send) => {
				if (closed) throw new Error('The curve stream has ended.')
				say(text)
				send()
			}
			return {
				load: (document) => sending('load  document', () => handle.load(document)),
				reset: () => sending('reset', () => handle.reset()),
				close: () => {
					if (closed) return
					closed = true
					say('end of the curve stream')
					handle.close()
					handle.free()
				},
			}
		},

		// Open a Tailscale stream (BLI-TSC): the acts the device can carry out now, its answer to each
		// asked for, and what a sign-in, network check or ping sends back. Every message the device
		// sends on it reaches onEvent as a feed's messages do; onClosed is called once the stream ends.
		// The record of what was sent never carries an auth key.
		async tailscale({ onEvent, onClosed, onActivity }) {
			if (!link.channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('tailscale')
			const handle = await link.channel.tailscale(
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			let closed = false
			return {
				act: (act, { method, key, name, peer } = {}) => {
					if (closed) throw new Error('The Tailscale stream has ended.')
					say(tailscaleActSaid(act, { method, name, peer }))
					handle.act(act, method, key, name, peer)
				},
				close: () => {
					if (closed) return
					closed = true
					say('end of the Tailscale stream')
					handle.close()
					handle.free()
				},
			}
		},

		// Open a Tamanu stream (BLI-TMU): the acts the device can carry out now, and its answer to each
		// asked for. Every message the device sends on it reaches onEvent as a feed's messages do;
		// onClosed is called once the stream ends.
		async tamanu({ onEvent, onClosed, onActivity }) {
			if (!link.channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('tamanu')
			const handle = await link.channel.tamanu(
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			let closed = false
			return {
				act: (act) => {
					if (closed) throw new Error('The Tamanu stream has ended.')
					say(`tamanu-act  ${act}`)
					handle.act(act)
				},
				close: () => {
					if (closed) return
					closed = true
					say('end of the Tamanu stream')
					handle.close()
					handle.free()
				},
			}
		},

		disconnect() {
			link.disconnect()
			feed = null
		},
	}
}
