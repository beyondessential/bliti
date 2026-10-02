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

import { Channel, QrCode, allocation_uuid, client_tx_uuid, service_uuid, slot_uuid } from './wasm/bliti_web.js'
import { loadProtocol as protocol } from './protocol.js'

/// What this client calls itself to a device. Opaque to the device, which logs it (BLI-MSG).
export const CLIENT_NAME = 'bliti-web'
export const CLIENT_VERSION = __APP_VERSION__

/// The name of the error connect throws where the chooser closed with nothing picked. The browser
/// does not say whether the list was empty or the operator dismissed it (WEB).
export const NOTHING_PICKED = 'NothingPicked'

/// The name of the error reconnect throws where the device can only be reached again through the
/// chooser (WEB).
export const NEEDS_CHOOSER = 'NeedsChooser'

export function createClient() {
	let device = null
	let channel = null
	// Whatever is currently delivering the feed: the streams the device pushed after connect, or a
	// subscription opened to resume. Null while the feed is declined (the page is hidden).
	let feed = null
	// A resume in flight, tracked so a decline arriving before it resolves still closes the stream it
	// produces. Tracking the resolved handle instead would leave that stream open forever, and the
	// device would go on pushing to a page nobody is looking at (BLI-MSG).
	let resuming = null

	// The listeners the open channel put on the device, so a later open can take them off again: a
	// link dropped under an old channel must not reach the operator as the new one closing.
	let detach = () => {}
	// Bumped on disconnect, so an open still in flight when the operator lets the device go does not
	// leave a channel behind it.
	let generation = 0

	// Open a channel to the device picked: the GATT connection, the characteristics, the handshake,
	// and the streams above it.
	async function open(qr, { onEvent, onClosed, onDisconnected, onActivity }) {
		const say = (direction, text) => onActivity?.(direction, text)
		const mine = ++generation
		const current = () => {
			if (mine === generation) return
			if (device?.gatt?.connected) device.gatt.disconnect()
			throw new Error('The device was let go of while connecting.')
		}
		detach()
		detach = () => {}
		if (device.gatt.connected) device.gatt.disconnect()

		const server = await device.gatt.connect()
		current()
		const service = await server.getPrimaryService(service_uuid())
		const clientTx = await service.getCharacteristic(client_tx_uuid())
		// The device notifies each client on a characteristic of its own, and says which (CHN).
		const allocation = await service.getCharacteristic(allocation_uuid())
		const slot = slot_uuid(new Uint8Array((await allocation.readValue()).buffer))
		current()
		if (!slot) {
			if (device?.gatt?.connected) device.gatt.disconnect()
			throw new Error('The device is serving as many clients as it can. Try again later.')
		}
		const deviceTx = await service.getCharacteristic(slot)
		current()

		// Writes are acknowledged, so the device is never sent more than it has taken. The bytes are
		// copied because the channel hands over a view it may reuse.
		const opened = new Channel(qr, (bytes) => clientTx.writeValueWithResponse(bytes.slice()))
		channel = opened

		const receive = (event) => opened.receive(new Uint8Array(event.target.value.buffer))
		deviceTx.addEventListener('characteristicvaluechanged', receive)
		// The channel closes once, whether the link drops or the connection ends on a fault; both
		// reach the operator through onDisconnected, and the guard keeps a doubled signal (a fault
		// that also drops the link) from reporting twice.
		let closed = false
		const closeOnce = (why) => {
			if (closed) return
			closed = true
			detach()
			detach = () => {}
			onDisconnected?.(why)
		}
		const dropped = () => closeOnce()
		const held = device
		held.addEventListener('gattserverdisconnected', dropped)
		detach = () => {
			closed = true
			deviceTx.removeEventListener('characteristicvaluechanged', receive)
			held.removeEventListener('gattserverdisconnected', dropped)
		}

		// Subscribing to notifications is what opens a session: it is the point at which the device
		// can send. Not to be confused with a bliti subscription, which is a stream.
		await deviceTx.startNotifications()
		current()
		say('note', 'notifications on, opening the session')

		// Said before the call rather than after: the handshake and the device's first messages all
		// happen inside it, so anything logged afterwards would land out of order behind them.
		say('out', `hello  name ${CLIENT_NAME}, version ${CLIENT_VERSION}`)
		await opened.connect(
			CLIENT_NAME,
			CLIENT_VERSION,
			(json) => onEvent(JSON.parse(json)),
			(why) => onClosed?.(why),
			(why) => closeOnce(why),
		)
		current()
		// The device pushes the default feed unprompted, so it is already the current feed. It is
		// declined by closing it (BLI-MSG); the device keeps sampling across a decline.
		feed = { close: () => opened.close_feed() }
	}

	return {
		// Why this browser cannot run the client, or null where it can. The real client owns this
		// because the reasons are its own: a secure context and Web Bluetooth are what it needs, and
		// nothing else in the application should have to know that.
		unsupported() {
			if (!window.isSecureContext) {
				return 'This page needs a secure context. Open it over https, or over localhost while developing.'
			}
			if (!navigator.bluetooth) {
				return 'This browser does not offer Web Bluetooth. Chrome on Android is the tested one.'
			}
			return null
		},

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
			try {
				device = await navigator.bluetooth.requestDevice({
					filters: names.map((name) => ({ name })),
					optionalServices: [service_uuid()],
				})
			} catch (error) {
				// Told apart here because a GATT lookup below rejects with the same name.
				if (error.name === 'NotFoundError') {
					const nothing = new Error('No device was picked.')
					nothing.name = NOTHING_PICKED
					throw nothing
				}
				throw error
			}

			const advertised = device.name ? qr.read_local_name(device.name) : undefined
			if (!advertised) {
				throw new Error('That device is not advertising a bliti payload.')
			}
			if (!advertised.supported) {
				throw new Error(
					`That device speaks bliti version ${advertised.version}, which this app does not read.`,
				)
			}
			if (!advertised.matches) {
				throw new Error('That is a different bliti device. Pick the one whose code you read.')
			}

			say('note', `matched the code against ${device.name}`)
			await open(qr, handlers)
		},

		// Open a channel again to the device picked last, once it has gone away and is coming back
		// (BLI-WEB, "When the device goes away"). The browser keeps the device it was given, so this
		// needs no chooser and no tap. Rejects with NEEDS_CHOOSER where there is no such device to go
		// back to, and with whatever the attempt met otherwise, for the caller to try again.
		async reconnect(qr, handlers) {
			if (!device?.gatt) {
				const chooser = new Error('The device has to be picked again.')
				chooser.name = NEEDS_CHOOSER
				throw chooser
			}
			handlers.onActivity?.('note', `reconnecting to ${device.name}`)
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
			if (feed || resuming || !channel) return
			let cancelled = false
			const pending = channel.subscribe(
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

		// Open a configuration session (BLI-CFG): one stream carrying the configuration in force, each
		// proposal, what became of it, and the confirmation. Every message the device sends on it
		// reaches onEvent as a feed's messages do; onClosed is called once the stream ends. Closing the
		// returned session ends it, which the device reads as abandoning whatever was not confirmed.
		async configure({ onEvent, onClosed, onActivity }) {
			if (!channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('configure')
			const handle = await channel.configure(
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
				confirm: () => sending('confirm', () => handle.confirm()),
				discard: () => sending('discard', () => handle.discard()),
				scan: (iface) => sending(iface ? `scan  interface ${iface}` : 'scan', () => handle.scan(iface)),
				survey: (iface) => sending(iface ? `survey  interface ${iface}` : 'survey', () => handle.survey(iface)),
				wps: (method, iface, ssid) =>
					sending(`wps  method ${method}${iface ? `  interface ${iface}` : ''}${ssid ? `  ssid ${ssid}` : ''}`, () =>
						handle.wps(method, iface, ssid),
					),
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
		// asked for. Every message the device sends on it reaches onEvent as a feed's messages do;
		// onClosed is called once the stream ends.
		async power({ onEvent, onClosed, onActivity }) {
			if (!channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('power')
			const handle = await channel.power(
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
			if (!channel) throw new Error('Not connected to a device.')
			const say = (text) => onActivity?.('out', text)
			say('curve')
			const handle = await channel.curve(
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

		disconnect() {
			generation++
			detach()
			detach = () => {}
			if (device?.gatt?.connected) device.gatt.disconnect()
			device = null
			channel = null
			feed = null
		},
	}
}
