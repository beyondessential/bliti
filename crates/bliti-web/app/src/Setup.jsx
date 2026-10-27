// The Network and Cable screens of NSET, and what every simple network screen shares: its title with
// the way back beside it, and what the session is doing while it is not open.

import { useEffect, useRef, useState } from 'react'

import { writable } from './network.js'
import { Title } from './Screen.jsx'
import { cablePlugged, uplink, useCable, waysOf, waysOn } from './setup.js'
import { Way } from './Status.jsx'

/// A simple network screen: `children` once the session is open, and in its place what it is doing
/// while it is not. `leaving` is the confirmation shown before the operator leaves with edits not
/// applied, and `reconnecting` says the application is reaching the device again (NSET, WEB).
export function SetupFrame({ title, onBack, session, reconnecting, onReopen, leaving, children }) {
	const { state } = session
	let body = children
	if (reconnecting) {
		body = (
			<section className="bar-state working" role="status">
				<p>
					<span className="spinner" aria-hidden="true" />
					Reconnecting to your Iti…
				</p>
			</section>
		)
	} else if (state.status === 'opening') {
		body = (
			<p className="muted">
				<span className="spinner" aria-hidden="true" />
				Asking the Iti for its network settings.
			</p>
		)
	} else if (state.status === 'closed') {
		body = (
			<>
				<p className="notice fault">{state.why ? `The Iti stopped answering: ${state.why}` : 'The Iti ended the session.'}</p>
				<button className="secondary" onClick={onReopen}>
					Open again
				</button>
			</>
		)
	}
	return (
		<div className="setup">
			<Title title={title}>
				<button className="secondary small" onClick={onBack}>
					Back
				</button>
			</Title>
			{leaving}
			{state.status === 'open' && state.notice && <p className="notice fault">{state.notice}</p>}
			{body}
		</div>
	)
}

/// The number of the proposal a screen made through the session, and where it is: null until the
/// screen proposes, and once another proposal has been made since.
export function useOwnProposal(session) {
	const [mine, setMine] = useState(null)
	const phase = mine !== null && mine === session.proposals ? session.phase : null
	return {
		phase,
		// Propose `document`, returning the problem that kept it from being proposed, if any.
		propose: (document) => {
			const problem = session.propose(document)
			if (!problem) setMine(session.proposals + 1)
			return problem
		},
		forget: () => setMine(null),
	}
}

/// Return the operator to the Status screen once, however many things say to.
export function useDone(onDone) {
	const done = useRef(false)
	return () => {
		if (done.current) return
		done.current = true
		onDone()
	}
}

const WAYS = [
	{ way: 'cable', title: 'Cable', screen: 'cable' },
	{ way: 'wifi', title: 'Wifi', screen: 'wifi' },
	{ way: 'hotspot', title: 'Iti hotspot', screen: 'hotspot' },
]

/// The Network screen: the three ways, each opening a screen of its own, or only the way to the
/// network settings where the configuration is outside the simple states.
export function NetworkScreen({ session, entries, onBack, onOpen, onSettings, ...frame }) {
	const document = session.state.inForce
	const ways = waysOn(document) && waysOf(document, entries)
	return (
		<SetupFrame title="Network" onBack={onBack} session={session} {...frame}>
			<section>
				<p className="muted intro">Phones and laptops must be on the same network as the Iti.</p>
				{!ways ? (
					<>
						<p className="lead">Set up under Advanced</p>
						<p className="muted">This Iti is using an advanced network setup. Proceed only if you know what you're doing or are following instructions.</p>
						<button className="block" onClick={onSettings}>
							Open network settings
						</button>
					</>
				) : (
					<ul className="ways">
						{WAYS.map(({ way, title, screen }) => {
							const { inUse, says } = ways[way]
							return (
								<li key={way}>
									<button className={inUse ? 'on' : undefined} onClick={() => onOpen(screen)}>
										<Way way={way} />
										<span className="body">
											<span className="name">{title}</span>
											<span className="now">
												{says.map((part, index) => (
													<span key={index}>
														{index > 0 && <span className="sep">·</span>}
														{part}
													</span>
												))}
											</span>
										</span>
										{inUse && <span className="tick" role="img" aria-label="In use" />}
										<span className="chevron" aria-hidden="true">
											›
										</span>
									</button>
								</li>
							)
						})}
					</ul>
				)}
			</section>
		</SetupFrame>
	)
}

/// The Cable screen: waits for a cable, then uses it, and returns to the Status screen once it works.
export function CableScreen({ session, entries, onBack, onDone, ...frame }) {
	const { state } = session
	const own = useOwnProposal(session)
	const done = useDone(onDone)
	const [problem, setProblem] = useState(null)
	const on = !!waysOn(state.inForce)?.cable
	const plugged = cablePlugged(entries)
	const inUse = uplink(entries)?.way === 'cable'
	const [inUseAtOpen] = useState(inUse)
	const proposed = own.phase !== null

	// Wifi stays on until there is a cable to move to (NSET).
	useEffect(() => {
		if (on || !plugged || proposed || problem || !writable(state)) return
		setProblem(own.propose(useCable(state.inForce)))
	})

	useEffect(() => {
		if (own.phase === 'applied' || (on && inUse && !inUseAtOpen)) done()
	})

	let body
	if (own.phase === 'applying' || (!on && plugged && !proposed && !problem)) {
		body = (
			<>
				<p className="lead">
					<span className="spinner" aria-hidden="true" />
					Switching to the cable
				</p>
				<p className="muted">This can take a moment.</p>
			</>
		)
	} else if (own.phase === 'failed' || problem) {
		body = (
			<>
				<p className="lead problem">
					<span className="alert" aria-hidden="true">
						!
					</span>
					The cable isn't connecting
				</p>
				<p className="muted">It's plugged in, but the network it reaches didn't give the Iti a connection.</p>
				<p className="muted reason">{problem?.reason ?? state.failure?.reason}</p>
			</>
		)
	} else if (on && inUse) {
		body = <p className="lead">The Iti is connected by cable</p>
	} else if (on && plugged) {
		body = (
			<>
				<p className="lead problem">
					<span className="alert" aria-hidden="true">
						!
					</span>
					The cable isn't connecting
				</p>
				<p className="muted">It's plugged in, but the network it reaches hasn't given the Iti a connection.</p>
			</>
		)
	} else {
		body = (
			<>
				<p className="lead">Plug in a network cable</p>
				<p className="muted">Connect the Iti's network port to the site's router or a wall socket.</p>
				<p className="muted">
					<span className="spinner" aria-hidden="true" />
					Waiting for a cable…
				</p>
			</>
		)
	}

	return (
		<SetupFrame title="Cable" onBack={onBack} session={session} {...frame}>
			<section>{body}</section>
		</SetupFrame>
	)
}
