// The Control screen of CSCR: the way into the network screen, the acts of CTL the device offers,
// and its battery curve (CRV), each act, import and reset confirmed before it is asked for.

import { useEffect, useRef, useState } from 'react'

import { formatDuration } from './readings.js'

/// The acts this build knows, in the order it offers them, and what it calls each (CSCR).
export const ACTS = [
	{
		act: 'restart',
		name: 'Restart bliti',
		hint: 'Only the provisioning service.',
		button: 'Restart',
		question: 'Restart bliti?',
	},
	{
		act: 'reboot',
		name: 'Reboot',
		hint: 'The whole device.',
		button: 'Reboot',
		question: 'Reboot the device?',
	},
	{
		act: 'power-off',
		name: 'Power off',
		hint: 'Stays off until turned on at the device.',
		button: 'Power off',
		question: 'Power off the device?',
		warning: 'It stays off until it is turned on at the device.',
		danger: true,
	},
]

export default function Control({ client, onNetwork, onActivity, onEvent, onAccepted, provisional, unapplied }) {
	// The acts the device listed that this build knows, or null until it has listed them. A device older
	// than this build never answers, and looks like one offering nothing (CSCR).
	const [acts, setActs] = useState(null)
	const [confirming, setConfirming] = useState(null)
	const [asking, setAsking] = useState(null)
	const [refused, setRefused] = useState(null)
	const stream = useRef(null)
	const asked = useRef(null)

	// The stream lasts as long as this is mounted (CSCR).
	useEffect(() => {
		let live = true
		let opened = null
		client
			.power({
				onEvent: (event) => {
					if (!live) return
					onEvent?.(event)
					if (event.kind !== 'message') return
					const message = event.message
					if (message.type === 'acts') {
						setActs(ACTS.filter((each) => message.acts.includes(each.act)))
					} else if (message.type === 'accepted' && asked.current) {
						onAccepted?.(asked.current)
					} else if (message.type === 'refused') {
						asked.current = null
						setAsking(null)
						setRefused(message.reason)
					}
				},
				onClosed: () => {
					if (!live) return
					stream.current = null
					setActs(null)
				},
				onActivity,
			})
			.then((handle) => {
				if (live) stream.current = opened = handle
				else handle.close()
			})
			.catch((error) => live && onActivity?.('note', `could not open a power stream: ${error.message ?? error}`))
		return () => {
			live = false
			opened?.close()
			stream.current = null
		}
	}, [client, onActivity, onEvent, onAccepted])

	function ask(act) {
		setConfirming(null)
		setRefused(null)
		try {
			stream.current?.act(act)
			asked.current = act
			setAsking(act)
		} catch (error) {
			setRefused(error.message ?? String(error))
		}
	}

	const offered = acts ?? []
	const pending = offered.find((each) => each.act === confirming)

	return (
		<>
			<section>
				<h2>Network</h2>
				<button className="secondary" onClick={onNetwork}>
					Network settings
				</button>
			</section>
			{offered.length > 0 && (
				<section>
					<h2>Power</h2>
					{pending ? (
						<div className="bar-state" role="alertdialog" aria-label={pending.question}>
							<p>
								<strong>{pending.question}</strong>
							</p>
							{pending.warning && <p>{pending.warning}</p>}
							{provisional && <p>The network settings it is trying are not saved, and will be lost.</p>}
							{unapplied && <p>The network changes not applied will be lost.</p>}
							<div className="row">
								<button className={pending.danger ? 'danger-fill' : undefined} onClick={() => ask(pending.act)}>
									{pending.button}
								</button>
								<button className="secondary" onClick={() => setConfirming(null)}>
									Cancel
								</button>
							</div>
						</div>
					) : (
						<ul className="acts">
							{offered.map((each) => (
								<li key={each.act}>
									<div>
										<span className="name">{each.name}</span>
										<span className="muted">{each.hint}</span>
									</div>
									<button
										className={`secondary${each.danger ? ' danger' : ''}`}
										onClick={() => setConfirming(each.act)}
										disabled={asking !== null}
									>
										{each.button}
									</button>
								</li>
							))}
						</ul>
					)}
					{refused && <p className="why reason">{refused}</p>}
				</section>
			)}
			<Battery client={client} onActivity={onActivity} onEvent={onEvent} />
		</>
	)
}

/// The name an exported curve document is saved under.
const EXPORTED = 'battery-curve.json'

