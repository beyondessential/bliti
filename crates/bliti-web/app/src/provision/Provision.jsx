// The provisioning client (PWEB): the page at `/provision`, through which an operator at a device in
// provisioning mode finds it, chooses the steps of a run, and watches the run through to the board's
// record.

import { useCallback, useEffect, useMemo, useState } from 'react'

import { CLIENT_VERSION } from '../link.js'
import { useOfflineReady } from '../offline.js'
import { Unsupported } from '../Reaching.jsx'
import { Activity, Title, TitleTaps, useDevMode } from '../Screen.jsx'
import Choosing from './Choosing.jsx'
import Done from './Done.jsx'
import Finding, { trouble } from './Finding.jsx'
import { createProvisionClient } from './client.js'
import Labels from './Labels.jsx'
import Refused from './Refused.jsx'
import Run from './Run.jsx'
import { civilDate, labelOf, refusal, screenOf } from './state.js'
import useLabels from './useLabels.js'
import useProvision from './useProvision.js'

// How many activity lines are kept, as for the page that reaches a device.
const LOGGED = 100

export default function Provision() {
	// The seam the harness fakes at, compiled out of any build but the harness's.
	const client = useMemo(() => (__TEST_SEAM__ && window.__blitiProvisionClient) || createProvisionClient(), [])
	const [unsupported] = useState(() => client.unsupported())
	const offlineReady = useOfflineReady()
	const [dev, tap] = useDevMode()
	const [log, setLog] = useState([])
	const note = useCallback(
		(direction, text) => setLog((lines) => [...lines, { at: new Date(), direction, text }].slice(-LOGGED)),
		[],
	)
	const { phase, problem, device, connected, lost, channel, state, limits, refusals, refused, notices, awaiting, usedKey, forget, find, findFake, cancel, disconnect, act } =
		useProvision(client, note)

	// The list of labels, which a run that has made a label adds to whenever the page sees it (PSHT),
	// and whether the operator is looking at it. The page's own, not an act on any device.
	const labels = useLabels()
	const [viewing, setViewing] = useState(false)
	const made = state ? labelOf(state.steps) : null
	const { offer } = labels
	useEffect(() => {
		if (made) offer(made)
	}, [made?.payload, made?.version, made?.at, offer])

	// Whether the operator chose to run the steps of a finished run again. The device holds the
	// finished run until it starts another, so this is the page's own, and ends with the channel or the run.
	const [again, setAgain] = useState(false)
	useEffect(() => {
		setAgain(false)
	}, [channel])
	useEffect(() => {
		if (state?.run !== 'finished') setAgain(false)
	}, [state?.run])

	// Every screen: whatever it holds, and the activity record beneath it in dev mode (PWEB).
	const shell = (body) => (
		<TitleTaps.Provider value={tap}>
			<main>
				{body}
				{dev && <Activity log={log} />}
			</main>
		</TitleTaps.Provider>
	)

	// Where the browser cannot open a channel, the page says so until the fake device is reached (PWEB).
	// Told from the page's state rather than `device`, which outlives the channel.
	if (unsupported && !connected && !lost && !phase) {
		// spec: PWEB
		return shell(<Unsupported title={<Title title="Provision an Iti" />} reason={unsupported} dev={dev} onFake={findFake} />)
	}

	if (viewing) return shell(<Labels labels={labels} onBack={() => setViewing(false)} />)

	if (!connected && !lost) {
		return shell(
			<>
				<Finding
					phase={phase}
					problem={problem}
					offlineReady={offlineReady}
					labels={labels}
					dev={dev}
					onFind={find}
					onFake={findFake}
					onCancel={cancel}
					onLabels={() => setViewing(true)}
				/>
				{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}
			</>,
		)
	}

	// A device on screen: its title with Disconnect beside it while connected (PWEB). What is shown of a
	// device whose channel has closed is faded, and offers nothing that sends an act.
	const suffix = device?.suffix
	const why = state && limits ? refusal(state, { ...limits, today: civilDate() }) : null
	const screen = state && limits && !why ? (state.run === 'finished' && again ? 'choosing' : screenOf(state)) : null
	const named = screen === 'choosing' || screen === 'done' ? undefined : `Code ending ${suffix}`
	const live = connected
	const start = (steps) => act('start', { steps })

	let body
	if (why) body = <Refused refusal={why} />
	else if (screen === 'choosing') {
		body = <Choosing state={state} suffix={suffix} live={live} busy={awaiting.length > 0} onStart={start} />
	} else if (screen === 'run') {
		body = <Run state={state} live={live} awaiting={awaiting} refusals={refusals} usedKey={usedKey} onForget={forget} onAct={act} />
	} else if (screen === 'done') {
		body = (
			<Done
				state={state}
				suffix={suffix}
				live={live}
				labels={labels}
				onAnother={disconnect}
				onAgain={() => setAgain(true)}
				onLabels={() => setViewing(true)}
			/>
		)
	} else if (connected) {
		body = (
			<section>
				<p className="lead">
					<span className="spinner" aria-hidden="true" />
					Checking what the Iti can do
				</p>
			</section>
		)
	}

	return shell(
		<>
			<Title title="Provision an Iti" sub={named}>
				{connected && (
					<button className="secondary small" onClick={disconnect}>
						Disconnect
					</button>
				)}
			</Title>
			{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}
			{lost && <Lost phase={phase} problem={problem} onFind={find} onCancel={cancel} />}
			{notices.map((text) => (
				<p key={text} className="notice fault">
					{text}
				</p>
			))}
			{live && refused && (
				<p className="why" role="alert">
					{refused}
				</p>
			)}
			{body}
		</>,
	)
}

/// The channel has closed: what was last seen stays beneath, and the operator is asked to press USER
/// to check back while the page reaches the device again on its own (PWEB, "When the channel closes").
function Lost({ phase, problem, onFind, onCancel }) {
	const said = problem && trouble(problem)
	// spec: PWEB
	return (
		<section>
			<div className="bar-state working" role="status">
				<p>
					<span className="spinner" aria-hidden="true" />
					{phase === 'connecting' ? 'Connecting to the Iti.' : 'Lost the Iti. Press USER on it to check back.'}
				</p>
				<div className="row">
					{phase === 'connecting' ? (
						<button className="small secondary" onClick={onCancel}>
							Cancel
						</button>
					) : (
						<button className="small" onClick={onFind} disabled={phase !== null}>
							Find Iti
						</button>
					)}
				</div>
				{said && <p className="why">{[said.help, said.detail].filter(Boolean).join(' ')}</p>}
			</div>
		</section>
	)
}
