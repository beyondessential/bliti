// The power and battery screen of CSCR: the acts of CTL the device offers, its batteries as the
// facts and readings of NFO give them, and its battery curve (CRV), each act, import and reset
// confirmed before it is asked for.

import { useEffect, useRef, useState } from 'react'

import { batteryName, face, headline, tone } from './Readings.jsx'
import { formatDuration, isTrouble, labelOf, reasonOf, scaleOf } from './readings.js'

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
		question: 'Reboot the Iti?',
		warning: 'The Iti will be unavailable while it reboots.',
	},
	{
		act: 'power-off',
		name: 'Power off',
		hint: 'Stays off until turned on at the device.',
		button: 'Power off',
		question: 'Power off the Iti?',
		warning: 'It stays off until it is turned on at the device.',
		danger: true,
	},
]

export default function Power({ client, entries, onActivity, onEvent, onAccepted }) {
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
					} else if (message.type === 'cannot-act') {
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

	return (
		<>
			{offered.length > 0 && (
				<section>
					<h2>Power</h2>
					<ul className="acts">
						{offered.map((each) => (
							<li key={each.act} className={each.act === confirming ? 'asking' : undefined}>
								<div>
									<span className="name">{each.name}</span>
									<span className="muted">{each.hint}</span>
								</div>
								{each.act === confirming ? (
									<div className="bar-state" role="alertdialog" aria-label={each.question}>
										<p>
											<strong>{each.question}</strong>
										</p>
										{each.warning && <p>{each.warning}</p>}
										<div className="row fill">
											<button className={each.danger ? 'danger-fill' : undefined} onClick={() => ask(each.act)}>
												{each.button}
											</button>
											<button className="secondary" onClick={() => setConfirming(null)}>
												Cancel
											</button>
										</div>
									</div>
								) : (
									<button
										className={`secondary${each.danger ? ' danger' : ''}`}
										onClick={() => setConfirming(each.act)}
										disabled={asking !== null}
									>
										{each.button}
									</button>
								)}
							</li>
						))}
					</ul>
					{refused && <p className="why reason">{refused}</p>}
				</section>
			)}
			<Battery client={client} entries={entries} onActivity={onActivity} onEvent={onEvent} />
		</>
	)
}

/// The name an exported curve document is saved under.
const EXPORTED = 'battery-curve.json'

/// The battery the device manages a backup supply for, and learns a curve of (NFO).
const BUILT_IN = 'built-in'

/// The battery section: a block for each battery the device reports a charge for, and beneath the
/// built-in one what the device gives for a full charge and a full recharge, and its curve to export,
/// import and reset (CSCR). Left out where the device reports no charge.
function Battery({ client, entries, onActivity, onEvent }) {
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
					} else if (message.type === 'cannot-configure' || message.type === 'cannot-act') {
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

	// By name, with the built-in battery last so its curve sits beside it (CSCR).
	const charges = entries
		.filter((entry) => entry.name === 'battery-charge')
		.sort(
			(a, b) =>
				(batteryName(a) === BUILT_IN) - (batteryName(b) === BUILT_IN) ||
				batteryName(a).localeCompare(batteryName(b)),
		)
	if (charges.length === 0) return null

	const source = entries.find((entry) => entry.name === 'power-source')
	const several = charges.length > 1
	// A device without a built-in battery has no curve to offer, whatever it answers (CSCR).
	const curved = curves?.document && charges.some((entry) => batteryName(entry) === BUILT_IN)

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
		curves?.lasts && `A full charge lasts ${span(curves.lasts)}.`,
		curves?.recharge && `A full recharge takes ${span(curves.recharge)}.`,
	].filter(Boolean)

	return (
		<section>
			<h2>Battery</h2>
			{several && source && (
				<dl className="facts source">
					<Fact entry={source} />
				</dl>
			)}
			{charges.map((charge) => (
				<Cell
					key={batteryName(charge)}
					charge={charge}
					entries={entries}
					named={several}
					source={several ? null : source}
				/>
			))}
			{curved && (
				<div className="curve">
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
				</div>
			)}
			<input ref={picker} type="file" accept=".json,application/json" hidden onChange={picked} />
			{refused && <p className="why reason">{refused}</p>}
		</section>
	)
}

/// One battery: its charge headlined with its direction beside it and its scale beneath, the reason
/// of either where there is one, then its times and voltage, each paired with the charge by the
/// battery's name. Named only where the device reports several (CSCR).
function Cell({ charge, entries, named, source }) {
	const own = (name) => entries.find((entry) => entry.name === name && batteryName(entry) === batteryName(charge))
	const direction = own('battery-direction')
	const scale = scaleOf(charge)
	const facts = [source, own('battery-time-to-empty'), own('battery-time-to-full'), own('battery-voltage')].filter(
		Boolean,
	)

	return (
		<div className="battery">
			{named && <span className="which">{batteryName(charge)}</span>}
			<div className="charge">
				<span className={`value ${tone(charge)}`}>{face(charge)}</span>
				{direction && <span className={`way ${tone(direction)}`}>{headline(direction)}</span>}
			</div>
			{scale !== null && (
				<div className={`bar${isTrouble(charge) ? ' warn' : ''}`}>
					<span style={{ width: `${scale * 100}%` }} />
				</div>
			)}
			{[charge, direction].filter((entry) => entry && reasonOf(entry)).map((entry) => (
				<p key={entry.name} className={`charge-reason ${tone(entry)}`}>
					{reasonOf(entry)}
				</p>
			))}
			{facts.length > 0 && (
				<dl className="facts">
					{facts.map((entry) => (
						<Fact key={entry.name} entry={entry} />
					))}
				</dl>
			)}
		</div>
	)
}

/// A labelled figure of a battery, coloured by its status, with its reason beneath where it has one.
function Fact({ entry }) {
	return (
		<>
			<dt>{labelOf(entry.name)}</dt>
			<dd className={tone(entry) || undefined}>
				{face(entry)}
				{reasonOf(entry) && <span className="reason">{reasonOf(entry)}</span>}
			</dd>
		</>
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