/// The battery section: what the device gives for a full charge and a full recharge, and its curve
/// to export, import and reset (CSCR). Left out until the device answers `curve` with a document,
/// so a device older than this build, or one managing no backup supply, looks like one with nothing
/// to offer here.
function Battery({ client, onActivity, onEvent }) {
	const [curves, setCurves] = useState(null)
	// The import or reset being confirmed: `{ reset: true }`, or the file picked and what it held.
	const [confirming, setConfirming] = useState(null)
	const [asking, setAsking] = useState(false)
	const [refused, setRefused] = useState(null)
	const stream = useRef(null)
	const picker = useRef(null)

	// The stream lasts as long as the screen is open, as the power stream does (CSCR).
	useEffect(() => {
		let live = true
		let opened = null
		client
			.curve({
				onEvent: (event) => {
					if (!live) return
					onEvent?.(event)
					if (event.kind !== 'message') return
					const message = event.message
					if (message.type === 'curves') {
						setCurves(message)
					} else if (message.type === 'accepted') {
						setAsking(false)
					} else if (message.type === 'refused') {
						setAsking(false)
						setRefused(message.reason)
					}
				},
				onClosed: () => {
					if (!live) return
					stream.current = null
					setCurves(null)
				},
				onActivity,
			})
			.then((handle) => {
				if (live) stream.current = opened = handle
				else handle.close()
			})
			.catch((error) => live && onActivity?.('note', `could not open a curve stream: ${error.message ?? error}`))
		return () => {
			live = false
			opened?.close()
			stream.current = null
		}
	}, [client, onActivity, onEvent])

	if (!curves?.document) return null

	function confirm(what) {
		setRefused(null)
		setConfirming(what)
	}

	function ask() {
		const what = confirming
		setConfirming(null)
		setRefused(null)
		try {
			if (what.reset) stream.current?.reset()
			else stream.current?.load(what.document)
			setAsking(true)
		} catch (error) {
			setRefused(error.message ?? String(error))
		}
	}

	async function picked(event) {
		const file = event.target.files?.[0]
		// Cleared so picking the same file again is still a change.
		event.target.value = ''
		if (!file) return
		try {
			confirm({ name: file.name, document: JSON.parse(await file.text()) })
		} catch {
			setRefused(`${file.name} is not JSON.`)
		}
	}

	const figures = [
		curves.lasts && `A full charge lasts ${span(curves.lasts)}.`,
		curves.recharge && `A full recharge takes ${span(curves.recharge)}.`,
	].filter(Boolean)

	return (
		<section>
			<h2>Battery</h2>
			{confirming ? (
				<Confirmation what={confirming} onConfirm={ask} onCancel={() => setConfirming(null)} />
			) : (
				<>
					{figures.length > 0 && (
						<p className="muted learnt">
							{figures.map((line, index) => (
								<span key={line}>
									{index > 0 && <br />}
									{line}
								</span>
							))}
						</p>
					)}
					<p className="muted learnt">The device learns its battery curve over time, which improves these estimates.</p>
					<ul className="acts">
						<li>
							<div>
								<span className="name">Export curve</span>
								<span className="muted">To analyse, or copy to another device.</span>
							</div>
							<button className="secondary" onClick={() => download(curves.document)}>
								Export
							</button>
						</li>
						<li>
							<div>
								<span className="name">Import curve</span>
								<span className="muted">Start from one learnt on another device.</span>
							</div>
							<button className="secondary" onClick={() => picker.current?.click()} disabled={asking}>
								Import
							</button>
						</li>
						<li>
							<div>
								<span className="name">Reset curve</span>
								<span className="muted">Do this after changing the battery cell.</span>
							</div>
							<button className="secondary danger" onClick={() => confirm({ reset: true })} disabled={asking}>
								Reset
							</button>
						</li>
					</ul>
				</>
			)}
			<input ref={picker} type="file" accept=".json,application/json" hidden onChange={picked} />
			{refused && <p className="why reason">{refused}</p>}
		</section>
	)
}

/// Asking the operator to confirm an import or a reset, in place of the rows (CSCR).
function Confirmation({ what, onConfirm, onCancel }) {
	const question = what.reset ? 'Reset the charge curve?' : `Import ${what.name}?`
	return (
		<div className="bar-state" role="alertdialog" aria-label={question}>
			<p>
				<strong>{question}</strong>
			</p>
			<p>{what.reset ? 'What the device has learnt is discarded.' : 'It replaces what the device has learnt.'}</p>
			<div className="row">
				<button className={what.reset ? 'danger-fill' : undefined} onClick={onConfirm}>
					{what.reset ? 'Reset' : 'Import'}
				</button>
				<button className="secondary" onClick={onCancel}>
					Cancel
				</button>
			</div>
		</div>
	)
}

/// A time the device gives, with how far either way it may be off.
function span({ duration, margin }) {
	return `${formatDuration(duration)} (±${formatDuration(margin)})`
}

/// Save the curve document in force as a file, as it came off the wire.
function download(document) {
	const blob = new Blob([`${JSON.stringify(document, null, '\t')}\n`], { type: 'application/json' })
	const url = URL.createObjectURL(blob)
	const link = window.document.createElement('a')
	link.href = url
	link.download = EXPORTED
	link.click()
	setTimeout(() => URL.revokeObjectURL(url), 0)
}
