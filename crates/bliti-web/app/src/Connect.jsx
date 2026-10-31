// The screens for reaching a device: reading its code, finding it, and connecting, as three steps
// each with one thing to do (WEB).

import { useEffect, useRef, useState } from 'react'

import { CLIENT_VERSION } from './client.js'
import { FAKE_CODE } from './fake/device.js'
import { FakeDevice, Problem, Steps } from './Reaching.jsx'
import { suffix } from './remembered.js'
import { cameraAvailable, scan } from './scanner.js'
import { Title } from './Screen.jsx'

const REACHING = ['Scan code', 'Find Iti', 'Connect']

/// `unsupported` is why the browser cannot open a channel, where it cannot and the fake device is not
/// held, and `dev` whether the
/// application is in dev mode, which offers the fake device. `phase` is null,
/// `choosing` while the chooser is open, or `connecting` once a device is picked. `problem` is why
/// the last attempt to reach the device ended, where it did not end by choice.
export default function Connect({
	unsupported,
	dev,
	client,
	note,
	offlineReady,
	code,
	onCode,
	known,
	recent,
	phase,
	problem,
	onFind,
	onCancel,
	onOtherCode,
}) {
	const [scanning, setScanning] = useState(false)
	const [cameraFailed, setCameraFailed] = useState(false)
	// A code the camera saw that is not one this build reads, said beneath it while it goes on reading.
	const [seen, setSeen] = useState('')
	const [typedError, setTypedError] = useState('')
	const video = useRef(null)
	const scanning_ = useRef(null)

	useEffect(() => () => scanning_.current?.abort(), [])

	// The fake device's code reads to it only in dev mode (WEB).
	const readCode = (text) => client.readCode(text, { fake: dev })

	async function read(text, how) {
		try {
			const found = await readCode(text)
			setTypedError('')
			onCode(found, how)
		} catch (error) {
			const why = error.message ?? String(error)
			setTypedError(why)
			note('note', `code not read: ${why}`)
		}
	}

	async function startScan() {
		const controller = new AbortController()
		scanning_.current = controller
		setScanning(true)
		setCameraFailed(false)
		setSeen('')
		try {
			const found = await scan(video.current, {
				read: readCode,
				onRejected: setSeen,
				signal: controller.signal,
			})
			if (found) onCode(found, 'from the camera')
		} catch (error) {
			setCameraFailed(true)
			note('note', `the camera is not available: ${error.message ?? error}`)
		} finally {
			scanning_.current = null
			setScanning(false)
		}
	}

	const camera = cameraAvailable()
	const fake = <FakeDevice onClick={() => read(FAKE_CODE, 'for the fake device')} />
	const title = (
		<>
			<Title title="Tamanu Iti">{!unsupported && !offlineReady && <span className="offline">Not ready offline</span>}</Title>
			{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}
		</>
	)

	if (unsupported) {
		return (
			<>
				{title}
				<section>
					<Problem>This browser can't connect to an Iti</Problem>
					<p className="muted">{unsupported}</p>
					{dev && fake}
				</section>
			</>
		)
	}

	if (!code) {
		const failed = cameraFailed || !camera
		return (
			<>
				{title}
				<Steps names={REACHING} at={0} />
				{!scanning && recent?.length > 0 && (
					<section>
						<h2>Recently used</h2>
						<ul className="acts recent">
							{recent.map((each) => (
								<li key={each.code}>
									<div>
										{each.hostname && <span className="name">{each.hostname}</span>}
										<span className="muted">Code ending {suffix(each.code)}</span>
									</div>
									<button
										aria-label={`Connect to ${each.hostname ?? `the Iti with code ending ${suffix(each.code)}`}`}
										onClick={() => read(each.code, 'from those used recently')}
									>
										Connect
									</button>
								</li>
							))}
						</ul>
					</section>
				)}
				<section className="steps-body">
					{scanning ? (
						<>
							<p className="lead">Point your camera at the code</p>
							<p className="muted">Hold your phone steady. It scans on its own.</p>
						</>
					) : failed ? (
						<>
							<Problem>Camera not available</Problem>
							<p className="muted">Allow camera access for this page in your browser settings, then try again.</p>
						</>
					) : (
						<>
							<p className="lead">{recent?.length > 0 ? 'Or connect to a different Iti' : 'Scan the code on your Iti'}</p>
							<p className="muted">Find the QR code on the outside of the Iti, then open your camera.</p>
						</>
					)}
					<video ref={video} hidden={!scanning} playsInline muted />
					{scanning && seen && <p className="why">{seen}</p>}
					{scanning ? (
						<button className="secondary block" onClick={() => scanning_.current?.abort()}>
							Cancel
						</button>
					) : (
						camera && (
							<button className="block" onClick={startScan}>
								{cameraFailed ? 'Try again' : 'Open camera'}
							</button>
						)
					)}
					{dev && !scanning && fake}
				</section>
				{!scanning && (
					<details className="advanced">
						<summary>Advanced</summary>
						<div className="inside">
							<TypedCode onRead={(text) => read(text, '')} fault={typedError} />
						</div>
					</details>
				)}
			</>
		)
	}

	const name = known?.hostname && <strong className="device-name">{known.hostname}</strong>
	const chip = <span className="chip">{code.localName}</span>
	const other = (
		<button className="link" onClick={onOtherCode}>
			Scan a different code
		</button>
	)

	if (phase === 'connecting') {
		return (
			<>
				{title}
				<Steps names={REACHING} at={2} />
				<section>
					{name}
					<p className="lead">
						<span className="spinner" aria-hidden="true" />
						Connecting to your Iti
					</p>
					<p className="muted">Keep your phone close to the Iti.</p>
					<button className="secondary block" onClick={onCancel}>
						Cancel
					</button>
				</section>
			</>
		)
	}

	if (problem?.kind === 'not-found' || problem?.kind === 'lost') {
		return (
			<>
				{title}
				<Steps names={REACHING} at={1} />
				<section>
					{name}
					<Problem>{problem.kind === 'lost' ? 'Lost connection to your Iti' : "Your Iti wasn't found"}</Problem>
					<p className="muted">Check it's switched on and nearby, then try again. Look for:</p>
					{chip}
					<button className="block" onClick={onFind} disabled={phase !== null}>
						Try again
					</button>
					{problem.detail && <p className="why">{problem.detail}</p>}
					<div className="links">{other}</div>
				</section>
			</>
		)
	}

	return (
		<>
			{title}
			<Steps names={REACHING} at={1} />
			<section>
				{name}
				<p className="done-line">Code scanned</p>
				{code.fake ? (
					<>
						<p className="lead">Now connect to the fake Iti</p>
						<p className="muted">It has no chooser.</p>
					</>
				) : (
					<>
						<p className="lead">Now find your Iti</p>
						<p className="muted">Tap Find my Iti. It should be the only device that shows up, named:</p>
						{chip}
					</>
				)}
				<button className="block" onClick={onFind} disabled={phase !== null}>
					{code.fake ? 'Connect' : 'Find my Iti'}
				</button>
				{problem?.detail && <p className="why">{problem.detail}</p>}
				<div className="links">
					{other}
					{!code.fake && (
						<button className="link" onClick={() => download(code)}>
							Save code for reprinting
						</button>
					)}
				</div>
			</section>
		</>
	)
}

function TypedCode({ onRead, fault }) {
	const [typed, setTyped] = useState('')
	return (
		<>
			<label htmlFor="code-text">QR code contents</label>
			<input
				id="code-text"
				className={fault ? 'field-fault' : undefined}
				value={typed}
				onChange={(event) => setTyped(event.target.value)}
				onKeyDown={(event) => event.key === 'Enter' && onRead(typed)}
				placeholder="BLITI:..."
				autoComplete="off"
				autoCapitalize="characters"
				spellCheck="false"
			/>
			{fault && <p className="why">{fault}</p>}
			<button onClick={() => onRead(typed)}>Use</button>
		</>
	)
}

/// Save the code as SVG, for printing a replacement. Named after the last four characters of the
/// encoded payload, which are all key fingerprint (WEB).
function download(code) {
	const url = URL.createObjectURL(new Blob([code.svg], { type: 'image/svg+xml' }))
	const link = document.createElement('a')
	link.href = url
	link.download = `bliti-${code.suffix}.svg`
	link.click()
	// Revoked once the browser has had a turn to start the download from it.
	setTimeout(() => URL.revokeObjectURL(url), 0)
}
