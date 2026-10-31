// The fake device of PWEB, "The fake device": an Iti in provisioning mode every build carries, which
// the provisioning page reaches with no channel and no Bluetooth.
//
// It stands in at the messages of MSG, as the fake of the page that reaches a device does. It is idle
// once connected, and a run it is asked to start passes every step chosen at once and finishes, with no
// result for any step, so it adds nothing to the list of labels.

import { civilDate } from '../provision/state.js'
import { FAKE_SUFFIX } from './name.js'


// Every step of RUN, in its order.
const STEPS = [
	['hardware', 'Check the hardware'],
	['eeprom', 'Program the EEPROM'],
	['disk', 'Write the system to the disk'],
	['overlay', 'Install the HAT overlay'],
	['encryption', 'Set up disk encryption'],
	['tailscale', 'Sign in to Tailscale'],
	['bootloader', 'Configure the bootloader'],
	['label', 'Make the label'],
]

const message = (body) => ({ kind: 'message', message: body })

// How long a run is `running` before it is `finished`, enough for the page to show it. The harness
// can lengthen it, to act within it.
const SEEN = (__TEST_SEAM__ && window.__blitiTimings?.seen) || 50

/// A `provision-state` of the fake device in `run`, with each step of `chosen` as `passed`, or as
/// `pending` while the run is `running`, and `unchosen` for the rest, of the revision and the date the
/// page implements.
function stateOf(revision, run, chosen = []) {
	const chosenState = run === 'running' ? 'pending' : 'passed'
	return {
		type: 'provision-state',
		revision,
		built: civilDate(),
		run,
		steps: STEPS.map(([id, title]) => ({ id, title, done: 'not-done', state: chosen.includes(id) ? chosenState : 'unchosen' })),
		prompts: [],
	}
}

const refusal = (act, at, reason) => ({ type: 'cannot-act', act, at, reason })

/// `limits()` is the page's own, which carries the highest revision of the steps it implements.
export function createFakeProvisionClient({ limits }) {
	let open = false
	// Ends the provisioning stream open on the channel, which letting the device go takes with it.
	let end = () => {}

	return {
		async find(handlers) {
			handlers.onActivity?.('note', 'the fake device has no chooser')
			handlers.onPicked?.({ suffix: FAKE_SUFFIX })
			open = true
			return { suffix: FAKE_SUFFIX }
		},

		async reconnect(handlers) {
			handlers.onActivity?.('note', 'reconnecting to the fake device')
			open = true
		},

		async provision({ onEvent, onActivity }) {
			if (!open) throw new Error('Not connected to a device.')
			const {
				revisions: [, revision],
			} = await limits()
			onActivity?.('out', 'provision')
			let live = true
			end = () => (live = false)
			let run = 'idle'
			let chosen = []
			const emit = (...bodies) =>
				setTimeout(() => {
					for (const body of bodies) if (live) onEvent(message(body))
				}, 0)
			const settle = () => stateOf(revision, run, chosen)
			emit(settle())

			// Each act is answered once. Only `start` and `acknowledge` are ever accepted, because the fake
			// device never holds a prompt, and never fails, waits or runs for any time.
			const act = (name, { steps } = {}) => {
				if (!live) throw new Error('The provisioning stream has ended.')
				const idle = run === 'idle' || run === 'finished'
				if (name === 'start' && idle) {
					const unknown = (steps ?? []).findIndex((id) => !STEPS.some(([known]) => known === id))
					if (!steps?.length) return emit(refusal(name, "$['steps']", 'no step was chosen'))
					if (unknown !== -1) return emit(refusal(name, `$['steps'][${unknown}]`, `no step ${steps[unknown]}`))
					// The run is seen to start before it finishes, which is what ends the operator's choosing
					// steps again on a finished run.
					run = 'running'
					chosen = steps
					emit({ type: 'provision-accepted' }, settle())
					return setTimeout(() => {
						if (!live || run !== 'running') return
						run = 'finished'
						onEvent(message(settle()))
					}, SEEN)
				}
				if (name === 'acknowledge' && run === 'finished') {
					run = 'idle'
					chosen = []
					return emit({ type: 'provision-accepted' }, settle())
				}
				if (name === 'answer') return emit(refusal(name, "$['prompt']", 'no prompt is open'))
				return emit(refusal(name, '$', `the fake device is ${run}, and does not take ${name}`))
			}
			return {
				act,
				close: () => {
					if (!live) return
					live = false
					onActivity?.('out', 'end of the provisioning stream')
				},
			}
		},

		disconnect() {
			open = false
			end()
		},
	}
}
