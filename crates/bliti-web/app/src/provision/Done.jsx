// A run that has finished: the board's record, and what to do next (PWEB, "When the run has
// finished").

import { useState } from 'react'

import { Steps } from '../Reaching.jsx'
import { REACHING } from './Finding.jsx'
import { ordered } from './labels.js'
import { download, downloadText, useSheetReady } from './sheet.js'
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
	const ready = useSheetReady()
	const [failed, setFailed] = useState(null)
	const waiting = ordered(labels.list).filter((label) => label.printed === null)

	// The sheet of the labels not yet printed, which are marked printed once it is downloaded (PSHT).
	async function save() {
		setFailed(null)
		try {
			await download(waiting)
			labels.markPrinted(waiting.map((label) => label.payload))
		} catch (error) {
			setFailed(error.message ?? String(error))
		}
	}
	return (
		<>
			<Steps names={REACHING} at={3} />
			<section>
				<p className="done-line">Provisioned</p>
				<p className="lead">{record?.suffix ?? suffix} is ready</p>
				{/* spec: PWEB */}
				<p className="muted">Power it off and remove the USB stick.</p>
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
				{failed && (
					<p className="why" role="alert">
						{failed}
					</p>
				)}
				{waiting.length > 0 && (
					<button className="block go" onClick={save}>
						{downloadText(waiting.length, ready)}
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
							{waiting.length === 0 && (
								<button className="block go" onClick={onAnother}>
									Provision another Iti
								</button>
							)}
							<button className="link" onClick={onAgain}>
								Run steps again
							</button>
							{waiting.length > 0 && (
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
