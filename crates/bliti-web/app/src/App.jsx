import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import Network from './Network.jsx'
import Readings from './Readings.jsx'
import { createClient } from './client.js'
import { entryOf, identityKey, pushHistory } from './readings.js'
import { cameraAvailable, scan } from './scanner.js'

// How many notices are kept. The far end decides how many arrive.
const KEPT = 20

// How many activity lines are kept. Longer than the notices, because the log doubles as a debug
// surface and the interesting line is often several steps back.
const LOGGED = 100

// Readings stream continuously on the feed and would drown the log, so it records that the feed is
// running rather than every reading on it. Facts and the hello are rare and are recorded.
const STREAMED = new Set(['reading'])

export default function App() {
	// The seam the harness fakes at: a fake client is fed decoded messages with no wasm and no
	// Bluetooth in the loop. Compiled out of any build but the harness's.
	const client = useMemo(() => (__TEST_SEAM__ && window.__blitiClient) || createClient(), [])

	const [unsupported] = useState(() => client.unsupported())
	const [code, setCode] = useState(null)
	const [readError, setReadError] = useState('')
	const [scanning, setScanning] = useState(false)
	const [connecting, setConnecting] = useState(false)
	const [connectStatus, setConnectStatus] = useState('')
	const [connected, setConnected] = useState(false)
	const [device, setDevice] = useState(null)
	// Which screen of a connected device is showing: its readings, or its network configuration.
	const [screen, setScreen] = useState('device')
	// Every fact and reading the device has sent, the latest of each kept under its identity, and the
	// history each reading accumulates forward from when the feed opened (VIEW). No history is sent by
	// the device; a graph fills forward from connection.
	const [entries, setEntries] = useState(() => new Map())
	const [history, setHistory] = useState(() => new Map())
	const [notices, setNotices] = useState([])
	const [log, setLog] = useState([])
	const video = useRef(null)
	const scanning_ = useRef(null)

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

	// One message off the wire, already sorted into the outcomes of MSG by the protocol half. The
	// application renders what it understood and says what it could not, and never blanks the view for
	// either: a device newer than this build is ordinary, and most of a view beats none of it.
	const onEvent = useCallback(
		(event) => {
			switch (event.kind) {
				case 'message': {
					const message = event.message
					if (message.type === 'hello') {
						setDevice({ name: message.name, version: message.version })
					} else if (message.type === 'fact' || message.type === 'reading') {
						const entry = entryOf(message)
						setEntries((held) => new Map(held).set(identityKey(entry), entry))
						setHistory((held) => pushHistory(held, entry))
					}
					if (!STREAMED.has(message.type)) note('in', describe(message))
					break
				}
				case 'skipped':
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

	// The configuration session's messages go to the log as the feed's do. Only the log: the screen
	// reads them itself.
	const noteSession = useCallback(
		(event) => note('in', event.kind === 'message' ? describe(event.message) : `${event.kind}  ${event.detail}`),
		[note],
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

	// Following the link opens the application with the payload already in the fragment.
	useEffect(() => {
		if (location.hash.length > 1) readFrom(location.hash)
	}, [readFrom])

	// The feed runs while the operator is looking. Hiding the page closes it, which is the decline;
	// showing it again subscribes to `default` to resume. This is what keeps a phone in a pocket from
	// pulling readings over a link nobody is reading (VIEW, MSG).
	useEffect(() => {
		if (!connected) return

		const onVisibility = () => {
			if (document.hidden) {
				client.pauseFeed()
			} else {
				client.resumeFeed({ onEvent, onActivity: note })
			}
		}

		// The pushed feed is already running from connect. Only decline it if the page starts hidden.
		if (document.hidden) client.pauseFeed()
		document.addEventListener('visibilitychange', onVisibility)
		return () => {
			document.removeEventListener('visibilitychange', onVisibility)
			client.pauseFeed()
		}
	}, [connected, client, onEvent, note])

	async function connect() {
		setConnecting(true)
		setConnectStatus('Looking for the device...')
		try {
			await client.connect(code.qr, {
				onEvent,
				onActivity: note,
				onClosed: (why) => note('note', why ? `the feed ended: ${why}` : 'the feed ended'),
				onDisconnected: (why) => {
					note('note', why ? `the connection to the device closed: ${why}` : 'the device disconnected')
					setConnected(false)
					setConnecting(false)
					setScreen('device')
					setConnectStatus(
						why ? `The connection to the device failed: ${why}` : 'The device disconnected.',
					)
				},
			})
			setConnectStatus('')
			setConnected(true)
		} catch (error) {
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
		setScreen('device')
		setConnecting(false)
		setConnectStatus('')
		setEntries(new Map())
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

	if (connected && screen === 'network') {
		return (
			<main>
				<Network client={client} onActivity={note} onEvent={noteSession} onBack={() => setScreen('device')} />
				<Activity log={log} />
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
						<div className="actions">
							<button className="secondary small" onClick={() => setScreen('network')}>
								Network settings
							</button>
							<button className="secondary small" onClick={disconnect}>
								Disconnect
							</button>
						</div>
					</div>
					{device && (
						<p className="muted software">
							{device.name} {device.version}
						</p>
					)}
					{notices.map((each) => (
						<p key={each.id} className={`notice ${each.kind}`}>
							{each.detail}
						</p>
					))}
					<Readings entries={[...entries.values()]} history={history} />
				</>
			)}

			<Activity log={log} />
		</main>
	)
}

function Activity({ log }) {
	if (log.length === 0) return null
	return (
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
	)
}

const ARROWS = { in: '←', out: '→', note: '·' }

function clock(at) {
	return at.toLocaleTimeString(undefined, {
		hour: '2-digit',
		minute: '2-digit',
		second: '2-digit',
	})
}

/// One message, summarised for the log: its type and enough to tell one from another, without
/// reprinting the traits or value of every reading.
function describe(message) {
	const { type, at, ...rest } = message
	const name = rest.fact ?? rest.measurement
	if (name) return `${type}  ${name}`
	const summary = Object.entries(rest)
		.map(([member, value]) => {
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
