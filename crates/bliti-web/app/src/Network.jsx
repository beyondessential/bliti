// The network configuration screen of NSCR: the ordering, one candidate's fields, the hotspot and
// the country, edited here and proposed only when the operator applies, with the state of the session
// pinned in view.

import { useEffect, useReducer, useRef, useState } from 'react'

import { absent, acts, attachmentKinds, check, countries, hotspotBlockedBy, wpsMethods, wpsRadios } from './capabilities.js'
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
	SelectField,
	adapterOptions,
	blankCandidate,
	blankHotspot,
	fitted,
	kindName,
} from './NetworkFields.jsx'
import { pathOf, within } from './path.js'
import { bandName } from './wireless.js'
import { loadProtocol } from './protocol.js'

export default function Network({ client, onActivity, onEvent, onBack, onStage, onDisconnect, joined }) {
	const [state, dispatch] = useReducer(reduce, undefined, opening)
	const [attempt, setAttempt] = useState(0)
	const session = useRef(null)

	// The session lasts as long as this is mounted: unmounting closes the stream, which the device
	// reads as abandoning whatever was not confirmed (CFG). The application keeps it mounted while a
	// proposal runs, so the operator can leave the screen and come back to it (NSCR). It opens once the
	// checker a document is held to before it is proposed is loaded, so nothing can be applied unchecked.
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

	// Joining by WPS, for the network `ssid` names where one is picked from a scan, on the adapter
	// `name` names or on one the device picks.
	const joinByWps = (method, ssid, name) => {
		if (send((handle) => handle.wps(method, name, ssid))) dispatch({ type: 'wps', method, ssid })
	}

	// Where the session stands, and what can be done about it from elsewhere in the application.
	const edits = state.status === 'open' ? changes(state.edit, state.inForce, state.inForceKeys) : 0
	useEffect(() => {
		onStage?.(
			state.status === 'open'
				? {
						stage: state.stage,
						confirming: !!state.confirming,
						changes: edits,
						confirm,
						cancel,
						reset: () => dispatch({ type: 'reset' }),
					}
				: null,
		)
	}, [state.status, state.stage, state.confirming, edits])
	useEffect(() => () => onStage?.(null), [])

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

	function cancel() {
		if (send((handle) => handle.discard())) dispatch({ type: 'cancelled' })
	}

	function confirm() {
		if (send((handle) => handle.confirm())) dispatch({ type: 'confirming' })
	}

	function restart() {
		dispatch({ type: 'restart' })
		setAttempt((count) => count + 1)
	}

	const title = (
		<div className="heading title">
			<h1>Network</h1>
			<button className="secondary small" onClick={onBack}>
				Back
			</button>
		</div>
	)

	if (state.status === 'busy') {
		return (
			<div className="network">
				{title}
				<p className="notice">Someone else is changing this device's network. Try again once they are done.</p>
				<button className="secondary" onClick={restart}>
					Try again
				</button>
			</div>
		)
	}

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
	const failure = state.stage === 'errored' ? state.failure : null
	const failedKey = failure ? candidateAt(failure.at, state.edit.keys) : null
	const hotspotPath = pathOf(['hotspot'])
	const countryPath = pathOf(['regulatory-domain'])
	const failureElsewhere =
		failure && !failedKey && !within(failure.at, hotspotPath) && !within(failure.at, countryPath)

	return (
		<div className="network">
			{title}
			{state.notice && <p className="notice fault">{state.notice}</p>}

			<Order
				state={state}
				readOnly={readOnly}
				change={change}
				select={(key) => dispatch({ type: 'select', key })}
				failure={failureElsewhere ? failure : null}
				wpsFailure={state.act?.type === 'wps' ? state.act : null}
				joinByWps={joinByWps}
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
					scan={{
						busy: state.act?.type === 'scan' && !state.act.failure,
						failure: state.act?.type === 'scan' ? state.act.failure : null,
						points: state.networks,
						start: (name) => {
							if (send((handle) => handle.scan(name))) dispatch({ type: 'act', act: 'scan', interface: name })
						},
						joinByWps,
					}}
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
					acts(caps).survey
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
						/>
					</fieldset>
					<p className="muted hint">Sets which channels the radio may use.</p>
				</section>
			)}

			<SessionBar
				state={state}
				count={changes(state.edit, state.inForce, state.inForceKeys)}
				onApply={() => propose(state.edit)}
				onReset={() => dispatch({ type: 'reset' })}
				onCancel={cancel}
				onConfirm={confirm}
			/>
		</div>
	)
}

