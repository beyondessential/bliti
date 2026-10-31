// The provisioning page's hold on a device: finding it, the channel and the provisioning stream on it,
// what it last said, and reaching it again once it has gone (PWEB). The screens only read this and
// ask for acts through it.

import { useCallback, useEffect, useRef, useState } from 'react'

import { NEEDS_CHOOSER, NOTHING_PICKED, SLOT_FULL } from '../link.js'
import { actSaid, stateSaid } from './state.js'
import { STEP, forgetKey, isKeyPrompt, saveKey, savedKey } from './tailscale.js'

// How long to wait between attempts to reach the device again. The harness shortens it, which is the
// only thing it changes.
const TIMINGS = { retry: 3_000, ...(__TEST_SEAM__ ? window.__blitiTimings : undefined) }

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

/// Whether the page is shown, which is when it reaches a device again (PWEB, "When the channel
/// closes").
function useVisible() {
	const [visible, setVisible] = useState(!document.hidden)
	useEffect(() => {
		const update = () => setVisible(!document.hidden)
		document.addEventListener('visibilitychange', update)
		return () => document.removeEventListener('visibilitychange', update)
	}, [])
	return visible
}

/// Why finding a device failed, as the screen says it: nothing picked, no slot free, or the reason.
function problemOf(error) {
	if (error.name === NOTHING_PICKED) return { kind: 'not-found' }
	if (error.name === SLOT_FULL) return { kind: 'full' }
	return { kind: 'failed', detail: error.message ?? String(error) }
}

