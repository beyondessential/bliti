// The configuration session the simple network screens share (BLI-NSET): opened from the Status
// screen, held across the screens, and closed on returning to it. Its state is the network settings
// screen's reducer, which the simple screens read and propose through, and never edit through.

import { useCallback, useEffect, useReducer, useRef, useState } from 'react'

import { check } from './capabilities.js'
import { fromDocument, generatePassphrase, opening, reduce, validate } from './network.js'
import { loadProtocol } from './protocol.js'

/// Where the last proposal made through the session is: `applying`, `applied` or `failed`.
function phaseOf(state) {
	switch (state.stage) {
		case 'applying':
			return 'applying'
		case 'errored':
			return 'failed'
		case 'editing':
			return 'applied'
		default:
			return null
	}
}

export function useSetupSession(client, { onActivity, onEvent }) {
	const [state, dispatch] = useReducer(reduce, undefined, opening)
	// Whether a session is wanted, and how many have been opened, so each opening is one of its own.
	const [run, setRun] = useState({ open: false, count: 0 })
	// How many proposals have been made through this session, so a screen can tell its own apart.
	const [proposals, setProposals] = useState(0)
	// A passphrase for a hotspot not yet set up, drawn once for each session (SHARE).
	const [generated, setGenerated] = useState(null)
	const handle = useRef(null)

	useEffect(() => {
		if (!run.open) return
		let live = true
		let opened = null
		loadProtocol()
			.then(() => {
				if (!live) throw new Error('left')
				return client.configure({
					onEvent: (event) => {
						if (!live) return
						onEvent?.(event)
						dispatch({ type: 'event', event })
					},
					onClosed: (why) => live && dispatch({ type: 'closed', why }),
					onActivity,
				})
			})
			.then((opening) => {
				if (live) handle.current = opened = opening
				else opening.close()
			})
			.catch((error) => live && dispatch({ type: 'closed', why: error.message ?? String(error) }))
		return () => {
			live = false
			opened?.close()
			handle.current = null
		}
	}, [client, run, onActivity, onEvent])

	const open = useCallback(() => {
		dispatch({ type: 'restart' })
		setProposals(0)
		setGenerated(generatePassphrase())
		setRun((was) => ({ open: true, count: was.count + 1 }))
	}, [])

	/// Open again, as on a reconnected channel, keeping what the screens hold: a proposal under way is
	/// read from the outcome the device then reports (WEB).
	const resume = useCallback(() => {
		dispatch({ type: 'resume' })
		setRun((was) => ({ open: true, count: was.count + 1 }))
	}, [])

	// Closed at once, so a session opened straight after it is not busy behind this one (NSET).
	const close = useCallback(() => {
		handle.current?.close()
		handle.current = null
		setRun((was) => (was.open ? { ...was, open: false } : was))
	}, [])

	const send = (act) => {
		try {
			if (!handle.current) throw new Error('The configuration session is not open.')
			act(handle.current)
			return true
		} catch (error) {
			dispatch({ type: 'closed', why: error.message ?? String(error) })
			return false
		}
	}

	/// Propose `document`, checked first as the network settings screen checks it. The problem where
	/// it could not work, and otherwise null, with the proposal's number in `proposals`.
	const propose = (document) => {
		const problem = validate(document) ?? check(document, state.capabilities)
		if (problem) return problem
		if (send((session) => session.propose(document))) {
			dispatch({ type: 'proposed', edit: fromDocument(document) })
			setProposals((count) => count + 1)
		}
		return null
	}

	const scan = () => {
		if (send((session) => session.scan())) dispatch({ type: 'act', act: 'scan' })
	}

	return {
		active: run.open,
		state,
		// The document the device holds, or the one being applied while it is.
		held: state.stage === 'applying' && state.proposed ? state.proposed.document : state.inForce,
		phase: proposals > 0 ? phaseOf(state) : null,
		proposals,
		generated,
		open,
		resume,
		close,
		propose,
		scan,
	}
}
