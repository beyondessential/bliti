// A run that has finished: the board's record, and what to do next (PWEB, "When the run has
// finished").

import { useState } from 'react'

import { Steps } from '../Reaching.jsx'
import { REACHING } from './Finding.jsx'
import { ordered } from './labels.js'
import { SheetButton } from './Labels.jsx'
import { useSheet } from './sheet.js'
import { APPROVAL, awaitsApproval, labelOf, recordOf, showSaved } from './state.js'

/// Copying the passphrase, and saying whether it worked.
function Copy({ text }) {
	const [copied, setCopied] = useState(null)
	async function copy() {
		try {
			await navigator.clipboard.writeText(text)
			setCopied(true)
		} catch {
			setCopied(false)
		}
	}
	return (
		<>
			<button className="link" onClick={copy}>
				Copy
			</button>
			{copied !== null && (
				<span className="muted" role="status">
					{copied ? 'Copied' : "Couldn't copy"}
				</span>
			)}
		</>
	)
}

/// `suffix` names the device, for a run whose steps report no record. `live` is whether the device can
/// be acted on, which the buttons need. The page never sends `acknowledge`: the device holds the
/// finished run until it is powered off, so the record can be had again (PWEB).
export default function Done({ state, suffix, live, labels, onAnother, onAgain, onLabels }) {
	const record = recordOf(state.steps)
	const sheet = useSheet()
	const waiting = ordered(labels.list).filter((label) => label.printed === null)
	const name = record?.suffix ?? suffix
	const provisioned = state.provisioned === true
	// The primary action (PWEB, PSHT): the sheet where the board is provisioned and a label waits to be
	// printed, running steps again where it is not provisioned, and otherwise the next board.
	const primary = !provisioned ? 'again' : waiting.length > 0 ? 'sheet' : 'another'

	const markPrinted = (printed) => labels.markPrinted(printed.map((label) => label.payload))
	return (
		<>
			<Steps names={REACHING} at={3} />
			<section>
				{/* spec: PWEB */}
				{provisioned ? (
					<>
						<p className="done-line">Provisioned</p>
						<p className="lead">{name} is ready</p>
						<p className="muted">Power it off and remove the USB stick.</p>
					</>
				) : (
					<>
						<p className="lead">Run finished</p>
						<p className="muted">{name} is not provisioned yet.</p>
					</>
				)}
				{/* spec: PWEB */}
				{awaitsApproval(state.steps) && <p className="muted">{APPROVAL}</p>}
				{record && (
					<div className="setup">
						<div className="details first">
							<dl>
								<dt>Code ending</dt>
								<dd>{record.suffix}</dd>
								{record.version && (
									<>
										<dt>Version</dt>
										<dd>{record.version}</dd>
									</>
								)}
								{record.disk && (
									<>
										<dt>Disk</dt>
										<dd className="mono">{record.disk}</dd>
									</>
								)}
								<dt>Saved</dt>
								<dd>{showSaved(record.at)}</dd>
								{record.passphrase && (
									<>
										<dt>Recovery passphrase</dt>
										<dd className="secret">
											<span className="passphrase">{record.passphrase}</span>
											<Copy text={record.passphrase} />
										</dd>
									</>
								)}
							</dl>
							<p className="muted hint">Also saved to the USB stick.</p>
						</div>
					</div>
				)}
				{/* spec: PSHT */}
				{labelOf(state.steps) && <p className="muted">Its label is in your list.</p>}
				{sheet.failed && (
					<p className="why" role="alert">
						{sheet.failed}
					</p>
				)}
				{live && primary === 'again' && (
					<button className="block go" onClick={onAgain}>
						Run steps again
					</button>
				)}
				{waiting.length > 0 && (
					<SheetButton
						sheet={sheet}
						labels={waiting}
						className={primary === 'sheet' ? 'block go' : 'block secondary go'}
						onMade={markPrinted}
					/>
				)}
				{live && primary === 'another' && (
					<button className="block go" onClick={onAnother}>
						Provision another Iti
					</button>
				)}
				<div className="links">
					{labels.list.labels.length > 0 && (
						<button className="link" onClick={onLabels}>
							Labels ({labels.unprinted} not printed)
						</button>
					)}
					{live && (
						<>
							{primary !== 'again' && (
								<button className="link" onClick={onAgain}>
									Run steps again
								</button>
							)}
							{primary !== 'another' && (
								<button className="link" onClick={onAnother}>
									Provision another Iti
								</button>
							)}
						</>
					)}
				</div>
			</section>
		</>
	)
}
