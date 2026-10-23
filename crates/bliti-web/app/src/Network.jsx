// The network configuration screen of NSCR: the ordering, one candidate's fields, the hotspot and
// the country, edited here and proposed only when the operator applies, with the state of the session
// pinned in view. The session closes when the screen does.

import { useEffect, useReducer, useRef, useState } from 'react'

import { absent, acts, attachmentKinds, check, countries, hotspotBlockedBy, hotspotOn } from './capabilities.js'
import {
	candidateAt,
	changes,
	checking,
	addCandidate,
	describeCandidate,
	failureHeadline,
	moveCandidate,
	newKey,
	opening,
	reduce,
	removeCandidate,
	setMember,
	stagesOf,
	stateFor,
	stateWording,
	turning,
	turningHotspot,
	unapplied,
	uncheckable,
	unedited,
	updateCandidate,
	validate,
	writable,
} from './network.js'
import {
	CandidateFields,
	CountryField,
	HotspotFields,
	blankCandidate,
	blankHotspot,
	fitted,
	kindName,
} from './NetworkFields.jsx'
import { pathOf, within } from './path.js'
import { bandName } from './wireless.js'
import { loadProtocol } from './protocol.js'

export default function Network({ client, onActivity, onEvent, onBack, onDisconnect, joined }) {
	const [state, dispatch] = useReducer(reduce, undefined, opening)
	const [attempt, setAttempt] = useState(0)
	const [leaving, setLeaving] = useState(false)
	const session = useRef(null)

	// The session lasts as long as this is mounted: unmounting closes the stream, which changes nothing
	// the device holds (CFG), and drops the edits (NSCR). It opens once the checker a document is held
	// to before it is proposed is loaded, so nothing can be applied unchecked.
	useEffect(() => {
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
			.then((handle) => {
				if (live) session.current = opened = handle
				else handle.close()
			})
			.catch((error) => live && dispatch({ type: 'closed', why: error.message ?? String(error) }))
		return () => {
			live = false
			opened?.close()
			session.current = null
		}
	}, [client, attempt, onActivity, onEvent])

	const send = (act) => {
		try {
			if (!session.current) throw new Error('The configuration session is not open.')
			act(session.current)
			return true
		} catch (error) {
			dispatch({ type: 'closed', why: error.message ?? String(error) })
			return false
		}
	}

	// Edits the device does not hold are lost on leaving, so leaving asks, and the browser warns before
	// the page goes (NSCR).
	const lost = unapplied(state)
	useEffect(() => {
		if (!lost) return
		const warn = (event) => {
			event.preventDefault()
			event.returnValue = ''
		}
		window.addEventListener('beforeunload', warn)
		return () => window.removeEventListener('beforeunload', warn)
	}, [lost])

	const change = (edit) => dispatch({ type: 'edit', change: (held) => fitted(edit(held), state.capabilities) })
	const readOnly = !writable(state)
	const caps = state.capabilities

	function propose(edit) {
		const document = edit.document
		const problem = validate(document) ?? check(document, caps)
		if (problem) {
			dispatch({ type: 'problem', problem })
			return
		}
		if (send((handle) => handle.propose(document))) dispatch({ type: 'proposed', edit })
	}

	// Only the document that failed is applied again without checking the candidate that failed it,
	// and every other candidate as it was (NSCR).
	function applyUnchecked() {
		const key = uncheckable(state)
		if (key && unedited(state)) propose(checking(state.edit, key, false))
	}

	function restart() {
		dispatch({ type: 'restart' })
		setAttempt((count) => count + 1)
	}

	const title = (
		<div className="heading title">
			<h1>Network</h1>
			<button className="secondary small" onClick={() => (lost ? setLeaving(true) : onBack())}>
				Back
			</button>
		</div>
	)

	if (state.status !== 'open') {
		return (
			<div className="network">
				{title}
				{state.status === 'opening' && (
					<p className="muted">
						<span className="spinner" aria-hidden="true" />
						Asking the device for its network settings.
					</p>
				)}
				{state.status === 'closed' && state.unreachable && (
					<>
						<p className="notice fault">
							{state.why
								? `The device's network settings could not be opened again: ${state.why}`
								: "The device's network settings could not be opened again."}
						</p>
						<button className="secondary" onClick={onDisconnect}>
							Disconnect
						</button>
					</>
				)}
				{state.status === 'closed' && !state.unreachable && (
					<>
						<p className="notice fault">
							{state.why ? `The device stopped answering: ${state.why}` : 'The device ended the session.'}
						</p>
						<button className="secondary" onClick={restart}>
							Start again
						</button>
					</>
				)}
			</div>
		)
	}

	const marks = {
		at: state.stage === 'errored' ? state.failure?.at : state.problem?.at,
		problem: state.stage === 'errored' ? null : state.problem,
	}
	const failure = state.stage === 'errored' && state.failure ? { ...state.failure, refused: state.refused } : null
	const failedKey = failure ? candidateAt(failure.at, state.edit.keys) : null
	const hotspotPath = pathOf(['hotspot'])
	const countryPath = pathOf(['regulatory-domain'])
	const failureElsewhere =
		failure && !failedKey && !within(failure.at, hotspotPath) && !within(failure.at, countryPath)
	// A session another holds is read-only, and offers no scan or survey (NSCR).
	const watching = state.stage === 'watching'
	const scan = watching ? null : {
		busy: state.act?.type === 'scan' && !state.act.failure,
		failure: state.act?.type === 'scan' ? state.act.failure : null,
		points: state.networks,
		start: (name) => {
			if (send((handle) => handle.scan(name))) dispatch({ type: 'act', act: 'scan', interface: name })
		},
	}

	return (
		<div className="network">
			{title}
			{leaving && lost && (
				<div className="bar-state" role="alertdialog" aria-label="Leave the network settings?">
					<p>
						<strong>Leave the network settings?</strong>
					</p>
					<p>The changes you have not applied will be lost.</p>
					<div className="row">
						<button className="danger-fill" onClick={onBack}>
							Leave
						</button>
						<button className="secondary" onClick={() => setLeaving(false)}>
							Stay
						</button>
					</div>
				</div>
			)}
			{state.notice && <p className="notice fault">{state.notice}</p>}

			<Order
				state={state}
				readOnly={readOnly}
				change={change}
				select={(key) => dispatch({ type: 'select', key })}
				failure={failureElsewhere ? failure : null}
			/>

			{state.selected && state.edit.keys.includes(state.selected) && (
				<Candidate
					key={state.selected}
					state={state}
					candidateKey={state.selected}
					readOnly={readOnly}
					change={change}
					marks={marks}
					failure={failedKey === state.selected ? failure : null}
					unchecked={
						uncheckable(state) === state.selected ? { enabled: unedited(state), apply: applyUnchecked } : null
					}
					scan={scan}
					onRemove={() => change((edit) => removeCandidate(edit, state.selected))}
				/>
			)}

			<Hotspot
				state={state}
				joined={joined}
				readOnly={readOnly}
				change={change}
				marks={marks}
				failure={failure && within(failure.at, hotspotPath) ? failure : null}
				survey={
					acts(caps).survey && !watching
						? {
								busy: state.act?.type === 'survey' && !state.act.failure,
								failure: state.act?.type === 'survey' ? state.act.failure : null,
								spectrum: state.spectrum,
								start: (name) => {
									if (send((handle) => handle.survey(name))) dispatch({ type: 'act', act: 'survey', interface: name })
								},
							}
						: null
				}
			/>

			{countries(caps) !== null && (
				<section>
					<h2>Country</h2>
					{failure && within(failure.at, countryPath) && <Failure failure={failure} />}
					<fieldset disabled={readOnly}>
						<CountryField
							value={state.edit.document['regulatory-domain']}
							onChange={(code) => change((edit) => setMember(edit, 'regulatory-domain', code))}
							capabilities={caps}
							marks={marks}
							scan={scan}
						/>
					</fieldset>
				</section>
			)}

			<SessionBar
				state={state}
				count={changes(state.edit, state.inForce, state.inForceKeys)}
				onApply={() => propose(state.edit)}
				onReset={() => dispatch({ type: 'reset' })}
			/>
		</div>
	)
}

