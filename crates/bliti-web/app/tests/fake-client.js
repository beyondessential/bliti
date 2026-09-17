// A client fed decoded messages directly, standing in for the real one at the message layer.
//
// This is not a fake Bluetooth stack and does not try to be one: it begins where the protocol half
// leaves off, at the outcomes of BLI-MSG, which is exactly the boundary the view is written against.
export const installFakeClient = `
window.__blitiEvents = []
window.__blitiSubscriptions = []
window.__blitiClient = {
	unsupported: () => null,
	async readCode(text) {
		if (!text || text === 'nope') throw new Error('That is not a bliti code.')
		return { qr: { fake: true }, human: 'AHFY-TP4T-6K2M-9WQX', version: 1 }
	},
	async connect(qr, { onEvent, onClosed, onDisconnected }) {
		window.__blitiEmit = onEvent
		window.__blitiDisconnect = onDisconnected
	},
	async subscribe(topic, { onEvent }) {
		// A subscription does not resolve instantly in the real client, and the window while it is in
		// flight is where a close can be missed. The harness can widen that window on purpose.
		if (window.__blitiSubscribeDelay) {
			await new Promise((r) => setTimeout(r, window.__blitiSubscribeDelay))
		}
		const record = { topic, open: true, events: onEvent }
		window.__blitiSubscriptions.push(record)
		window.__blitiEmitSub = onEvent
		return {
			close: async () => {
				record.open = false
			},
		}
	},
	disconnect() {},
}
`

/// Take the application to an open channel, with the code read and the device found.
export async function openChannel(page) {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
	await page.getByPlaceholder('AHFY-TP4T-...').fill('AHFY-TP4T-6K2M-9WQX')
	await page.getByRole('button', { name: 'Use' }).click()
	await page.getByRole('button', { name: 'Find the device' }).click()
}

/// Feed one reading, as the protocol half would have described it.
export async function emit(page, event) {
	await page.evaluate((event) => window.__blitiEmit(event), event)
}
