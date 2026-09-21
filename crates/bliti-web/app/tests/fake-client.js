// A client fed decoded messages directly, standing in for the real one at the message layer.
//
// This is not a fake Bluetooth stack and does not try to be one: it begins where the protocol half
// leaves off, at the outcomes of MSG, which is exactly the boundary the view is written against.
//
// The device pushes the `default` feed unprompted, so after connect a feed is already running. It is
// declined with pauseFeed (the client closing the stream) and resumed with resumeFeed (a subscribe
// for `default`). Both are recorded in window.__blitiFeeds so the lifecycle can be asserted.
export const installFakeClient = `
window.__blitiFeeds = []
window.__blitiClient = {
	unsupported: () => null,
	async readCode(text) {
		if (!text || text === 'nope') throw new Error('That is not a bliti code.')
		return { qr: { fake: true }, human: 'AHFY-TP4T-6K2M-9WQX', version: 1 }
	},
	async connect(qr, { onEvent, onClosed, onDisconnected }) {
		this._onEvent = onEvent
		window.__blitiEmit = onEvent
		window.__blitiDisconnect = onDisconnected
		// The device's pushed feed, already running.
		const feed = { source: 'push', open: true, deliver: (event) => feed.open && onEvent(event) }
		window.__blitiFeeds.push(feed)
		this._feed = feed
	},
	pauseFeed() {
		if (this._resuming) this._resuming.cancelled = true
		if (this._feed) {
			this._feed.open = false
			this._feed = null
		}
	},
	async resumeFeed({ onEvent }) {
		if (this._feed || this._resuming) return
		const resuming = { cancelled: false }
		this._resuming = resuming
		if (window.__blitiResumeDelay) {
			await new Promise((r) => setTimeout(r, window.__blitiResumeDelay))
		}
		this._resuming = null
		window.__blitiEmit = onEvent
		const feed = { source: 'subscribe', topic: 'default', open: true, deliver: (event) => feed.open && onEvent(event) }
		window.__blitiFeeds.push(feed)
		if (resuming.cancelled) feed.open = false
		else this._feed = feed
	},
	disconnect() {
		this._feed = null
	},
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

/// Feed one message or outcome, as the protocol half would have described it.
export async function emit(page, event) {
	await page.evaluate((event) => window.__blitiEmit(event), event)
}

/// Close the channel the way the connection ending does, optionally with a reason.
export async function closeChannel(page, why) {
	await page.evaluate((why) => window.__blitiDisconnect(why ?? undefined), why ?? null)
}
