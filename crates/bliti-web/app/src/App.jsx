import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import Readings from './Readings.jsx'
import { createClient } from './client.js'
import { latest, pushSample, readHistory } from './readings.js'
import { cameraAvailable, scan } from './scanner.js'

// The topic the diagnostics feature defines. A device that does not serve it sends nothing and does
// not fail, which is what a device older than this application looks like (BLI-MSG, "Subscribing"),
// so subscribing here is correct against both and carries no readings until a device offers them.
const TOPIC = 'system'

// How many notices are kept. The far end decides how many arrive.
const KEPT = 20

// How many activity lines are kept. Longer than the notices, because the log doubles as a debug
// surface and the interesting line is often several steps back.
const LOGGED = 100

// Message types that arrive continuously while subscribed. They are the point of the view and would
// drown the log, so the log records that a subscription is running rather than every sample on it.
const STREAMED = new Set(['system-sample'])

export default function App() {
	// The seam the harness fakes at: a fake client is fed decoded messages with no wasm and no
	// Bluetooth in the loop. Compiled out of any build but the harness's, because anything able to run
	// a script on this origin before the app mounts could otherwise install its own client and become
	// the device's peer, and the presence token is the only credential there is.
	const client = useMemo(() => (__TEST_SEAM__ && window.__blitiClient) || createClient(), [])

	const [unsupported] = useState(() => client.unsupported())
	const [code, setCode] = useState(null)
	const [readError, setReadError] = useState('')
	const [scanning, setScanning] = useState(false)
	const [connecting, setConnecting] = useState(false)
	const [connectStatus, setConnectStatus] = useState('')
	const [connected, setConnected] = useState(false)
	const [device, setDevice] = useState(null)
	// What the device is, and how it is doing. The window holds the recent samples a graph is drawn
	// from; the device sends its buffered window before anything live, so a graph is populated the
	// moment it appears rather than filling from empty while an operator waits (BLI-SYS).
	const [statics, setStatics] = useState([])
	const [window_, setWindow] = useState([])
	const [history, setHistory] = useState(() => new Map())
	const [notices, setNotices] = useState([])
	const [log, setLog] = useState([])
	const video = useRef(null)
	const scanning_ = useRef(null)

	// Both are fed by the far end, so both are bounded: a device repeating a fault or a refusal must
	// cost a fixed amount of memory rather than growing the view until the phone the operator is
	// trying to diagnose stops responding.
	// Every line carries when it happened and which way it went, so the log reads as a record of the
	// conversation rather than as a place the view writes remarks.
	const note = useCallback(
		(direction, text) =>
			setLog((lines) => [...lines, { at: new Date(), direction, text }].slice(-LOGGED)),
		[],
	)
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
					} else if (event.message.type === 'system-identity') {
						setStatics(event.message.readings)
					} else if (event.message.type === 'system-sample') {
						setWindow((held) =>
							pushSample(held, { at: event.message.at, readings: event.message.readings }),
						)
					} else if (event.message.type === 'system-history') {
						setHistory(readHistory(event.message.series))
					}
					if (!STREAMED.has(event.message.type)) note('in', describe(event.message))
					break
				case 'skipped':
					// Safe to pass over, and silent on screen: it belongs in the record, not in the way.
					note('in', `skipped  ${event.detail}`)
					break
				case 'refused':
					note('in', `refused  ${event.detail}`)
					notice('refused', `The device said something this version of the app is too old to act on: ${event.detail}`)
					break
				case 'fault':
					note('in', `fault  ${event.detail}`)
					notice('fault', `The device is not speaking the protocol: ${event.detail}`)
					break
			}
		},
		[note, notice],
	)

	const readFrom = useCallback(
		async (text) => {
			try {
				const read = await client.readCode(text)
				setCode(read)
				setReadError('')
				note('note', `code read  ${read.human}`)
			} catch (error) {
				const why = error.message ?? String(error)
				setReadError(why)
				note('note', `code not read: ${why}`)
			}
		},
		[client, note],
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
			opening = client
				.subscribe(TOPIC, { onEvent, onClosed: () => {}, onActivity: note })
				.then(async (handle) => {
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
	}, [connected, client, onEvent, note])

	async function connect() {
		setConnecting(true)
		setConnectStatus('Looking for the device...')
		try {
			await client.connect(code.qr, {
				onEvent,
				onActivity: note,
				onClosed: (why) => note('note', why ? `reporting stream ended: ${why}` : 'reporting stream ended'),
				onDisconnected: () => {
					note('note', 'the device disconnected')
					setConnected(false)
					setConnecting(false)
					setConnectStatus('Disconnected.')
				},
			})
			setConnectStatus('')
			setConnected(true)
			// Nothing is logged here: the handshake and the device's first messages happen inside the
			// call above, so a line written now would sit behind them and read out of order.
		} catch (error) {
			// Picking nothing in the chooser is an ordinary thing to do, not a failure to report.
			const why = error.message ?? String(error)
			note('note', error.name === 'NotFoundError' ? 'no device chosen' : `could not connect: ${why}`)
			setConnectStatus(error.name === 'NotFoundError' ? '' : why)
			setConnecting(false)
			client.disconnect()
		}
	}

	function disconnect() {
		client.disconnect()
		setConnected(false)
		setConnecting(false)
		setConnectStatus('')
		setStatics([])
		setWindow([])
		setHistory(new Map())
		note('note', 'disconnected')
	}

	async function startScan() {
		const controller = new AbortController()
		scanning_.current = controller
		setScanning(true)
		setReadError('')
		try {
			const found = await scan(video.current, {
				read: (text) => client.readCode(text),
				onRejected: setReadError,
				signal: controller.signal,
			})
			if (found) {
				setCode(found)
				setReadError('')
				note('note', `code read from the camera  ${found.human}`)
			}
		} catch (error) {
			setReadError(`The camera is not available: ${error.message ?? error}`)
		} finally {
			scanning_.current = null
			setScanning(false)
		}
	}

	// Leaving the code screen with the camera running would leave it running with nothing showing
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

			{!code && (
				<section>
					<h2>QR code</h2>
					<p className="muted">Scan the code on the device, or type the letters printed under it.</p>
					<TypedCode onRead={readFrom} />
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

			{code && !connected && (
				<section>
					<h2>QR code read</h2>
					<p className="code">{code.human}</p>
					<p className="muted">
						Your device appears as a jumble of letters that changes. Pick it, and this checks it
						against your code.
					</p>
					<div className="row">
						<button onClick={connect} disabled={connecting}>
							Find the device
						</button>
						<button className="secondary" onClick={() => setCode(null)}>
							Use another code
						</button>
					</div>
					{connectStatus && <p className="muted">{connectStatus}</p>}
				</section>
			)}

			{connected && (
				<>
					<div className="heading">
						<h2>Device</h2>
						<button className="secondary small" onClick={disconnect}>
							Disconnect
						</button>
					</div>
					{notices.map((each) => (
						<p key={each.id} className={`notice ${each.kind}`}>
							{each.detail}
						</p>
					))}
					<Readings readings={latest(statics, window_)} window={window_} history={history} />
				</>
			)}

			{log.length > 0 && (
				<section>
					<h2>Activity</h2>
					<div className="log">
						{log.map((line, index) => (
							<p key={index} className={`line ${line.direction}`}>
								<time>{clock(line.at)}</time>
								<span className="arrow">{ARROWS[line.direction]}</span>
								<span className="said">{line.text}</span>
							</p>
						))}
					</div>
				</section>
			)}
		</main>
	)
}

const ARROWS = { in: '\u2190', out: '\u2192', note: '\u00b7' }

function clock(at) {
	return at.toLocaleTimeString(undefined, {
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
	})
}

/// One message, summarised for the log. The type and enough of the message to tell one from another,
/// without reprinting a hundred readings.
function describe(message) {
	const { type, ...rest } = message
	const summary = Object.entries(rest)
		.map(([member, value]) => {
			if (Array.isArray(value)) return `${member} ${value.length}`
			if (value && typeof value === 'object') return member
			return `${member} ${value}`
		})
		.join(', ')
	return summary ? `${type}  ${summary}` : type
}

function TypedCode({ onRead }) {
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
