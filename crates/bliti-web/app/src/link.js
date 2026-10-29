// The link to one device over Web Bluetooth (BLI-WEB), which the client that reaches a device
// through its QR code and the provisioning client both open: picking it in the chooser, the GATT
// connection and its characteristics, and the channel above them.
//
// The channel itself is made by the caller, because what it is keyed by differs: a QR code for one
// and the provisioning token for the other.

import { allocation_uuid, client_tx_uuid, read_allocation, service_uuid } from './wasm/bliti_web.js'

/// What this client calls itself to a device. Opaque to the device, which logs it (BLI-MSG).
export const CLIENT_NAME = 'bliti-web'
export const CLIENT_VERSION = __APP_VERSION__

/// The name of the error `pick` throws where the chooser closed with nothing picked. The browser
/// does not say whether the list was empty or the operator dismissed it (WEB).
export const NOTHING_PICKED = 'NothingPicked'

/// The name of the error `reopen` throws where the device can only be reached again through the
/// chooser (WEB).
export const NEEDS_CHOOSER = 'NeedsChooser'

/// The name of the error `open` throws where the device has no slot free (CHN).
export const SLOT_FULL = 'SlotFull'

/// Why this browser cannot reach a device, or null where it can: a secure context and Web Bluetooth
/// are what it needs.
export function unsupportedReason() {
	if (!window.isSecureContext) {
		return 'This page needs a secure context. Open it over https, or over localhost while developing.'
	}
	if (!navigator.bluetooth) {
		return 'Open this page in Chrome on an Android phone.'
	}
	return null
}

export function createLink() {
	let device = null
	let channel = null

	// The listeners the open channel put on the device, so a later open can take them off again: a
	// link dropped under an old channel must not reach the operator as the new one closing.
	let detach = () => {}
	// Bumped on disconnect, so an open still in flight when the operator lets the device go does not
	// leave a channel behind it.
	let generation = 0

	return {
		/// The device picked, and the channel last opened to it.
		get device() {
			return device
		},
		get channel() {
			return channel
		},

		/// Ask the browser for a device, rejecting with NOTHING_PICKED where the chooser closed with none.
		async pick(options) {
			try {
				device = await navigator.bluetooth.requestDevice(options)
			} catch (error) {
				// Told apart here because a GATT lookup below rejects with the same name.
				if (error.name === 'NotFoundError') {
					const nothing = new Error('No device was picked.')
					nothing.name = NOTHING_PICKED
					throw nothing
				}
				throw error
			}
			return device
		},

		/// Whether a device was picked that can be opened again without the chooser.
		get reopenable() {
			return Boolean(device?.gatt)
		},

		/// Open a channel to the device picked: the GATT connection, the characteristics, the handshake,
		/// and the streams above it. `make(write, chunk)` builds the channel, and `onMessage` is told of
		/// each message the device sends before any stream is opened on it.
		async open(make, { onMessage, onClosed, onDisconnected, onActivity }) {
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
			// The device notifies each client on a characteristic of its own, and says which, with the
			// ATT_MTU of the link, which the browser does not tell the page (CHN).
			const allocation = await service.getCharacteristic(allocation_uuid())
			const given = read_allocation(new Uint8Array((await allocation.readValue()).buffer))
			current()
			if (!given) {
				if (device?.gatt?.connected) device.gatt.disconnect()
				const full = new Error('The device is serving as many clients as it can. Try again later.')
				full.name = SLOT_FULL
				throw full
			}
			const slot = given.slot
			const chunk = given.chunk
			given.free()
			const deviceTx = await service.getCharacteristic(slot)
			current()

			// Writes are acknowledged, so the device is never sent more than it has taken, and each fills
			// what the link carries, so few are needed (CHN). The bytes are copied because the channel
			// hands over a view it may reuse.
			const opened = make((bytes) => clientTx.writeValueWithResponse(bytes.slice()), chunk)
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
				(json) => onMessage(JSON.parse(json)),
				(why) => onClosed?.(why),
				(why) => closeOnce(why),
			)
			current()
		},

		/// Let go of the device and forget it, and of any open still in flight.
		disconnect() {
			generation++
			detach()
			detach = () => {}
			if (device?.gatt?.connected) device.gatt.disconnect()
			device = null
			channel = null
		},
	}
}
