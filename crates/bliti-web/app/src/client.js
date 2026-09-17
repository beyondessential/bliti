// The browser half of the client (BLI-WEB): Web Bluetooth, the camera, and nothing else.
//
// Everything with protocol in it is in the wasm module: reading a QR code, matching an
// advertisement, the handshake, the streams, and reading a message into one of the outcomes of
// BLI-MSG. This file drives the browser APIs and hands their bytes across, and the boundary is the
// same one the prototype drew.
//
// The interface below is also the seam the test harness fakes at: a fake client feeds the interface
// decoded messages with no wasm and no Bluetooth in the loop, which is what lets the view and the
// subscription lifecycle be tested without pretending to be a Bluetooth stack.

import init, {
	Channel,
	QrCode,
	client_tx_uuid,
	device_tx_uuid,
	service_uuid,
	start,
} from './wasm/bliti_web.js'
import wasmUrl from './wasm/bliti_web_bg.wasm?url'

/// What this client calls itself to a device. Opaque to the device, which logs it (BLI-MSG).
export const CLIENT_NAME = 'bliti-web'
export const CLIENT_VERSION = __APP_VERSION__

let loaded
async function protocol() {
	// The failure is not cached along with the success: a wasm fetch that fails once, on a flaky
	// network before the service worker has cached it, would otherwise leave every later call
	// rethrowing the same stale error with no way back short of a reload.
	loaded ??= init({ module_or_path: wasmUrl })
		.then(() => {
			start()
		})
		.catch((error) => {
			loaded = undefined
			throw error
		})
	await loaded
}

export function createClient() {
	let device = null
	let channel = null

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
		async readCode(text) {
			await protocol()
			const qr = new QrCode(text)
			return { qr, human: qr.human, version: qr.version }
		},

		// Finding the device the QR code belongs to (ADV, "Matching").
		//
		// The browser gives a chooser rather than the advertisements themselves, and the payload it
		// filters on holds a salt that changes, so the chooser cannot be narrowed to one device ahead
		// of time. It is filtered to devices carrying the bliti service, and the one the operator picks
		// is checked against the QR code before anything is sent to it.
		async connect(qr, { onEvent, onClosed, onDisconnected, onActivity }) {
			const say = (direction, text) => onActivity?.(direction, text)
			await protocol()
			say('note', 'asking the browser to choose a device')
			device = await navigator.bluetooth.requestDevice({
				filters: [{ services: [service_uuid()] }],
			})

			const advertised = device.name ? qr.read_local_name(device.name) : undefined
			if (!advertised) {
				throw new Error('That device is not advertising a bliti payload.')
			}
			if (advertised.version !== qr.version) {
				throw new Error(
					`That device speaks bliti version ${advertised.version}, which this app does not read.`,
				)
			}
			if (!advertised.matches) {
				throw new Error('That is a different bliti device. Pick the one whose code you read.')
			}

			say('note', `matched the code against ${device.name}`)
			const server = await device.gatt.connect()
			const service = await server.getPrimaryService(service_uuid())
			const clientTx = await service.getCharacteristic(client_tx_uuid())
			const deviceTx = await service.getCharacteristic(device_tx_uuid())

			// Writes are acknowledged, so the device is never sent more than it has taken. The bytes are
			// copied because the channel hands over a view it may reuse.
			channel = new Channel(qr, (bytes) => clientTx.writeValueWithResponse(bytes.slice()))

			deviceTx.addEventListener('characteristicvaluechanged', (event) => {
				channel.receive(new Uint8Array(event.target.value.buffer))
			})
			// The channel closes once, whether the link drops or the connection ends on a fault; both
			// reach the operator through onDisconnected, and the guard keeps a doubled signal (a fault
			// that also drops the link) from reporting twice.
			let closed = false
			const closeOnce = (why) => {
				if (closed) return
				closed = true
				onDisconnected?.(why)
			}
			device.addEventListener('gattserverdisconnected', () => closeOnce())

			// Subscribing to notifications is what opens a session: it is the point at which the device
			// can send. Not to be confused with a bliti subscription, which is a stream.
			await deviceTx.startNotifications()
			say('note', 'notifications on, opening the session')

			// Said before the call rather than after: the handshake and the device's first messages all
			// happen inside it, so anything logged afterwards would land out of order behind them.
			say('out', `client-hello  name ${CLIENT_NAME}, version ${CLIENT_VERSION}`)
			await channel.connect(
				CLIENT_NAME,
				CLIENT_VERSION,
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
				(why) => closeOnce(why),
			)
		},

		// A subscription is a stream: it begins here and ends when the handle is closed, which is the
		// unsubscribe (BLI-MSG, "Subscribing").
		async subscribe(topic, { onEvent, onClosed, onActivity }) {
			onActivity?.('out', `subscribe  ${topic}`)
			const handle = await channel.subscribe(
				topic,
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			// The handle is a wasm-bindgen object, so its Rust allocation lives until JS frees it.
			// Closing and freeing are paired here so the caller cannot leak one per visibility toggle,
			// and the guard makes closing twice harmless rather than a use-after-free.
			let closed = false
			return {
				close: async () => {
					if (closed) return
					closed = true
					onActivity?.('out', `unsubscribe  ${topic}`)
					handle.close()
					handle.free()
				},
			}
		},

		disconnect() {
			if (device?.gatt?.connected) device.gatt.disconnect()
			device = null
			channel = null
		},
	}
}
