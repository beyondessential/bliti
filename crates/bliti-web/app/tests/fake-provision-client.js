// A provisioning client fed decoded messages directly, standing in for the real one at the message
// layer, as fake-client.js does for the page that reaches a device (PWEB).
//
// It is supplied through the test seam as window.__blitiProvisionClient. It begins where the protocol
// half leaves off, at the outcomes of MSG, so a test can have the device say anything on the
// provisioning stream: `emitState` sends a `provision-state`, and `say` any other outcome.
//
// Every act the page sends on the stream is recorded in window.__blitiProvisionActs, in order, as the
// members the wasm half would send, leaving out any not given. The device answers each act as the
// protocol has it, with `provision-accepted`, unless a test has queued something else for that act in
// window.__blitiProvisionAnswers, where each entry is one outcome or a list of them.
//
// Each `find` takes its outcome from window.__blitiProvisionPicks, one entry per attempt: 'ok' to
// reach a device (the default), 'nothing' for a chooser closed with nothing picked, 'full' for a
// device with no slot free, 'hold' to hold the attempt once a device is picked until a test calls
// window.__blitiProvisionRelease, and anything else to fail with that as the reason. Each attempt to
// reach the device again takes its outcome from window.__blitiProvisionReturns, 'ok' to reach it,
// 'chooser' where it can only be picked again, and anything else, or nothing, to fail; attempts are
// counted in window.__blitiProvisionReconnects.
//
// window.__blitiProvisionOpening is the `provision-state` message a device sends as a stream opens,
// where a test sets one.
export const installFakeProvisionClient = `
window.__blitiProvisionActs = []
window.__blitiProvisionAnswers = {}
window.__blitiProvisionPicks = []
window.__blitiProvisionReturns = []
window.__blitiProvisionReconnects = 0
window.__blitiProvisionStreams = []
window.__blitiProvisionDisconnects = 0
window.__blitiProvisionOpening = null
window.__blitiProvisionSuffix = 'K7QX'
// Short enough that reaching the device again happens within a test.
window.__blitiTimings = { retry: 50 }
window.__blitiProvisionClient = {
	unsupported: () => window.__blitiProvisionUnsupported ?? null,
	async limits() {
		return window.__blitiProvisionLimits ?? { revisions: [1, 1], lifetime: 30 }
	},
	async find(handlers) {
		const outcome = window.__blitiProvisionPicks.shift() ?? 'ok'
		if (outcome === 'nothing') {
			const nothing = new Error('No device was picked.')
			nothing.name = 'NothingPicked'
			throw nothing
		}
		const suffix = window.__blitiProvisionSuffix
		handlers.onPicked?.({ suffix })
		if (outcome === 'full') {
			const full = new Error('The device is serving as many clients as it can. Try again later.')
			full.name = 'SlotFull'
			throw full
		}
		if (outcome === 'hold') {
			const attempt = (this._attempt = (this._attempt ?? 0) + 1)
			await new Promise((resolve) => (window.__blitiProvisionRelease = resolve))
			if (attempt !== this._attempt) throw new Error('The device was let go of while connecting.')
		} else if (outcome !== 'ok') {
			throw new Error(outcome)
		}
		this._open(handlers)
		return { suffix }
	},
	async reconnect(handlers) {
		window.__blitiProvisionReconnects++
		const outcome = window.__blitiProvisionReturns.shift()
		if (outcome === 'chooser') {
			const chooser = new Error('The device has to be picked again.')
			chooser.name = 'NeedsChooser'
			throw chooser
		}
		if (outcome !== 'ok') throw new Error('Connection attempt failed.')
		this._open(handlers)
	},
	_open({ onDisconnected }) {
		window.__blitiProvisionDisconnect = onDisconnected
	},
	async provision({ onEvent, onClosed }) {
		const stream = { open: true, closedByPage: false }
		window.__blitiProvisionStreams.push(stream)
		window.__blitiProvisionEmit = (event) => stream.open && onEvent(event)
		window.__blitiProvisionEnd = (why) => {
			stream.open = false
			onClosed?.(why ?? null)
		}
		if (window.__blitiProvisionOpening) {
			const opening = window.__blitiProvisionOpening
			setTimeout(() => stream.open && onEvent({ kind: 'message', message: opening }), 0)
		}
		return {
			act(act, members = {}) {
				if (!stream.open) throw new Error('The provisioning stream has ended.')
				const given = Object.fromEntries(Object.entries(members).filter(([, value]) => value !== undefined))
				window.__blitiProvisionActs.push({ act, ...given })
				const queue = window.__blitiProvisionAnswers[act]
				const next = (Array.isArray(queue) ? queue.shift() : undefined) ?? {
					kind: 'message',
					message: { type: 'provision-accepted' },
				}
				setTimeout(() => {
					for (const event of Array.isArray(next) ? next : [next]) if (stream.open) onEvent(event)
				}, 0)
			},
			close() {
				stream.open = false
				stream.closedByPage = true
			},
		}
	},
	disconnect() {
		window.__blitiProvisionDisconnects++
		this._attempt = (this._attempt ?? 0) + 1
		window.__blitiProvisionDisconnect = undefined
	},
}
`

