import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import Control from './Control.jsx'
import Network, { HeldBar } from './Network.jsx'
import Readings, { Identity } from './Readings.jsx'
import { CLIENT_VERSION, NEEDS_CHOOSER, NOTHING_PICKED, createClient } from './client.js'
import { entryOf, forgetHistory, hasValue, identityKey, isEnded, pushHistory } from './readings.js'
import { lastGroup, loadOn, loadRecent, remember, saveOn, saveRecent } from './remembered.js'
import { cameraAvailable, scan } from './scanner.js'

// How many notices are kept. The far end decides how many arrive.
const KEPT = 20

// How many activity lines are kept. Longer than the notices, because the log doubles as a debug
// surface and the interesting line is often several steps back.
const LOGGED = 100

// Readings stream continuously on the feed and would drown the log, so it records that the feed is
// running rather than every reading on it. Facts and the hello are rare and are recorded.
const STREAMED = new Set(['reading'])

// What the application says while a device carries an act out. Only ever that it is under way: an
// act accepted is not an act done, and nothing is left connected to say which it was (WEB).
const UNDER_WAY = { restart: 'Restarting bliti…', reboot: 'Rebooting…', 'power-off': 'Shutting down…' }

// How long to wait between attempts to reach a device coming back, how long to go on trying before
// saying it has not come back, and how long "Shutting down…" stays up once the channel has closed.
// The harness shortens them, which is the only thing it changes.
const TIMINGS = {
	retry: 3_000,
	giveUp: 180_000,
	hold: 3_000,
	...(__TEST_SEAM__ ? window.__blitiTimings : undefined),
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

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
	// Which screen of a connected device is showing: its readings, its control, or its network
	// configuration.
	const [screen, setScreen] = useState('device')
	// The act the device is carrying out, from being told of it until it is back, has not come back,
	// or the operator lets it go (WEB). Mirrored in a ref for the channel's callbacks, which outlive
	// any one render.
	const [going, setGoing] = useState(null)
	// Why the device is carrying the act out, as `going-away` gave it. Set with every act, so it never
	// outlives the one it came with.
	const [cause, setCause] = useState(null)
	const going_ = useRef(null)
	// The attempt to reach a device coming back, and the hold on "Shutting down…", either of which
	// disconnecting cancels.
	const returning = useRef(null)
	const holding_ = useRef(null)
	// Where the configuration session stands, as the network screen reports it, and whether to keep
	// that screen, and its session, while the operator is on the device view: from leaving it with edits
	// not applied or a proposal applying or applied, until there is nothing left to apply, confirm or
	// review (NSCR).
	const [held, setHeld] = useState(null)
	const [keep, setKeep] = useState(false)
	// Every fact and reading the device has sent, the latest of each kept under its identity, and the
	// history each reading accumulates forward from when the feed opened (VIEW). No history is sent by
	// the device; a graph fills forward from connection.
	const [entries, setEntries] = useState(() => new Map())
	const [history, setHistory] = useState(() => new Map())
	const [notices, setNotices] = useState([])
	const [log, setLog] = useState([])
	// What the tab held before this load, read once, so that a second run of the mount effect under
	// StrictMode sees the same fragment and the same device as the first (WEB, "After a reload").
	const [start] = useState(() => ({ fragment: location.hash, on: loadOn(), recent: loadRecent() }))
	// The devices this tab has opened a channel with, most recent first, once those stored have been
	// checked to still read. Null until then (WEB, "Remembering devices").
	const [recent, setRecent] = useState(null)
	const video = useRef(null)
	const scanning_ = useRef(null)
	const code_ = useRef(null)
	code_.current = code

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

	// The device is about to carry out an act: say so until it is over (WEB). An act this build does
	// not know is left to end the channel as anything else does.
	const startGoing = useCallback(
		(act, cause = null) => {
			if (!UNDER_WAY[act] || going_.current) return
			going_.current = act
			setGoing(act)
			setCause(cause)
			setScreen('device')
			note('note', `the device is carrying out ${act}`)
		},
		[note],
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
					} else if (message.type === 'going-away') {
						startGoing(message.act, message.cause)
					} else if (message.type === 'fact' || message.type === 'reading') {
						const entry = entryOf(message)
						if (isEnded(entry)) {
							setEntries((held) => {
								const next = new Map(held)
								next.delete(identityKey(entry))
								return next
							})
							setHistory((held) => forgetHistory(held, entry))
						} else {
							setEntries((held) => new Map(held).set(identityKey(entry), entry))
							setHistory((held) => pushHistory(held, entry))
						}
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
		[note, notice, startGoing],
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

	// Following the link opens the application with the payload already in the fragment, which is
	// taken off the address once read so a reload comes back to what the page held (WEB). Otherwise, a
	// page reloaded while on a device comes back holding its code. A remembered code that no longer
	// reads is forgotten, and never held.
	useEffect(() => {
		let live = true
		const stillReads = (text) => client.readCode(text).catch(() => null)
		if (start.fragment.length > 1) {
			window.history.replaceState(null, '', location.pathname + location.search)
			readFrom(start.fragment)
		}
		;(async () => {
			const read = await Promise.all(start.recent.map((each) => stillReads(each.code)))
			if (!live) return
			setRecent((held) => held ?? start.recent.filter((_, index) => read[index]))
			if (start.fragment.length > 1 || !start.on) return
			const back = await stillReads(start.on)
			if (!live || !back) return
			setCode((held) => held ?? back)
			note('note', `code held from before the reload  ${back.human}`)
		})()
		return () => {
			live = false
		}
	}, [client, readFrom, note, start])

	// Remembered whenever a channel opens, which also puts it at the top, and again once it says what
	// it is called.
	const hostnameEntry = connected
		? [...entries.values()].find((entry) => entry.fact && entry.name === 'hostname' && hasValue(entry))
		: undefined
	const hostname = hostnameEntry ? String(hostnameEntry.value) : undefined
	useEffect(() => {
		if (connected && code) setRecent((held) => remember(held ?? [], code.human, hostname))
	}, [connected, code, hostname])
	useEffect(() => {
		if (recent) saveRecent(recent)
	}, [recent])

	// The device the page is on, for as long as a reload should come back to it: while the channel is
	// open, and while the device carries out an act.
	const on = code && (connected || going) ? code.human : null
	useEffect(() => saveOn(on), [on])

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

	// The end of a going-away, however it ends: the operator back at the code they read, with the
	// device let go of and `status` said.
	const endGoing = useCallback(
		(status) => {
			returning.current = null
			clearTimeout(holding_.current)
			holding_.current = null
			going_.current = null
			setGoing(null)
			client.disconnect()
			setConnected(false)
			setConnecting(false)
			setScreen('device')
			setKeep(false)
			setEntries(new Map())
			setHistory(new Map())
			setConnectStatus(status)
		},
		[client],
	)

	// What a channel reports back, the same for the first connection and every one after it.
	const handlers = () => ({
		onEvent,
		onActivity: note,
		onClosed: (why) => note('note', why ? `the feed ended: ${why}` : 'the feed ended'),
		onDisconnected: closed,
	})

	function closed(why) {
		note('note', why ? `the connection to the device closed: ${why}` : 'the device disconnected')
		setConnected(false)
		setConnecting(false)
		const act = going_.current
		if (act === 'power-off') {
			// Said for long enough to be read, then gone (WEB).
			holding_.current = setTimeout(() => endGoing('The device disconnected.'), TIMINGS.hold)
			return
		}
		if (act) {
			comeBack()
			return
		}
		setScreen('device')
		setConnectStatus(why ? `The connection to the device failed: ${why}` : 'The device disconnected.')
	}

	// Reach the device again once it is back, on its own and without the chooser, until it answers
	// or is unlikely to (WEB).
	async function comeBack() {
		const attempt = {}
		returning.current = attempt
		const deadline = Date.now() + TIMINGS.giveUp
		while (Date.now() < deadline) {
			await sleep(TIMINGS.retry)
			if (returning.current !== attempt) return
			try {
				await client.reconnect(code_.current.qr, handlers())
			} catch (error) {
				if (returning.current !== attempt) return
				if (error.name === NEEDS_CHOOSER) {
					endGoing('The device has to be picked again. Find it to reconnect.')
					return
				}
				note('note', `not back yet: ${error.message ?? error}`)
				continue
			}
			if (returning.current !== attempt) return
			returning.current = null
			going_.current = null
			setGoing(null)
			setEntries(new Map())
			setHistory(new Map())
			setScreen('device')
			setConnected(true)
			note('note', 'the device is back')
			return
		}
		if (returning.current === attempt) endGoing('The device has not come back. Check it is on and nearby.')
	}

	async function connect() {
		setConnecting(true)
		setConnectStatus('Looking for the device...')
		try {
			await client.connect(code.qr, handlers())
			setConnectStatus('')
			setConnected(true)
		} catch (error) {
			const why = error.message ?? String(error)
			const nothing = error.name === NOTHING_PICKED
			note('note', nothing ? 'no device chosen' : `could not connect: ${why}`)
			setConnectStatus(nothing ? 'If your device was not in the list, check it is on and nearby.' : why)
			setConnecting(false)
			client.disconnect()
		}
	}

	function disconnect() {
		returning.current = null
		clearTimeout(holding_.current)
		holding_.current = null
		going_.current = null
		setGoing(null)
		client.disconnect()
		setConnected(false)
		setScreen('device')
		setKeep(false)
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

	// Done with what it was kept for: confirmed or discarded, or the session gone.
	useEffect(() => {
		if (screen !== 'network' && keep && (!held || (held.stage === 'editing' && held.changes === 0))) setKeep(false)
	}, [screen, keep, held])

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

	function leaveNetwork() {
		setScreen('control')
		setKeep(held?.stage === 'applying' || held?.stage === 'applied' || held?.changes > 0)
	}

	// What each interface is joined to, from the device's wireless-network facts (NFO).
	const joined = [...entries.values()]
		.filter((entry) => entry.fact && entry.name === 'wireless-network' && !isEnded(entry))
		.map((entry) => ({
			interface: entry.traits?.interface?.name,
			ssid: entry.value,
			band: entry.traits?.channel?.band,
			channel: entry.traits?.channel?.number,
		}))

	const all = [...entries.values()]
	const provisional = all.some(
		(entry) => entry.fact && entry.name === 'network-configuration' && entry.value === 'provisional',
	)
	// Edits the network screen holds that no proposal carries yet, which an act would cost (CSCR).
	const unapplied = held !== null && held.changes > 0 && (held.stage === 'editing' || held.stage === 'errored')

	const network = connected && !going && (screen === 'network' || keep) && (
		<div hidden={screen !== 'network'}>
			<Network
				client={client}
				onActivity={note}
				onEvent={noteSession}
				onBack={leaveNetwork}
				onStage={setHeld}
				onDisconnect={disconnect}
				joined={joined}
			/>
		</div>
	)

	if (connected && !going && screen === 'network') {
		return (
			<main>
				{network}
				<Activity log={log} />
			</main>
		)
	}

	const holding = keep && held !== null
	// The kept configuration session's bar, on every screen but the network screen's own (NSCR).
	const heldBar = holding && !going && <HeldBar held={held} onReview={() => setScreen('network')} />

	// A screen of a connected device: its title with its actions beside it, where every such screen
	// carries them, and the kept session's bar beneath (VIEW, CSCR).
	const screenOf = (title, actions, body) => (
		<main>
			{network}
			<div className="heading title">
				<h1>{title}</h1>
				<div className="actions">{actions}</div>
			</div>
			{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}
			{heldBar}
			{body}
			<Activity log={log} />
		</main>
	)

	const disconnectButton = (
		<button className="secondary small" onClick={disconnect}>
			Disconnect
		</button>
	)
	const identity = (
		<>
			<h2>Device</h2>
			{device && (
				<p className="muted software">
					{device.name} {device.version}
				</p>
			)}
		</>
	)

	// The device carrying an act out: who it is, and what it is doing, until it is over (WEB).
	if (going) {
		return screenOf(
			'Info',
			disconnectButton,
			<>
				{identity}
				<Identity entries={all} />
				<section className="bar-state working" role="status">
					<p>
						<span className="spinner" aria-hidden="true" />
						{UNDER_WAY[going]}
					</p>
					{cause === 'low-battery' && <p>Its battery is low.</p>}
				</section>
			</>,
		)
	}

	if (connected && screen === 'control') {
		return screenOf(
			'Control',
			<button className="secondary small" onClick={() => setScreen('device')}>
				Back
			</button>,
			<Control
				client={client}
				onNetwork={() => setScreen('network')}
				onActivity={note}
				onEvent={noteSession}
				onAccepted={startGoing}
				provisional={provisional}
				unapplied={unapplied}
			/>,
		)
	}

	if (connected) {
		return screenOf(
			'Info',
			<>
				<button className="small" onClick={() => setScreen('control')}>
					Control
				</button>
				{disconnectButton}
			</>,
			<>
				{identity}
				{notices.map((each) => (
					<p key={each.id} className={`notice ${each.kind}`}>
						{each.detail}
					</p>
				))}
				<Readings entries={all} history={history} showProvisional={!holding} />
			</>,
		)
	}

	// The code held is a remembered device's, which is shown with its hostname (WEB).
	const known = code && recent?.find((each) => each.code === code.human)

	return (
		<main>
			{network}
			<h1>bliti</h1>
			{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}

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

			{!code && recent?.length > 0 && (
				<section>
					<h2>Recent</h2>
					<ul className="acts recent">
						{recent.map((each) => (
							<li key={each.code}>
								<div>
									{each.hostname && <span className="name">{each.hostname}</span>}
									<span className="code">…-{lastGroup(each.code)}</span>
								</div>
								<button
									className="secondary"
									aria-label={`Use ${each.hostname ?? lastGroup(each.code)}`}
									onClick={() => readFrom(each.code)}
								>
									Use
								</button>
							</li>
						))}
					</ul>
				</section>
			)}

			{code && !connected && (
				<section>
					<h2>QR code read</h2>
					{known?.hostname && <strong className="device-name">{known.hostname}</strong>}
					<p className="code">{code.human}</p>
					<p>
						<button className="link" onClick={() => download(code)}>
							Download SVG
						</button>
					</p>
					<p className="muted">
						A device named <span className="code">{code.localName}</span> will show up. It should be
						the only one in the list.
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

/// Save the code as SVG, for printing a replacement. Named after the rendering's last group, which is
/// all public key and matches the end of the rendering printed beside the code (WEB).
function download(code) {
	const url = URL.createObjectURL(new Blob([code.svg], { type: 'image/svg+xml' }))
	const link = document.createElement('a')
	link.href = url
	link.download = `bliti-${code.human.split('-').pop()}.svg`
	link.click()
	// Revoked once the browser has had a turn to start the download from it.
	setTimeout(() => URL.revokeObjectURL(url), 0)
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