/// `note(direction, text)` adds a line to the activity record.
export default function useProvision(client, note) {
	// Where an attempt to find a device is: null, `choosing` while the chooser is open, or
	// `connecting` once a device is picked. The attempt itself is held in a ref, so one cancelled is
	// told apart from the one after it.
	const [phase, setPhase] = useState(null)
	const attempt_ = useRef(null)
	// Why the last attempt ended, where it did not end by choice.
	const [problem, setProblem] = useState(null)
	// The device picked, by what names it.
	const [device, setDevice] = useState(null)
	const [connected, setConnected] = useState(false)
	const connected_ = useRef(false)
	// Whether the channel closed other than by the operator letting the device go (PWEB).
	const [lost, setLost] = useState(false)
	const lost_ = useRef(false)
	lost_.current = lost
	// How many channels have opened, so each opens a provisioning stream of its own.
	const [channel, setChannel] = useState(0)
	// The last `provision-state` the device sent, kept once the channel has gone.
	const [state, setState] = useState(null)
	const [limits, setLimits] = useState(null)
	// What the device said of acts it refused: an answer, by the prompt it was for, or any other act.
	const [refusals, setRefusals] = useState({})
	const [refused, setRefused] = useState(null)
	const [notices, setNotices] = useState([])
	// The acts sent that the device has not yet answered, oldest first, which it answers in turn (PRS).
	const [awaiting, setAwaiting] = useState([])
	const pending_ = useRef([])
	const stream_ = useRef(null)
	const said_ = useRef('')
	// The saved Tailscale key answered a prompt: when it was saved, and whether it has been forgotten
	// since (PWEB, "Tailscale details"). The prompt it answered, so it is answered once.
	const [usedKey, setUsedKey] = useState(null)
	const keyPrompt_ = useRef(false)
	// The automatic attempt to reach the device again, so one taken over by the operator is told apart.
	const reach_ = useRef(null)
	const visible = useVisible()

	// What the limits on the medium are, which the wasm module holds, once it has loaded (PRS).
	useEffect(() => {
		if (limits) return
		let live = true
		client
			.limits()
			.then((found) => live && setLimits(found))
			.catch((error) => live && note('note', `could not load the limits: ${error.message ?? error}`))
		return () => {
			live = false
		}
	}, [client, channel, limits, note])

	// Nothing of a device held, and nothing reaching for it.
	const reset = useCallback(() => {
		attempt_.current = null
		reach_.current = null
		connected_.current = false
		pending_.current = []
		said_.current = ''
		setPhase(null)
		setConnected(false)
		setLost(false)
		setState(null)
		setRefusals({})
		setRefused(null)
		setAwaiting([])
		setUsedKey(null)
		keyPrompt_.current = false
	}, [])

	const closed = useCallback(
		(why) => {
			// A channel still opening answers for itself.
			if (!connected_.current) return
			note('note', why ? `the connection to the device closed: ${why}` : 'the device disconnected')
			connected_.current = false
			setConnected(false)
			setLost(true)
		},
		[note],
	)

	// A channel open, a first or a reached again one.
	const opened = useCallback(() => {
		connected_.current = true
		setConnected(true)
		setLost(false)
		setPhase(null)
		setProblem(null)
		setRefusals({})
		setRefused(null)
		setChannel((count) => count + 1)
	}, [])

	const answered = useCallback(() => {
		const [asked, ...rest] = pending_.current
		pending_.current = rest
		setAwaiting(rest)
		return asked
	}, [])

	const notice = useCallback(
		(text) => setNotices((held) => (held.includes(text) ? held : [...held, text].slice(-20))),
		[],
	)

	// One message off the wire, already sorted into the outcomes of MSG by the protocol half.
	const onEvent = useCallback(
		(event) => {
			switch (event.kind) {
				case 'message': {
					const message = event.message
					if (message.type === 'provision-state') {
						setState(message)
						// Said when something other than a progress changed, so a long step does not fill the record.
						const said = stateSaid(message)
						if (said !== said_.current) note('in', said)
						said_.current = said
						const open = new Set(message.prompts.map((prompt) => prompt.id))
						setRefusals((held) =>
							Object.keys(held).every((id) => open.has(id))
								? held
								: Object.fromEntries(Object.entries(held).filter(([id]) => open.has(id))),
						)
					} else if (message.type === 'provision-accepted') {
						answered()
						note('in', 'provision-accepted')
					} else if (message.type === 'cannot-act') {
						const asked = answered()
						note('in', `cannot-act  ${message.act}  ${message.at}  ${message.reason}`)
						if (asked?.act === 'answer' && asked.prompt !== undefined) {
							if (asked.key) {
								forgetKey()
								setUsedKey(null)
							}
							setRefusals((held) => ({ ...held, [asked.prompt]: message.reason }))
						} else {
							setRefused(message.reason)
						}
					} else {
						note('in', message.type)
					}
					break
				}
				case 'skipped':
					note('in', `skipped  ${event.detail}`)
					break
				case 'refused':
					note('in', `refused  ${event.detail}`)
					notice(`The Iti said something this version of the page is too old to act on: ${event.detail}`)
					break
				case 'fault':
					note('in', `fault  ${event.detail}`)
					notice(`The Iti is not speaking the protocol: ${event.detail}`)
					break
			}
		},
		[note, notice, answered],
	)

	// The provisioning stream lasts as long as the channel does (PWEB). A channel that fails to open
	// one is let go of.
	useEffect(() => {
		if (!connected) return
		let live = true
		let handle = null
		pending_.current = []
		setAwaiting([])
		client
			.provision({
				onEvent: (event) => live && onEvent(event),
				onClosed: (why) => {
					if (!live) return
					stream_.current = null
					note('note', why ? `the provisioning stream ended: ${why}` : 'the provisioning stream ended')
				},
				onActivity: note,
			})
			.then((opened) => {
				if (live) stream_.current = handle = opened
				else opened.close()
			})
			.catch((error) => {
				if (!live) return
				const why = error.message ?? String(error)
				note('note', `could not open the provisioning stream: ${why}`)
				client.disconnect()
				reset()
				setProblem({ kind: 'failed', detail: why })
			})
		return () => {
			live = false
			stream_.current = null
			try {
				handle?.close()
			} catch {
				// The channel it was on has gone, and takes the stream with it.
			}
		}
	}, [client, connected, channel, onEvent, note, reset])

	// Reaching the device again on its own once the channel has closed, without the chooser, for as
	// long as the page is shown (PWEB, "When the channel closes").
	useEffect(() => {
		if (!lost || phase || !visible) return
		let live = true
		;(async () => {
			while (live) {
				await sleep(TIMINGS.retry)
				if (!live) return
				const attempt = {}
				reach_.current = attempt
				try {
					await client.reconnect({ onActivity: note, onDisconnected: closed })
				} catch (error) {
					if (reach_.current !== attempt) return
					reach_.current = null
					if (error.name === NEEDS_CHOOSER) return
					note('note', `not back yet: ${error.message ?? error}`)
					continue
				}
				if (reach_.current !== attempt) return
				reach_.current = null
				note('note', 'the device is back')
				opened()
				return
			}
		})()
		return () => {
			live = false
		}
	}, [client, lost, phase, visible, note, closed, opened])

	// Left to go: the device is let go of with the page.
	useEffect(() => () => client.disconnect(), [client])

	// Open the chooser and connect to the device picked, or connect to the fake device with no chooser,
	// from finding a device or from having lost one.
	const reach = useCallback(async (fake) => {
		const attempt = {}
		attempt_.current = attempt
		reach_.current = null
		setPhase('choosing')
		setProblem(null)
		const current = () => attempt_.current === attempt
		try {
			await client.find(
				{
					onActivity: note,
					onDisconnected: closed,
					onPicked: (picked) => {
						if (!current()) return
						setDevice(picked)
						setPhase('connecting')
					},
				},
				{ fake },
			)
		} catch (error) {
			if (!current()) return
			attempt_.current = null
			const why = problemOf(error)
			note('note', why.kind === 'not-found' ? 'no device chosen' : `could not connect: ${error.message ?? error}`)
			setProblem(why)
			setPhase(null)
			// A device lost keeps the one it had, which is what reaching it again goes to.
			if (!lost_.current) client.disconnect()
			return
		}
		// Cancelled after the channel opened anyway: let go of it, unless another attempt is under way.
		if (!current()) {
			if (!attempt_.current) client.disconnect()
			return
		}
		attempt_.current = null
		opened()
	}, [client, note, closed, opened])

	const find = useCallback(() => reach(false), [reach])
	const findFake = useCallback(() => reach(true), [reach])

	/// Abandon connecting, back at finding a device.
	const cancel = useCallback(() => {
		attempt_.current = null
		client.disconnect()
		setPhase(null)
		note('note', 'connecting cancelled')
	}, [client, note])

	/// Let the device go, back at finding one.
	const disconnect = useCallback(() => {
		client.disconnect()
		reset()
		setProblem(null)
		note('note', 'disconnected')
	}, [client, reset, note])

	/// Send an act on the provisioning stream. `members` are `steps`, `prompt` and `value`, and
	/// `secret` says `value` is one that is not to be recorded or kept (PWEB), except the Tailscale key,
	/// which is kept unless it came from storage.
	const act = useCallback(
		(name, members = {}, { secret = false, fromStorage = false } = {}) => {
			const { steps, prompt, value } = members
			setRefused(null)
			if (name === 'answer') setRefusals(({ [prompt]: _, ...rest }) => rest)
			note('out', actSaid(name, { ...members, secret }))
			if (name === 'answer' && secret && prompt === 'auth-key' && typeof value === 'string' && value !== '') {
				// Typed here rather than answered from storage, so it is kept for the next board.
				if (!fromStorage) saveKey(value)
			}
			try {
				if (!stream_.current) throw new Error('Not connected to the Iti.')
				stream_.current.act(name, { steps, prompt, value })
				pending_.current = [...pending_.current, { act: name, prompt, key: secret && prompt === 'auth-key' }]
				setAwaiting(pending_.current)
			} catch (error) {
				const why = error.message ?? String(error)
				if (name === 'answer') setRefusals((held) => ({ ...held, [prompt]: why }))
				else setRefused(why)
			}
		},
		[note],
	)

	// The saved key answers the prompt that asks for it, once, without the field; a failed Tailscale
	// step discards it (PWEB, "Tailscale details").
	useEffect(() => {
		if (!state) return
		const step = state.steps.find((each) => each.id === STEP)
		if (step?.state === 'failed') {
			forgetKey()
			setUsedKey(null)
		}
		const prompt = state.prompts.find(isKeyPrompt)
		if (!prompt) {
			keyPrompt_.current = false
			return
		}
		if (!connected || lost || keyPrompt_.current) return
		const saved = savedKey()
		if (!saved) return
		keyPrompt_.current = true
		setUsedKey({ saved: saved.saved, forgotten: false })
		note('note', 'answering the Tailscale key from the one saved')
		act('answer', { prompt: prompt.id, value: saved.key }, { secret: true, fromStorage: true })
	}, [state, connected, lost, act, note])

	/// Forget the saved key, which the boards after this one will ask for again.
	const forget = useCallback(() => {
		forgetKey()
		setUsedKey((held) => held && { ...held, forgotten: true })
		note('note', 'forgot the saved Tailscale key')
	}, [note])

	return {
		usedKey,
		forget,
		phase,
		problem,
		device,
		connected,
		lost,
		channel,
		state,
		limits,
		refusals,
		refused,
		notices,
		awaiting,
		find,
		findFake,
		cancel,
		disconnect,
		act,
	}
}
