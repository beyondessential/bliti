// Handing text on: through the system's share sheet where the browser offers one, and otherwise to
// the clipboard (TSCR).

import { useEffect, useRef, useState } from 'react'

/// Whether the browser offers the system's share sheet.
export const canShare = () => typeof navigator.share === 'function'

/// Share `data` through the share sheet, or copy `fallback` where there is none, or where sharing
/// failed for any reason but the operator dismissing the sheet. Resolves to `copied` where it copied.
export async function share(data, fallback) {
	if (canShare()) {
		try {
			await navigator.share(data)
			return 'shared'
		} catch (error) {
			if (error.name === 'AbortError') return null
		}
	}
	await navigator.clipboard.writeText(fallback)
	return 'copied'
}

/// How long a button says it copied before it reads as itself again.
const SAID = 2_000

/// A label that reads `done` for a moment after `mark` is called, and `label` otherwise.
export function useMomentary(label, done) {
	const [on, setOn] = useState(false)
	const timer = useRef(null)
	useEffect(() => () => clearTimeout(timer.current), [])
	const mark = () => {
		clearTimeout(timer.current)
		setOn(true)
		timer.current = setTimeout(() => setOn(false), SAID)
	}
	return [on ? done : label, mark]
}
