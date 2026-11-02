// Choosing the steps of a run (PWEB, "Choosing the steps").

import { useState } from 'react'

import { Steps } from '../Reaching.jsx'
import { REACHING } from './Finding.jsx'
import { ticked } from './state.js'

/// The device is idle and has not yet found every step's done-state, so no step is listed.
export function Checking({ suffix }) {
	// spec: PWEB
	return (
		<>
			<Steps names={REACHING} at={2} />
			<section>
				<p className="done-line">Connected</p>
				<p className="lead" role="status">
					<span className="spinner" aria-hidden="true" />
					The Iti is checking the board
				</p>
				<span className="chip">{suffix}</span>
			</section>
		</>
	)
}

/// Every step in order with a checkbox, ticked unless the board already has what it makes, and
/// starting a run of those ticked. `live` is whether the device can be asked anything.
export default function Choosing({ state, suffix, live, busy, onStart }) {
	// The ticks the steps' done-states gave when the list was first shown, so a later state does not
	// change them under the operator, and what the operator has changed over them.
	const [seeded] = useState(() => Object.fromEntries(state.steps.map((step) => [step.id, ticked(step)])))
	const [changed, setChanged] = useState({})
	const isTicked = (step) => changed[step.id] ?? seeded[step.id] ?? ticked(step)
	const chosen = state.steps.filter(isTicked).map((step) => step.id)

	return (
		<>
			<Steps names={REACHING} at={2} />
			<section>
				<p className="done-line">Connected</p>
				<p className="lead">Choose what to do</p>
				<p className="muted">Steps it found already done are unticked.</p>
				<span className="chip">{suffix}</span>
				{/* spec: PWEB */}
				<div>
					{state.steps.map((step) => (
						<label key={step.id} className="pick">
							<input
								type="checkbox"
								checked={isTicked(step)}
								disabled={!live}
								onChange={(event) => setChanged((held) => ({ ...held, [step.id]: event.target.checked }))}
							/>
							<span>
								{step.title}
								{step.why && <span className="why-not">{step.why}</span>}
							</span>
						</label>
					))}
				</div>
				{live && (
					<button
						className="block go"
						disabled={chosen.length === 0 || busy}
						onClick={() => onStart(chosen)}
					>
						Start
					</button>
				)}
			</section>
		</>
	)
}