/// The state of the session, pinned to the bottom of the viewport: which stage the operator is in,
/// what the device is on, and only the actions the stage allows (NSCR).
function SessionBar({ state, count, onApply, onReset }) {
	const { stage } = state
	let tone = ''
	let said
	let actions = null

	if (stage === 'applying') {
		tone = 'working'
		said = (
			<>
				<span className="spinner" aria-hidden="true" />
				<strong>Applying…</strong> Checking the new settings.
			</>
		)
	} else if (stage === 'watching') {
		said = (
			<>
				<strong>Someone else is changing this device's network.</strong> You can see what they apply, but not change
				anything until they are done.
			</>
		)
	} else {
		if (stage === 'errored') {
			tone = 'failed'
			said = state.refused ? (
				<>
					<strong>Not applied.</strong> The device did not take these settings, and is still on its last ones.
				</>
			) : (
				<>
					<strong>Could not apply.</strong>{' '}
					{count === 0 ? 'The device is on these settings.' : 'The device is on the settings that failed, not on your changes.'}
				</>
			)
		} else if (state.problem) {
			tone = 'failed'
			said = (
				<>
					<strong>Not applied.</strong> Fix the marked field first.
				</>
			)
		} else if (count === 0) {
			said = (
				<>
					<strong>Applied.</strong> The device is on these settings.
				</>
			)
		} else {
			said = (
				<>
					<strong>
						{count} {count === 1 ? 'change' : 'changes'} not applied.
					</strong>{' '}
					The device is still on its current settings.
				</>
			)
		}
		const nothing = stage === 'editing' && count === 0
		actions = (
			<>
				<button onClick={onApply} disabled={nothing}>
					Apply
				</button>
				<button className="secondary" onClick={onReset} disabled={nothing}>
					Reset
				</button>
			</>
		)
	}

	return (
		<section className={`bar-state sticky${tone ? ` ${tone}` : ''}`} data-stage={stage} role="status">
			<p>{said}</p>
			<div className="row">{actions}</div>
		</section>
	)
}