/// A device's message, as the protocol half describes one it understood.
export const message = (body) => ({ kind: 'message', message: body })

/// A step as the device lists it: not done, and not part of any run, unless `extra` says otherwise.
export const step = (id, extra = {}) => ({
	id,
	title: extra.title ?? id[0].toUpperCase() + id.slice(1),
	done: 'not-done',
	state: 'unchosen',
	...extra,
})

/// A prompt as the device lists it, blocking and of kind `notice` unless `extra` says otherwise.
export const prompt = (id, stepId, extra = {}) => ({
	id,
	step: stepId,
	kind: 'notice',
	text: `Do ${id}`,
	blocking: true,
	...extra,
})

/// The date `days` before the phone's own date, as `YYYY-MM-DD`.
export function daysAgo(days, from = new Date()) {
	const date = new Date(from.getFullYear(), from.getMonth(), from.getDate() - days)
	const pad = (n) => String(n).padStart(2, '0')
	return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

/// A `provision-state` as the device sends it: revision 1, an image built today, and no prompt open,
/// unless `body` says otherwise.
export const stateOf = (body = {}) => ({
	type: 'provision-state',
	revision: 1,
	built: daysAgo(0),
	run: 'idle',
	steps: [step('hardware', { title: 'Check the hardware' }), step('eeprom', { title: 'Program the EEPROM' })],
	prompts: [],
	...body,
})

/// Open the provisioning page, with the fake client in place of the real one. `opening` is the
/// `provision-state` a device sends as the stream opens.
export async function openProvision(page, { opening } = {}) {
	await page.addInitScript(installFakeProvisionClient)
	if (opening) await page.addInitScript((opening) => (window.__blitiProvisionOpening = opening), opening)
	await page.goto('/provision')
}

/// Open the page and take it to an open channel, whose device sends `opening` as the stream opens.
export async function openChannel(page, opening = stateOf()) {
	await openProvision(page, { opening })
	await page.getByRole('button', { name: 'Find Iti' }).click()
	await page.getByRole('button', { name: 'Disconnect' }).waitFor()
}

/// Have the device say something on the open provisioning stream.
export async function say(page, event) {
	await page.waitForFunction(() => window.__blitiProvisionEmit !== undefined)
	await page.evaluate((event) => window.__blitiProvisionEmit(event), event)
}

/// Have the device send a `provision-state`, with `body` over the defaults of `stateOf`.
export async function emitState(page, body) {
	await say(page, message(stateOf(body)))
}

/// Every act the page has sent on the provisioning stream, in order.
export async function acts(page) {
	return page.evaluate(() => window.__blitiProvisionActs)
}

/// Close the channel the way the link dropping does.
export async function lose(page, why) {
	await page.evaluate((why) => window.__blitiProvisionDisconnect(why ?? undefined), why ?? null)
}

/// Queue what the device answers the next act `act` with, in place of `provision-accepted`.
export async function answerWith(page, act, ...events) {
	await page.evaluate(
		([act, events]) => {
			window.__blitiProvisionAnswers[act] = [...(window.__blitiProvisionAnswers[act] ?? []), events]
		},
		[act, events],
	)
}

/// Queue the outcome of each attempt to find a device.
export async function picks(page, ...outcomes) {
	await page.evaluate((outcomes) => window.__blitiProvisionPicks.push(...outcomes), outcomes)
}

/// Queue the outcome of each attempt to reach the device again.
export async function returns(page, ...outcomes) {
	await page.evaluate((outcomes) => window.__blitiProvisionReturns.push(...outcomes), outcomes)
}

/// Turn dev mode on by tapping the screen's title seven times in a row (WEB).
export async function devMode(page) {
	await page.locator('.heading.title h1').click({ clickCount: 7 })
}

/// The lines of the activity record, in order.
export async function activity(page) {
	return page.locator('.log .line .said').allTextContents()
}
