import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { createClient } from './client.js'
import { cameraAvailable, scan } from './scanner.js'

// The topic the diagnostics feature defines. A device that does not serve it sends nothing and does
// not fail, which is what a device older than this application looks like (BLI-MSG, "Subscribing"),
// so subscribing here is correct against both and carries no readings until a device offers them.
const TOPIC = 'system'

// How many notices and activity lines are kept. The far end decides how many arrive.
const KEPT = 20

export default function App() {
	// The seam the harness fakes at: a fake client is fed decoded messages with no wasm and no
	// Bluetooth in the loop. Compiled out of any build but the harness's, because anything able to run
	// a script on this origin before the app mounts could otherwise install its own client and become
	// the device's peer, and the sticker secret is the only credential there is.
	const client = useMemo(() => (__TEST_SEAM__ && window.__blitiClient) || createClient(), [])

	const [unsupported] = useState(() => client.unsupported())
	const [sticker, setSticker] = useState(null)
	const [readError, setReadError] = useState('')
	const [scanning, setScanning] = useState(false)
	const [connecting, setConnecting] = useState(false)
	const [connectStatus, setConnectStatus] = useState('')
	const [connected, setConnected] = useState(false)
	const [device, setDevice] = useState(null)
	const [identity, setIdentity] = useState(null)
	const [notices, setNotices] = useState([])
	const [log, setLog] = useState([])
	const video = useRef(null)
	const scanning_ = useRef(null)

	// Both are fed by the far end, so both are bounded: a device repeating a fault or a refusal must
	// cost a fixed amount of memory rather than growing the view until the phone the operator is
	// trying to diagnose stops responding.
	const note = useCallback((line) => setLog((lines) => [...lines, line].slice(-KEPT)), [])
	const notice = useCallback(
		(kind, detail) =>
			setNotices((all) => {
				const last = all[all.length - 1]
				if (last && last.kind === kind && last.detail === detail) return all
				return [...all, { kind, detail, id: `${Date.now()}-${all.length}` }].slice(-KEPT)
			}),
		[],
	)

	// One reading off the wire, already sorted into the outcomes of BLI-MSG by the protocol half. The
	// application renders what it understood and says what it could not, and never blanks the view for
	// either: a device newer than this build is ordinary, and most of a view beats none of it.
	const onEvent = useCallback(
		(event) => {
			switch (event.kind) {
				case 'message':
					if (event.message.type === 'device-hello') {
						setDevice({ name: event.message.name, version: event.message.version })
					} else if (event.message.type === 'identity') {
						setIdentity(event.message)
					}
					break
				case 'skipped':
					// Safe to pass over, and silent on screen: it belongs in the record, not in the way.
					note(event.detail)
					break
				case 'refused':
					notice('refused', `The device said something this version of the app is too old to act on: ${event.detail}`)
					break
				case 'fault':
					notice('fault', `The device is not speaking the protocol: ${event.detail}`)
					break
			}
		},
		[note, notice],
	)

	const readFrom = useCallback(
		async (text) => {
			try {
				setSticker(await client.readSticker(text))
				setReadError('')
			} catch (error) {
				setReadError(error.message ?? String(error))
			}
		},
		[client],
	)

	// Following the link opens the application with the payload already in the fragment. It is read
	// here in the browser and goes no further.
	useEffect(() => {
		if (location.hash.length > 1) readFrom(location.hash)
	}, [readFrom])

	// A subscription lasts exactly as long as the operator is looking. Hiding the page closes the
	// stream, which is the unsubscribe, and showing it again opens a fresh one: this is what keeps a
	// phone in a pocket from pulling samples over a link nobody is reading (BLI-MSG).
	useEffect(() => {
		if (!connected) return
		// The in-flight open is tracked rather than the handle it resolves to. Tracking the handle
		// means a close arriving while an open is still in flight finds nothing to close, and the
		// stream that arrives a moment later is never closed by anyone: the device goes on pushing to
		// a page that is hidden or gone, which is the one thing this mechanism exists to prevent.
		let opening = null
		let stopped = false

		const open = () => {
			if (opening || stopped || document.hidden) return
			opening = client.subscribe(TOPIC, { onEvent, onClosed: () => {} }).then(async (handle) => {
				// The page may have been hidden, or the effect torn down, while this was in flight.
				if (stopped || document.hidden) {
					await handle.close()
					return null
				}
				return handle
			})
		}

		const close = async () => {
			const inFlight = opening
			opening = null
			if (!inFlight) return
			const handle = await inFlight
			if (handle) await handle.close()
		}

		const onVisibility = () => (document.hidden ? close() : open())

		open()
		document.addEventListener('visibilitychange', onVisibility)
		return () => {
			stopped = true
			document.removeEventListener('visibilitychange', onVisibility)
			close()
		}
	}, [connected, client, onEvent])

	async function connect() {
		setConnecting(true)
		setConnectStatus('Looking for the device...')
		try {
			await client.connect(sticker.sticker, {
				onEvent,
				onClosed: (why) =>
					note(why ? `The device stopped reporting: ${why}` : 'The device stopped reporting.'),
				onDisconnected: () => {
					note('The device disconnected.')
					setConnected(false)
					setConnecting(false)
					setConnectStatus('Disconnected.')
				},
			})
			setConnectStatus('')
			setConnected(true)
			note('Channel open.')
		} catch (error) {
			// Picking nothing in the chooser is an ordinary thing to do, not a failure to report.
			setConnectStatus(error.name === 'NotFoundError' ? '' : (error.message ?? String(error)))
			setConnecting(false)
			client.disconnect()
		}
	}

	async function startScan() {
		const controller = new AbortController()
		scanning_.current = controller
		setScanning(true)
		setReadError('')
		try {
			const found = await scan(video.current, {
				read: (text) => client.readSticker(text),
				onRejected: setReadError,
				signal: controller.signal,
			})
			if (found) {
				setSticker(found)
				setReadError('')
			}
		} catch (error) {
			setReadError(`The camera is not available: ${error.message ?? error}`)
		} finally {
			scanning_.current = null
			setScanning(false)
		}
	}

	// Leaving the sticker screen with the camera running would leave it running with nothing showing
	// it, so the scan is cancelled on the way out as well as by the button.
	useEffect(() => () => scanning_.current?.abort(), [])

	if (unsupported) {
		return (
			<main>
				<h1>bliti</h1>
				<section>
					<h2>Not available</h2>
					<p className="bad">{unsupported}</p>
				</section>
			</main>
		)
	}

	return (
		<main>
			<h1>bliti</h1>

			{!sticker && (
				<section>
					<h2>Sticker</h2>
					<p className="muted">Scan the code on the device, or type the letters printed under it.</p>
					<TypedSticker onRead={readFrom} />
					{cameraAvailable() && (
						<div className="row" style={{ marginTop: 8 }}>
							{scanning ? (
								<button className="secondary" onClick={() => scanning_.current?.abort()}>
									Stop the camera
								</button>
							) : (
								<button className="secondary" onClick={startScan}>
									Scan with camera
								</button>
							)}
						</div>
					)}
					{readError && <p className="bad">{readError}</p>}
					<video ref={video} hidden={!scanning} playsInline muted />
				</section>
			)}

			{sticker && !connected && (
				<section>
					<h2>Sticker read</h2>
					<p className="code">{sticker.human}</p>
					<p className="muted">
						Your device appears as a jumble of letters that changes. Pick it, and this checks it
						against your sticker.
					</p>
					<div className="row">
						<button onClick={connect} disabled={connecting}>
							Find the device
						</button>
						<button className="secondary" onClick={() => setSticker(null)}>
							Use another sticker
						</button>
					</div>
					{connectStatus && <p className="muted">{connectStatus}</p>}
				</section>
			)}

			{connected && (
				<section>
					<h2>Device</h2>
					{notices.map((each) => (
						<p key={each.id} className={`notice ${each.kind}`}>
							{each.detail}
						</p>
					))}
					<dl>
						{device ? (
							<>
								<dt>Running</dt>
								<dd>
									{device.name} {device.version}
								</dd>
							</>
						) : (
							<>
								<dt>Running</dt>
								<dd className="muted">not reported</dd>
							</>
						)}
						{identity && (
							<>
								<dt>Hostname</dt>
								<dd>{identity.hostname}</dd>
								{identity.addresses.map((address) => (
									<Fragment key={`${address.interface}-${address.address}`}>
										<dt>{address.interface}</dt>
										<dd>{address.address}</dd>
									</Fragment>
								))}
							</>
						)}
					</dl>
				</section>
			)}

			{log.length > 0 && (
				<section>
					<h2>Activity</h2>
					<div className="log muted">
						{log.map((line, index) => (
							<p key={index}>{line}</p>
						))}
					</div>
				</section>
			)}
		</main>
	)
}

function TypedSticker({ onRead }) {
	const [typed, setTyped] = useState('')
	return (
		<div className="row">
			<input
				value={typed}
				onChange={(event) => setTyped(event.target.value)}
				onKeyDown={(event) => event.key === 'Enter' && onRead(typed)}
				placeholder="AHFY-TP4T-..."
				autoComplete="off"
				autoCapitalize="characters"
				spellCheck="false"
			/>
			<button onClick={() => onRead(typed)}>Use</button>
		</div>
	)
}