/// The ordering of LINK as a list the operator rearranges, each candidate with the state the device
/// reports of it.
function Order({ state, readOnly, change, select, failure }) {
	const [adding, setAdding] = useState(false)
	const list = useRef(null)
	const dragging = useRef(null)
	const caps = state.capabilities
	const { keys, document } = state.edit
	const kinds = attachmentKinds(caps).filter((kind) => kind !== 'wireless' || !absent(caps, 'wireless', document))

	const move = (from, to) => change((edit) => moveCandidate(edit, from, to))

	const add = (kind) => {
		setAdding(false)
		const key = newKey()
		const candidate = blankCandidate(kind, caps)
		change((edit) => addCandidate(edit, candidate, key))
		select(key)
	}

	// Dragging by the grip: the row under the pointer is where the dragged one goes.
	const onPointerDown = (index) => (event) => {
		if (readOnly) return
		event.currentTarget.setPointerCapture(event.pointerId)
		dragging.current = index
	}
	const onPointerMove = (event) => {
		if (dragging.current === null || !list.current) return
		const rows = [...list.current.children]
		const over = rows.findIndex((row) => {
			const box = row.getBoundingClientRect()
			return event.clientY >= box.top && event.clientY < box.bottom
		})
		if (over !== -1 && over !== dragging.current) {
			move(dragging.current, over)
			dragging.current = over
		}
	}
	const onPointerUp = () => {
		dragging.current = null
	}

	return (
		<section>
			<div className="heading">
				<h2>Connections</h2>
				<button className="secondary small" onClick={() => setAdding(!adding)} disabled={readOnly} aria-expanded={adding}>
					Add
				</button>
			</div>
			<p className="muted hint">
				Tried from the top. The device uses the first one that connects, and moves to another when that changes. Drag to reorder.
			</p>
			{failure && <Failure failure={failure} />}
			{adding && !readOnly && (
				<div className="row adding">
					{kinds.map((kind) => (
						<button key={kind} className="secondary small" onClick={() => add(kind)}>
							{kindName(kind)}
						</button>
					))}
				</div>
			)}
			{adding && absent(caps, 'wireless', document) && (
				<p className="muted absent">{absent(caps, 'wireless', document).sentence}</p>
			)}
			{keys.length === 0 && <p className="muted">Nothing to attach by.</p>}
			<ul className="order" ref={list}>
				{keys.map((key, index) => {
					const candidate = document.attachments[index]
					const observed = stateWording(stateFor(state, key), candidate?.kind)
					const name = candidate?.label || 'Unnamed'
					return (
						<li key={key} className={state.selected === key ? 'selected' : undefined}>
							<button
								type="button"
								className="grip"
								aria-label={`Move ${name}`}
								disabled={readOnly}
								onPointerDown={onPointerDown(index)}
								onPointerMove={onPointerMove}
								onPointerUp={onPointerUp}
								onPointerCancel={onPointerUp}
								onKeyDown={(event) => {
									if (event.key === 'ArrowUp' && index > 0) move(index, index - 1)
									else if (event.key === 'ArrowDown' && index < keys.length - 1) move(index, index + 1)
									else return
									event.preventDefault()
								}}
							>
								⠿
							</button>
							<button
								type="button"
								className="what"
								aria-expanded={state.selected === key}
								onClick={() => select(state.selected === key ? null : key)}
							>
								<span className="name">{name}</span>
								<br />
								<span className="kind">
									{describeCandidate(candidate)}
									{candidate?.verify === false && ', not checked'}
									{candidate?.enabled === false && ', off'}
								</span>
							</button>
							{observed && <span className={`state ${observed.tone}`}>{observed.text}</span>}
						</li>
					)
				})}
			</ul>
		</section>
	)
}

