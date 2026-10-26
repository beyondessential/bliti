// What every screen shares: its title with its actions beside it, in the same place on each so that
// moving between them keeps the operator at one level (VIEW), and the activity record beneath, shown
// in dev mode (WEB, "Activity").

import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react'

import { loadDev, saveDev } from './remembered.js'

// How many taps in a row on a title turn dev mode on or off, and how long a pause ends a row (WEB).
const TAPS = 7
const ROW = 1_000

/// What a title tap counts towards: dev mode, as the application provides it.
export const TitleTaps = createContext(() => {})

/// Whether the application is in dev mode, and the tap that counts towards turning it on or off.
export function useDevMode() {
	const [on, setOn] = useState(loadDev)
	const row = useRef({ count: 0, at: 0 })
	useEffect(() => saveDev(on), [on])
	const tap = useCallback(() => {
		const now = Date.now()
		const held = row.current
		held.count = now - held.at < ROW ? held.count + 1 : 1
		held.at = now
		if (held.count < TAPS) return
		held.count = 0
		setOn((was) => !was)
	}, [])
	return [on, tap]
}

/// A screen's title, with `sub` beneath it where there is one and the screen's actions beside it.
export function Title({ title, sub, children }) {
	const tap = useContext(TitleTaps)
	const heading = <h1 onClick={tap}>{title}</h1>
	return (
		<div className="heading title">
			{sub ? (
				<div>
					{heading}
					<p className="muted software">{sub}</p>
				</div>
			) : (
				heading
			)}
			{children && <div className="actions">{children}</div>}
		</div>
	)
}

export function Activity({ log }) {
	if (log.length === 0) return null
	return (
		<section>
			<h2>Activity</h2>
			<div className="log">
				{log.map((line, index) => (
					<p key={index} className={`line ${line.direction}`}>
						<time>{clock(line.at)}</time>
						<span className="arrow">{ARROWS[line.direction]}</span>
						<span className="said">{line.text}</span>
					</p>
				))}
			</div>
		</section>
	)
}

const ARROWS = { in: '←', out: '→', note: '·' }

function clock(at) {
	return at.toLocaleTimeString(undefined, {
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
	})
}
