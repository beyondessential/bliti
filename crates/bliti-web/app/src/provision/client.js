// The browser half of the provisioning client (PWEB): the chooser, the link to the device picked, and
// the provisioning stream. As for the client that reaches a device through its QR code, everything
// with protocol in it is in the wasm module, and this file drives Web Bluetooth and hands bytes across.
//
// The interface below is the seam the test harness fakes at, as it does for that client.

import { createFakeProvisionClient } from '../fake/provision.js'
import { createLink, NEEDS_CHOOSER, unsupportedReason } from '../link.js'
import { loadProtocol as protocol } from '../protocol.js'
import { routed } from '../route.js'
import {
	Channel,
	provisioning_image_lifetime_days,
	provisioning_name_prefixes,
	provisioning_revisions,
	read_provisioning_name,
	service_uuid,
} from '../wasm/bliti_web.js'

/// The client the provisioning page holds: the real one, and the fake device (PWEB, "The fake
/// device"), which finding the fake device reaches in place of the chooser.
export function createProvisionClient() {
	const real = createRealProvisionClient()
	const fake = createFakeProvisionClient({ limits: () => real.limits() })
	// Whichever the channel, or the attempt at one, is with.
	let active = real
	return routed(real, fake, () => active, {
		unsupported: real.unsupported,
		limits: real.limits,
		find(handlers, { fake: wanted = false } = {}) {
			active = wanted ? fake : real
			return active.find(handlers)
		},
	})
}

function createRealProvisionClient() {
	const link = createLink()

	// A channel to the device picked, which no QR code anchors: the handshake is keyed by the
	// provisioning token and compares no fingerprint (PRV). What the device says before a stream is
	// opened, its hello, goes to the record.
	const open = ({ onActivity, onDisconnected }) =>
		link.open((write, chunk) => Channel.for_provisioning(write, chunk), {
			onMessage: (event) =>
				onActivity?.('in', event.kind === 'message' ? event.message.type : `${event.kind}  ${event.detail}`),
			onActivity,
			onDisconnected,
		})

	return {
		unsupported: unsupportedReason,

		/// The revisions of the stream this build implements, lowest first, and how many days before the
		/// phone's own date an image may have been built and still be driven (PRS).
		async limits() {
			await protocol()
			const [lowest, highest] = provisioning_revisions()
			return { revisions: [lowest, highest], lifetime: provisioning_image_lifetime_days() }
		},

		/// Pick a device in provisioning mode in the chooser and open a channel to it (PRV). The chooser
		/// is filtered on the name up to the suffix for each version marker implemented, and the service
		/// of ADV asked for alongside. Resolves with what names the device: the suffix of its name.
		async find(handlers) {
			const say = (direction, text) => handlers.onActivity?.(direction, text)
			await protocol()
			const prefixes = provisioning_name_prefixes()
			say('note', `asking the browser to choose ${prefixes.join(' or ')}`)
			const device = await link.pick({
				filters: prefixes.map((namePrefix) => ({ namePrefix })),
				optionalServices: [service_uuid()],
			})

			const named = device.name ? read_provisioning_name(device.name) : undefined
			if (!named) throw new Error("That isn't an Iti in provisioning mode.")
			const { marker, supported, suffix } = named
			named.free()
			if (!supported) throw new Error(`That Iti speaks version ${marker}, which this page does not read.`)

			say('note', `picked ${device.name}`)
			handlers.onPicked?.({ suffix })
			await open(handlers)
			return { suffix }
		},

		/// Open a channel again to the device picked last, with no chooser. Rejects with NEEDS_CHOOSER
		/// where there is no such device, and otherwise with whatever the attempt met, for the caller to
		/// try again.
		async reconnect(handlers) {
			if (!link.reopenable) {
				const chooser = new Error('The device has to be picked again.')
				chooser.name = NEEDS_CHOOSER
				throw chooser
			}
			handlers.onActivity?.('note', `reconnecting to ${link.device.name}`)
			await open(handlers)
		},

		/// Open the provisioning stream on the open channel (PRS). Every message the device sends on it
		/// reaches onEvent as a feed's messages do, and onClosed is called once the stream ends.
		/// `act(act, { steps, prompt, value })` sends an act, with only the members the act takes.
		async provision({ onEvent, onClosed, onActivity }) {
			if (!link.channel) throw new Error('Not connected to a device.')
			onActivity?.('out', 'provision')
			const handle = await link.channel.provision(
				(json) => onEvent(JSON.parse(json)),
				(why) => onClosed?.(why),
			)
			let closed = false
			return {
				act: (act, { steps, prompt, value } = {}) => {
					if (closed) throw new Error('The provisioning stream has ended.')
					handle.act(act, steps, prompt, value)
				},
				close: () => {
					if (closed) return
					closed = true
					onActivity?.('out', 'end of the provisioning stream')
					handle.close()
					handle.free()
				},
			}
		},

		disconnect() {
			link.disconnect()
		},
	}
}
