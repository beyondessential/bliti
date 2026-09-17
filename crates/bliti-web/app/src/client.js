// The browser half of the client (BLI-WEB): Web Bluetooth, the camera, and nothing else.
//
// Everything with protocol in it is in the wasm module: reading a sticker, matching an
// advertisement, the handshake, the streams, and reading a message into one of the outcomes of
// BLI-MSG. This file drives the browser APIs and hands their bytes across, and the boundary is the
// same one the prototype drew.
//
// The interface below is also the seam the test harness fakes at: a fake client feeds the interface
// decoded messages with no wasm and no Bluetooth in the loop, which is what lets the view and the
// subscription lifecycle be tested without pretending to be a Bluetooth stack.

import init, {
	Channel,
	Sticker,
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
	loaded ??= init({ module_or_path: wasmUrl }).then(() => {
		start()
	})
	await loaded
}

export function createClient() {
	let device = null
	let channel = null

	return {
		// Both paths a sticker arrives by land here, and the payload is treated identically once read.
		async readSticker(text) {
			await protocol()
			const sticker = new Sticker(text)
			return { sticker, human: sticker.human, version: sticker.version }
		},

		// Finding the device the sticker belongs to (BLI-ADV, "Matching").
		//
		// The browser gives a chooser rather than the advertisements themselves, and the payload it
		// filters on holds a salt that changes, so the chooser cannot be narrowed to one device ahead
		// of time. It is filtered to devices carrying the bliti service, and the one the operator picks
		// is checked against the sticker before anything is sent to it.
		async connect(sticker, { onEvent, onClosed, onDisconnected }) {
			await protocol()
			device = await navigator.bluetooth.requestDevice({
				filters: [{ services: [service_uuid()] }],
			})

			const advertised = device.name ? sticker.read_local_name(device.name) : undefined
			if (!advertised) {
				throw new Error('That device is not advertising a bliti payload.')
			}
			if (advertised.version !== sticker.version) {
				throw new Error(
					`That device speaks bliti version ${advertised.version}, which this app does not read.`,
				)
			}
			if (!advertised.matches) {
				throw new Error('That is a different bliti device. Pick the one whose sticker you read.')
			}

			const server = await device.gatt.connect()
			const service = await server.getPrimaryService(service_uuid())
			const clientTx = await service.getCharacteristic(client_tx_uuid())
			const deviceTx = await service.getCharacteristic(device_tx_uuid())

			// Writes are acknowledged, so the device is never sent more than it has taken. The bytes are
			// copied because the channel hands over a view it may reuse.
			channel = new Channel(sticker, (bytes) => clientTx.writeValueWithResponse(bytes.slice()))

			deviceTx.addEventListener('characteristicvaluechanged', (event) => {
				channel.receive(new Uint8Array(event.target.value.buffer))
			})
			device.addEventListener('gattserverdisconnected', () => onDisconnected?.())

			// Subscribing to notifications is what opens a session: it is the point at which the device
			// can send. Not to be confused with a bliti subscription, which is a stream.
			await deviceTx.startNotifications()

			await channel.connect(
				CLIENT_NAME,
				CLIENT_VERSION,
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
		},

		// A subscription is a stream: it begins here and ends when the handle is closed, which is the
		// unsubscribe (BLI-MSG, "Subscribing").
		async subscribe(topic, { onEvent, onClosed }) {
			return channel.subscribe(
				topic,
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
		},

		disconnect() {
			if (device?.gatt?.connected) device.gatt.disconnect()
			device = null
			channel = null
		},
	}
}