/// The state of the session, pinned to the bottom of the viewport: which stage the operator is in,
/// whether what the device runs is saved, what leaving would cost, and only the actions the stage
/// allows (NSCR).
function SessionBar({ state, count, onApply, onReset, onCancel, onConfirm }) {
	const { stage } = state
	let tone = ''
	let said
	let actions

	if (stage === 'applying') {
		tone = 'working'
		said = state.wps ? (
			<>
				<strong>{state.wpsNetwork ? `Joining ${state.wpsNetwork} by WPS.` : 'Joining by WPS.'}</strong>{' '}
				{state.wps !== 'pin' ? (
					'Press the WPS button on the access point.'
				) : state.pin ? (
					<>
						Enter <span className="pin">{state.pin}</span> on the access point.
					</>
				) : (
					'Waiting for the PIN.'
				)}
			</>
		) : (
			<>
				<span className="spinner" aria-hidden="true" />
				<strong>Applying.</strong> Checking the new settings.
			</>
		)
		actions = (
			<button className="secondary" onClick={onCancel}>
				Cancel
			</button>
		)
	} else if (stage === 'applied') {
		said = state.confirming ? (
			<>
				<strong>Saving.</strong>
			</>
		) : (
			<>
				<strong>Applied, not saved.</strong> Running now. Discarded if you disconnect.
			</>
		)
		actions = (
			<>
				<button onClick={onConfirm} disabled={state.confirming}>
					Confirm
				</button>
				<button className="secondary" onClick={onCancel} disabled={state.confirming}>
					Cancel
				</button>
			</>
		)
	} else {
		if (stage === 'errored') {
			tone = 'failed'
			said = (
				<>
					<strong>Could not apply.</strong> The device is back on the last saved configuration.
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
					<strong>Saved.</strong> The device is on its saved configuration.
				</>
			)
		} else {
			said = (
				<>
					<strong>
						{count} {count === 1 ? 'change' : 'changes'} not applied.
					</strong>{' '}
					The device is still on its saved configuration.
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

/// The session's state on the device view, while the operator is away from the screen with edits not
/// applied, a proposal being applied or applied, or one that failed there (NSCR).
export function HeldBar({ held, onReview }) {
	const discard = (
		<button className="secondary" onClick={held.reset}>
			Discard
		</button>
	)
	const review = (
		<button className="secondary" onClick={onReview}>
			Review
		</button>
	)
	if (held.stage === 'applying') {
		return (
			<section className="bar-state working" role="status">
				<p>
					<span className="spinner" aria-hidden="true" />
					<strong>Applying network settings.</strong>
				</p>
				<div className="row">{review}</div>
			</section>
		)
	}
	if (held.stage === 'applied') {
		return (
			<section className="bar-state" role="status">
				<p>
					<strong>{held.confirming ? 'Saving network settings.' : 'Network settings applied, not saved.'}</strong>
					{!held.confirming && ' Discarded if you disconnect.'}
				</p>
				<div className="row">
					<button onClick={held.confirm} disabled={held.confirming}>
						Confirm
					</button>
					<button className="secondary" onClick={held.cancel} disabled={held.confirming}>
						Discard
					</button>
					{review}
				</div>
			</section>
		)
	}
	if (held.stage === 'errored') {
		return (
			<section className="bar-state failed" role="status">
				<p>
					<strong>Could not apply the network settings.</strong> The device is back on its saved configuration.
				</p>
				<div className="row">
					{review}
					{discard}
				</div>
			</section>
		)
	}
	return (
		<section className="bar-state" role="status">
			<p>
				<strong>
					{held.changes} network {held.changes === 1 ? 'change' : 'changes'} not applied.
				</strong>{' '}
				Nobody else can change the network meanwhile.
			</p>
			<div className="row">
				{review}
				{discard}
			</div>
		</section>
	)
}

/// The ordering of LINK as a list the operator rearranges, each candidate with the state the device
/// reports of it.
function Order({ state, readOnly, change, select, failure, wpsFailure, joinByWps }) {
	const [adding, setAdding] = useState(false)
	const [picked, setPicked] = useState('')
	const list = useRef(null)
	const dragging = useRef(null)
	const caps = state.capabilities
	const { keys, document } = state.edit
	const kinds = attachmentKinds(caps).filter((kind) => kind !== 'wireless' || !absent(caps, 'wireless', document))
	const joiners = kinds.includes('wireless') && acts(caps).wps.length > 0 ? wpsRadios(caps) : []
	const wpsWith = joiners.includes(picked) ? picked : ''
	const wps = joiners.length > 0 ? wpsMethods(caps, wpsWith || undefined) : []

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
			{wpsFailure && (
				<>
					<p className="notice fault">
						{wpsFailure.network ? `Could not join ${wpsFailure.network} by WPS.` : 'Could not join by WPS.'}
					</p>
					<p className="why reason">{wpsFailure.failure.reason}</p>
				</>
			)}
			{adding && !readOnly && (
				<div className="row adding">
					{kinds.map((kind) => (
						<button key={kind} className="secondary small" onClick={() => add(kind)}>
							{kindName(kind)}
						</button>
					))}
					{wps.map((method) => (
						<button
							key={method}
							className="secondary small"
							onClick={() => {
								setAdding(false)
								joinByWps(method, undefined, wpsWith || undefined)
							}}
						>
							{method === 'pin' ? 'WPS PIN' : 'WPS button'}
						</button>
					))}
				</div>
			)}
			{adding && !readOnly && joiners.length > 1 && (
				<div className="adding">
					<SelectField
						label="WPS with"
						value={wpsWith}
						options={adapterOptions(caps, joiners, 'Device chooses')}
						onChange={setPicked}
						marks={{}}
					/>
				</div>
			)}
			{adding && absent(caps, 'wireless', document) && (
				<p className="muted absent">{absent(caps, 'wireless', document).sentence}</p>
			)}
			{keys.length === 0 && <p className="muted">Nothing to attach by.</p>}
			<ul className="order" ref={list}>
				{keys.map((key, index) => {
					const candidate = document.attachments[index]
					// The states held are the last configuration's, and the device holds them back while it
					// checks a new one, so they are left unsaid until it answers (CFG).
					const observed = state.stage === 'applying' ? null : stateWording(stateFor(state, key), candidate?.kind)
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

function Hotspot({ state, readOnly, change, marks, failure, survey, joined }) {
	const caps = state.capabilities
	const hotspot = state.edit.document.hotspot
	const why = absent(caps, 'hotspot', state.edit.document)
	const blocked = hotspot ? hotspotBlockedBy(caps, state.edit.document, joined ?? []) : null

	return (
		<section className="hotspot">
			<div className="heading">
				<h2>Hotspot</h2>
				{hotspot ? (
					<button className="secondary small" onClick={() => change((edit) => setMember(edit, 'hotspot', undefined))} disabled={readOnly}>
						Turn off
					</button>
				) : (
					!why && (
						<button className="secondary small" onClick={() => change((edit) => setMember(edit, 'hotspot', blankHotspot(caps)))} disabled={readOnly}>
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

/// A failed proposal: how far it got, as ticks and a cross, and the device's reason as it wrote it.
function Failure({ failure, kind }) {
	const stages = stagesOf(kind, failure.reached)
	return (
		<>
			<p className="notice fault">{failureHeadline(failure.reached, kind)}</p>
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
