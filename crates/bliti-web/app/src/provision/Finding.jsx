// The screens before a device is on screen: pressing USER and opening the chooser, nothing picked,
// connecting, and something else already connected (PWEB, "The page" and "Finding a device").

import { FakeDevice, Problem, Steps } from '../Reaching.jsx'
import { Title } from '../Screen.jsx'
import Picture from './Picture.jsx'

/// The steps of reaching a device to provision, which the operator is on one of until it is picked.
export const REACHING = ['Press USER', 'Find Iti', 'Provision']

const TROUBLE = {
	// spec: PWEB
	'not-found': {
		headline: "Didn't find an Iti",
		help: 'Only an Iti whose USER button was just pressed is listed. Press it, then try again.',
	},
	// spec: PWEB
	full: {
		headline: 'Something else is connected to that Iti',
		help: 'An Iti talks to one phone at a time. Once the other lets go, press USER and try again.',
	},
	failed: { headline: "Couldn't connect to that Iti", help: 'Press USER on the Iti, then try again.' },
}

/// What a problem finding a device says to the operator, as a line and the detail of a failure.
export function trouble(problem) {
	const { headline, help } = TROUBLE[problem.kind]
	return { headline, help, detail: problem.detail }
}

/// Finding a device: `phase` is null, `choosing` while the chooser is open, or `connecting` once a
/// device is picked. `problem` is why the last attempt ended, where it did not end by choice.
export default function Finding({ phase, problem, offlineReady, labels, dev, onFind, onFake, onCancel, onLabels }) {
	const title = (
		<Title title="Provision an Iti">{!offlineReady && <span className="offline">Not ready offline</span>}</Title>
	)

	if (phase === 'connecting') {
		// spec: PWEB
		return (
			<>
				{title}
				<Steps names={REACHING} at={1} />
				<section>
					<p className="lead">
						<span className="spinner" aria-hidden="true" />
						Connecting to the Iti
					</p>
					<p className="muted">Keep your phone close to it.</p>
					<button className="secondary block" onClick={onCancel}>
						Cancel
					</button>
				</section>
			</>
		)
	}

	if (problem) {
		const { headline, help, detail } = trouble(problem)
		return (
			<>
				{title}
				<Steps names={REACHING} at={1} />
				<section className="steps-body">
					<Problem>{headline}</Problem>
					<p className="muted">{help}</p>
					{detail && <p className="why">{detail}</p>}
					{problem.kind !== 'failed' && <Picture name="user-button" />}
					<button className="block" onClick={onFind} disabled={phase !== null}>
						Try again
					</button>
					{dev && <FakeDevice onClick={onFake} disabled={phase !== null} />}
				</section>
			</>
		)
	}

	// spec: PWEB
	return (
		<>
			{title}
			<Steps names={REACHING} at={phase ? 1 : 0} />
			<section className="steps-body">
				<p className="lead">Press USER on the Iti</p>
				<p className="muted">Only Itis whose button was just pressed show up.</p>
				<Picture name="user-button" />
				<button className="block" onClick={onFind} disabled={phase !== null}>
					Find Iti
				</button>
				{dev && <FakeDevice onClick={onFake} disabled={phase !== null} />}
				{/* spec: PSHT */}
				{labels.list.labels.length > 0 && (
					<div className="links">
						<button className="link" onClick={onLabels}>
							Labels ({labels.unprinted} not printed)
						</button>
					</div>
				)}
			</section>
		</>
	)
}
