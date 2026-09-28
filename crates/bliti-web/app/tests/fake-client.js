// A client fed decoded messages directly, standing in for the real one at the message layer.
//
// This is not a fake Bluetooth stack and does not try to be one: it begins where the protocol half
// leaves off, at the outcomes of MSG, which is exactly the boundary the view is written against.
//
// The device pushes the `default` feed unprompted, so after connect a feed is already running. It is
// declined with pauseFeed (the client closing the stream) and resumed with resumeFeed (a subscribe
// for `default`). Both are recorded in window.__blitiFeeds so the lifecycle can be asserted.
//
// A configuration session (CFG) is a fake device at the same layer. Every message the page sends on it
// is recorded in window.__blitiSent, in order, and window.__blitiAnswers holds the device's scripted
// answers: for each message type, a queue whose next entry is sent back when the page sends one. An
// entry is one outcome or a list of them. Anything else a test wants the device to say it emits.
//
// A control stream (CTL) is scripted from the same answers, and what the page sends on it is recorded
// in window.__blitiControlSent, apart from the configuration session's.
//
// window.__blitiReturns scripts reconnecting to a device coming back: one entry per attempt, 'ok' to
// reach it, 'chooser' where it can only be picked again, anything else to fail. An attempt with no
// entry fails. Attempts are counted in window.__blitiReconnects.
//
// Setting window.__blitiNothingPicked makes connect fail as a chooser closed with nothing picked does.
export const installFakeClient = `
window.__blitiFeeds = []
window.__blitiSent = []
window.__blitiControlSent = []
window.__blitiAnswers = {}
window.__blitiSessions = []
window.__blitiControls = []
window.__blitiReturns = []
window.__blitiReconnects = 0
// Short enough that coming back, giving up and the power-off hold all happen within a test.
window.__blitiTimings = { retry: 50, giveUp: 1500, hold: 400 }
window.__blitiClient = {
	unsupported: () => null,
	async readCode(text) {
		if (!text || text === 'nope') throw new Error('That is not a bliti code.')
		return { qr: { fake: true }, human: 'AHFY-TP4T-6K2M-9WQX', svg: '<svg xmlns="http://www.w3.org/2000/svg"/>', version: 1, localName: 'AHOW2EZUD4343RQ' }
	},
	async connect(qr, handlers) {
		if (window.__blitiNothingPicked) {
			const nothing = new Error('No device was picked.')
			nothing.name = 'NothingPicked'
			throw nothing
		}
		this._open(handlers)
	},
	async reconnect(qr, handlers) {
		window.__blitiReconnects++
		const outcome = window.__blitiReturns.shift()
		if (outcome === 'chooser') {
			const chooser = new Error('The device has to be picked again.')
			chooser.name = 'NeedsChooser'
			throw chooser
		}
		if (outcome !== 'ok') throw new Error('Connection attempt failed.')
		this._open(handlers)
	},
	_open({ onEvent, onDisconnected }) {
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
	async configure({ onEvent, onClosed }) {
		if (window.__blitiConfigureFails) throw new Error(window.__blitiConfigureFails)
		const session = { open: true, closedByPage: false }
		window.__blitiSessions.push(session)
		const answer = (message) => {
			const queue = window.__blitiAnswers[message.type]
			const next = Array.isArray(queue) ? queue.shift() : undefined
			if (next === undefined) return
			setTimeout(() => {
				for (const event of Array.isArray(next) ? next : [next]) if (session.open) onEvent(event)
			}, 0)
		}
		const send = (message) => {
			if (!session.open) throw new Error('The configuration session has ended.')
			window.__blitiSent.push(message)
			answer(message)
		}
		window.__blitiSession = {
			emit: (event) => session.open && onEvent(event),
			close: (why) => {
				session.open = false
				onClosed?.(why ?? null)
			},
		}
		send({ type: 'configure' })
		return {
			// Copied as the wasm half copies it, by writing it out as JSON.
			propose: (document) => send({ type: 'configuration', document: JSON.parse(JSON.stringify(document)) }),
			confirm: () => send({ type: 'confirm' }),
			discard: () => send({ type: 'discard' }),
			scan: (iface) => send(iface ? { type: 'scan', interface: iface } : { type: 'scan' }),
			survey: (iface) => send(iface ? { type: 'survey', interface: iface } : { type: 'survey' }),
			wps: (method, iface, ssid) =>
				send({ type: 'wps', method, ...(iface ? { interface: iface } : {}), ...(ssid ? { ssid } : {}) }),
			close: () => {
				session.open = false
				session.closedByPage = true
			},
		}
	},
	async control({ onEvent, onClosed }) {
		const stream = { open: true, closedByPage: false }
		window.__blitiControls.push(stream)
		const send = (message) => {
			if (!stream.open) throw new Error('The control stream has ended.')
			window.__blitiControlSent.push(message)
			const queue = window.__blitiAnswers[message.type]
			const next = Array.isArray(queue) ? queue.shift() : undefined
			if (next === undefined) return
			setTimeout(() => {
				for (const event of Array.isArray(next) ? next : [next]) if (stream.open) onEvent(event)
			}, 0)
		}
		window.__blitiControl = {
			emit: (event) => stream.open && onEvent(event),
			close: (why) => {
				stream.open = false
				onClosed?.(why ?? null)
			},
		}
		send({ type: 'control' })
		return {
			act: (act) => send({ type: 'act', act }),
			close: () => {
				stream.open = false
				stream.closedByPage = true
			},
		}
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

/// A device's message, as the protocol half describes one it understood.
export const message = (body) => ({ kind: 'message', message: body })

/// Script the device's next answers to messages of `type`, one entry per message the page sends.
export async function answer(page, type, ...entries) {
	await page.evaluate(
		([type, entries]) => {
			window.__blitiAnswers[type] = [...(window.__blitiAnswers[type] ?? []), ...entries]
		},
		[type, entries],
	)
}

/// Every message the page has sent on configuration sessions, in order.
export async function sent(page) {
	return page.evaluate(() => window.__blitiSent)
}

/// Have the device say something on the open configuration session.
///
/// The screen renders that it is asking the device before the session exists: it opens one only once
/// the wasm module has loaded, so a test that asserts on the waiting state reaches here while the
/// handle is still absent. Waiting for it is what the screen's own spinner is waiting for.
export async function say(page, event) {
	await page.waitForFunction(() => window.__blitiSession !== undefined)
	await page.evaluate((event) => window.__blitiSession.emit(event), event)
}

/// Take the application to the network screen of a device answering `configure` with `document` and
/// `capabilities`, and optionally the state of each candidate.
export async function openNetwork(page, { document, capabilities, states }) {
	await openChannel(page)
	const answers = [message({ type: 'configuration', document, capabilities })]
	if (states) answers.push(message({ type: 'state', attachments: states }))
	await answer(page, 'configure', answers)
	await openNetworkScreen(page)
	await page.getByRole('heading', { name: 'Connections' }).waitFor()
}

/// Go from the device view to the network screen, through the Control screen that holds it (CSCR).
export async function openNetworkScreen(page) {
	await page.getByRole('button', { name: 'Control' }).click()
	await page.getByRole('button', { name: 'Network settings' }).click()
}

/// Every message the page has sent on control streams, in order.
export async function controlSent(page) {
	return page.evaluate(() => window.__blitiControlSent)
}

/// Script the outcome of each attempt to reach a device coming back: 'ok', 'chooser', or a failure.
export async function returns(page, ...outcomes) {
	await page.evaluate((outcomes) => window.__blitiReturns.push(...outcomes), outcomes)
}
