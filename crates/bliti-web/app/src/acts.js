// One Tailscale stream (TSC) or Tamanu stream (TMU), held for as long as the screen using it is
// shown: the acts the device lists, the act asked for until the device answers it, and what the
// device said of it. The Status screen's rows, the Tailscale screen and the sign-in screen each hold
// their own, so each opens a stream on showing and closes it on leaving (STAT, TSCR).

import { useCallback, useEffect, useRef, useState } from 'react'

/// The stream of `kind`, `tailscale` or `tamanu`, opened on `client` again with each new channel.
///
/// `acts` is null until the device has listed them, which a device older than this build never does.
/// `asking` is the act asked for, until the device refuses it, sends its result or its failure, or
/// lists its acts anew, and is what holds an act back from being offered again. `awaiting` is the
/// same act until the device answers it however it does, or the stream ends: a network check under
/// way, or a sign-in waiting for its link. `reason` is `{ act, text }`, the device's words for the last refusal or
/// failure, until the next ask. `result` is the latest `{ act, result, at }` the device sent, and
/// `link` the latest login link, until the stream ends or a sign-in fails. `onResult` is told of
/// each result as it arrives.
export function useActs(client, kind, channel, { onActivity, onEvent, onResult }) {
	const [acts, setActs] = useState(null)
	const [asking, setAsking] = useState(null)
	const [awaiting, setAwaiting] = useState(null)
	const [reason, setReason] = useState(null)
	const [result, setResult] = useState(null)
	const [link, setLink] = useState(null)
	const [open, setOpen] = useState(false)
	const stream = useRef(null)
	// The act the device's next result or failure answers, as `awaiting` holds it, for the callbacks.
	const asked = useRef(null)
	const onResult_ = useRef(onResult)
	onResult_.current = onResult

	useEffect(() => {
		let live = true
		let opened = null
		const answered = () => {
			const act = asked.current
			asked.current = null
			setAsking(null)
			setAwaiting(null)
			return act
		}
		client[kind]({
			onEvent: (event) => {
				if (!live) return
				onEvent?.(event)
				if (event.kind !== 'message') return
				const message = event.message
				switch (message.type) {
					case `${kind}-acts`:
						setActs(message.acts)
						setAsking(null)
						break
					case 'cannot-act':
						setReason({ act: answered() ?? message.act, text: message.reason })
						break
					case `${kind}-failed`: {
						const act = answered()
						setReason({ act, text: message.reason })
						if (act === 'sign-in') setLink(null)
						break
					}
					case 'tailscale-result': {
						const held = { act: answered(), result: message.result, at: new Date() }
						setResult(held)
						onResult_.current?.(held)
						break
					}
					case 'tailscale-login-link':
						setLink(message.url)
						break
				}
			},
			onClosed: () => {
				if (!live) return
				stream.current = null
				asked.current = null
				setOpen(false)
				setActs(null)
				setAsking(null)
				setAwaiting(null)
				setLink(null)
			},
			onActivity,
		})
			.then((handle) => {
				if (!live) return handle.close()
				stream.current = opened = handle
				setOpen(true)
			})
			.catch((error) => live && onActivity?.('note', `could not open a ${kind} stream: ${error.message ?? error}`))
		return () => {
			live = false
			opened?.close()
			stream.current = null
		}
	}, [client, kind, channel, onActivity, onEvent])

	const ask = useCallback((act, members) => {
		setReason(null)
		setResult((held) => (held?.act === act ? null : held))
		if (act === 'sign-in') setLink(null)
		try {
			if (!stream.current) throw new Error('Not connected to the Iti.')
			stream.current.act(act, members)
			asked.current = act
			setAsking(act)
			setAwaiting(act)
		} catch (error) {
			setReason({ act, text: error.message ?? String(error) })
		}
	}, [])

	return { acts, asking, awaiting, reason, result, link, open, ask }
}
