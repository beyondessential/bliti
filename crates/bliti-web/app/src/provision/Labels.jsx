// The list of labels, to choose which to print and to remove from (PSHT, "Choosing what to print").

import { useState } from 'react'

import { Title } from '../Screen.jsx'
import { ordered } from './labels.js'
import { downloadText, useSheet } from './sheet.js'
import { showSaved } from './state.js'

/// What the label says of when it was made and printed.
function when(label) {
	return label.printed ? `Printed ${showSaved(label.printed)}` : `Added ${showSaved(label.at)}`
}

/// The button that downloads the sheet of `labels` through `sheet`, saying it is making the sheet
/// while it is (PSHT).
export function SheetButton({ sheet, labels, className, onMade }) {
	const { ready, making, make } = sheet
	// spec: PSHT
	return (
		<button className={className} disabled={making || labels.length === 0} onClick={() => make(labels, onMade)}>
			{making ? (
				<>
					<span className="spinner" aria-hidden="true" />
					Making the sheet
				</>
			) : (
				downloadText(labels.length, ready)
			)}
		</button>
	)
}


/// `labels` is the page's hold on the list, and `onBack` leaves it.
export default function Labels({ labels, onBack }) {
	const sheet = useSheet()
	const held = ordered(labels.list)
	// What the operator has changed, over the ticks the labels' printed state gives.
	const [changed, setChanged] = useState({})
	const isTicked = (label) => changed[label.payload] ?? label.printed === null
	const chosen = held.filter(isTicked)
	// The label being asked about removing.
	const [removing, setRemoving] = useState(null)

	function made(printed) {
		labels.markPrinted(printed.map((label) => label.payload))
		setChanged({})
	}

	function remove(label) {
		if (label.printed === null && removing !== label.payload) {
			setRemoving(label.payload)
			return
		}
		setRemoving(null)
		labels.remove(label.payload)
	}

	// spec: PSHT
	return (
		<>
			<Title title="Labels">
				<button className="secondary small" onClick={onBack}>
					Back
				</button>
			</Title>
			<section>
				{held.length === 0 ? (
					<p className="muted">No labels yet.</p>
				) : (
					<>
						<p className="muted">Unprinted labels are ticked.</p>
						<ul className="labels">
							{held.map((label) => (
								<li key={label.payload}>
									<input
										type="checkbox"
										aria-label={`Print ${label.payload.slice(-4)}`}
										checked={isTicked(label)}
										onChange={(event) =>
											setChanged((now) => ({ ...now, [label.payload]: event.target.checked }))
										}
									/>
									<div className="body">
										<span className="suffix">{label.payload.slice(-4)}</span> &middot; Tamanu Iti v{label.version}
										<span className="detail">{when(label)}</span>
										{removing === label.payload && (
											<span className="detail">Remove this label before it has been printed?</span>
										)}
									</div>
									<button className="link" onClick={() => remove(label)}>
										Remove
									</button>
									{removing === label.payload && (
										<button className="link" onClick={() => setRemoving(null)}>
											Keep
										</button>
									)}
								</li>
							))}
						</ul>
						{sheet.failed && (
							<p className="why" role="alert">
								{sheet.failed}
							</p>
						)}
						<SheetButton sheet={sheet} labels={chosen} className="block" onMade={made} />
					</>
				)}
				{!labels.kept && (
					<p className="muted waiting">This browser can't keep the list. The labels are lost when the page is closed.</p>
				)}
			</section>
		</>
	)
}