/// One candidate's fields, opened from its row. It is turned off and on again from here, keeping its
/// fields. After its checking failed a proposal, it offers to apply that again without checking it;
/// one not checked says so, and can be checked again (NSCR).
function Candidate({ state, candidateKey, readOnly, change, marks, failure, unchecked, scan, onRemove }) {
	const index = state.edit.keys.indexOf(candidateKey)
	const candidate = state.edit.document.attachments[index]
	const observed = stateFor(state, candidateKey)
	const edit = (update) => change((held) => updateCandidate(held, candidateKey, update))
	const known = ['wireless', 'wired-dynamic', 'wired-static'].includes(candidate.kind)
	const off = candidate.enabled === false

	return (
		<section className="candidate" aria-label={candidate.label || 'Unnamed'}>
			<div className="heading">
				<h2>{candidate.label || 'Unnamed'}</h2>
				<div className="actions">
					<button
						className="secondary small"
						onClick={() => change((held) => turning(held, candidateKey, off))}
						disabled={readOnly}
					>
						{off ? 'Turn on' : 'Turn off'}
					</button>
					<button className="secondary small" onClick={onRemove} disabled={readOnly}>
						Remove
					</button>
				</div>
			</div>
			{off && <p className="muted">Off. Kept, but not used until turned on.</p>}
			{failure && <Failure failure={failure} kind={candidate.kind} />}
			{unchecked && (
				<div className="unchecked">
					{!unchecked.enabled && <p className="muted">Undo your edits first.</p>}
					<button className="secondary small" onClick={unchecked.apply} disabled={!unchecked.enabled}>
						Apply without checking
					</button>
				</div>
			)}
			{!failure && observed?.is === 'unavailable' && observed.reason && <p className="muted">{observed.reason}</p>}
			{candidate.verify === false && (
				<div className="unchecked">
					<p className="muted">Not checked, so it applies even if it cannot connect.</p>
					<button className="secondary small" onClick={() => change((held) => checking(held, candidateKey, true))} disabled={readOnly}>
						Turn checking on
					</button>
				</div>
			)}
			{known ? (
				<fieldset disabled={readOnly}>
					<CandidateFields candidate={candidate} index={index} change={edit} capabilities={state.capabilities} marks={marks} scan={scan} />
				</fieldset>
			) : (
				<p className="muted">This version of the app cannot edit this kind of connection. It is kept as it is.</p>
			)}
		</section>
	)
}

/// The hotspot's fields. It is turned off and on again from here, keeping its fields (NSCR).
function Hotspot({ state, readOnly, change, marks, failure, survey, joined }) {
	const caps = state.capabilities
	const hotspot = state.edit.document.hotspot
	const on = hotspotOn(state.edit.document)
	const why = absent(caps, 'hotspot', state.edit.document)
	const blocked = hotspotBlockedBy(caps, state.edit.document, joined ?? [])

	return (
		<section className="hotspot">
			<div className="heading">
				<h2>Hotspot</h2>
				{on ? (
					<button className="secondary small" onClick={() => change((edit) => turningHotspot(edit, false))} disabled={readOnly}>
						Turn off
					</button>
				) : (
					!why && (
						<button
							className="secondary small"
							onClick={() => change((edit) => (hotspot ? turningHotspot(edit, true) : setMember(edit, 'hotspot', blankHotspot(caps))))}
							disabled={readOnly}
						>
							Turn on
						</button>
					)
				)}
			</div>
			{failure && <Failure failure={failure} />}
			{blocked && (
				<p className="notice">
					Can't run beside {blocked.ssid} on {bandName(blocked.band)} channel {blocked.channel}; turning that connection off lets it run.
				</p>
			)}
			{why && <p className="muted absent">{why.sentence}</p>}
			{hotspot && !on && <p className="muted">Off. Kept, but not used until turned on.</p>}
			{!hotspot && !why && <p className="muted">Off.</p>}
			{hotspot && (
				<fieldset disabled={readOnly}>
					<HotspotFields
						document={state.edit.document}
						change={(update) => change((edit) => setMember(edit, 'hotspot', update(edit.document.hotspot)))}
						capabilities={caps}
						marks={marks}
						survey={survey}
					/>
				</fieldset>
			)}
		</section>
	)
}

/// A failure or a refusal: how far it got, as ticks and a cross, and the device's reason as it wrote it.
function Failure({ failure, kind }) {
	const stages = stagesOf(kind, failure.reached)
	return (
		<>
			<p className="notice fault">{failureHeadline(failure.reached, kind, failure.refused)}</p>
			{stages.length > 0 && (
				<p className="stages">
					{stages.map((stage) => (
						<span key={stage.stage} className={stage.mark}>
							{stage.label}
						</span>
					))}
				</p>
			)}
			<p className="why reason">{failure.reason}</p>
		</>
	)
}
