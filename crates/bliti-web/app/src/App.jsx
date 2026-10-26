import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import Connect from './Connect.jsx'
import Network from './Network.jsx'
import { useOfflineReady } from './offline.js'
import Power from './Power.jsx'
import Readings, { Identity } from './Readings.jsx'
import { Activity, Title, TitleTaps, useDevMode } from './Screen.jsx'
import { CLIENT_VERSION, NEEDS_CHOOSER, NOTHING_PICKED, createClient } from './client.js'
import { entryOf, forgetHistory, hasValue, identityKey, isEnded, pushHistory } from './readings.js'
import { loadOn, loadRecent, remember, saveOn, saveRecent } from './remembered.js'

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
	// Said beside the title on the screen for reading a code until the application can open offline (WEB).
	const offlineReady = useOfflineReady()
	// Where an attempt to reach the device is: null, `choosing` while the chooser is open, or
	// `connecting` once a device is picked. The attempt itself is held in a ref, so one cancelled is
	// told apart from the one after it.
	const [phase, setPhase] = useState(null)
	const attempt_ = useRef(null)
	// Why the last attempt, or the last channel, ended, where it did not end by choice (WEB).
	const [problem, setProblem] = useState(null)
	const [connected, setConnected] = useState(false)
	const connected_ = useRef(false)
	connected_.current = connected
	const [device, setDevice] = useState(null)
	// Which screen of a connected device is showing: all its readings, its power and battery, or its
	// network settings. Every screen but the readings has a history entry of its own (WEB, "Moving
	// between screens"), and the ref is what the browser's way back is checked against.
	const [screen, setScreen] = useState('readings')
	const screen_ = useRef('readings')
	// Set while the application itself is going back, so that going back does not ask again.
	const leaving_ = useRef(false)
	// How the network settings screen asks before the operator leaves it with edits not applied (NSCR).
	const guard = useRef(null)
	const [dev, tap] = useDevMode()
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
	// Every fact and reading the device has sent, the latest of each kept under its identity, and the
	// history each reading accumulates forward from when the feed opened (VIEW). No history is sent by
	// the device; a graph fills forward from connection.
	const [entries, setEntries] = useState(() => new Map())
	const [history, setHistory] = useState(() => new Map())
	const [notices, setNotices] = useState([])
	const [log, setLog] = useState([])
	// What the tab held before this load, read once, so that a second run of the mount effect under
	// StrictMode sees the same device as the first (WEB, "After a reload").
	const [start] = useState(() => ({ on: loadOn(), recent: loadRecent() }))
	// The devices this tab has opened a channel with, most recent first, once those stored have been
	// checked to still read. Null until then (WEB, "Remembering devices").
	const [recent, setRecent] = useState(null)
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

	// Opening a screen goes one level down from the readings, and the browser's way back brings the
	// operator up again, as the screen's Back does (WEB, "Moving between screens").
	const openScreen = (next) => {
		window.history.pushState({ screen: next }, '')
		screen_.current = next
		setScreen(next)
	}
	const goBack = () => {
		leaving_.current = true
		window.history.back()
	}
	// Back at the readings however the operator got there, with the entry of the screen they were on
	// taken out of the history.
	const toLanding = useCallback(() => {
		if (window.history.state?.screen) {
			leaving_.current = true
			window.history.back()
		}
		screen_.current = 'readings'
		setScreen('readings')
	}, [])

	useEffect(() => {
		const onPop = (event) => {
			const next = event.state?.screen ?? 'readings'
			const confirmed = leaving_.current
			leaving_.current = false
			// An entry gone forward to with no device to show it on is nothing to go to.
			if (!connected_.current || going_.current) {
				if (next !== 'readings') window.history.replaceState(null, '')
				return
			}
			if (!confirmed && screen_.current === 'network' && next !== 'network' && guard.current) {
				window.history.pushState({ screen: 'network' }, '')
				guard.current()
				return
			}
			screen_.current = next
			setScreen(next)
		}
		// A reload ends the channel, so the screen whose entry it reloaded on is not there to go back from.
		if (window.history.state?.screen) window.history.replaceState(null, '')
		window.addEventListener('popstate', onPop)
		return () => window.removeEventListener('popstate', onPop)
	}, [])

	// The device is about to carry out an act: say so until it is over (WEB). An act this build does
	// not know is left to end the channel as anything else does.
	const startGoing = useCallback(
		(act, cause = null) => {
			if (!UNDER_WAY[act] || going_.current) return
			going_.current = act
			setGoing(act)
			setCause(cause)
			toLanding()
			note('note', `the device is carrying out ${act}`)
		},
		[note, toLanding],
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

	const onCode = (read, how) => {
		setCode(read)
		setProblem(null)
		note('note', `code read${how ? ` ${how}` : ''}  ${read.text}`)
	}

	// A page reloaded while on a device comes back holding its code (WEB). A remembered code that no
	// longer reads is forgotten, and never held.
	useEffect(() => {
		let live = true
		const stillReads = (text) => client.readCode(text).catch(() => null)
		;(async () => {
			const read = await Promise.all(start.recent.map((each) => stillReads(each.code)))
			if (!live) return
			setRecent((held) => held ?? start.recent.filter((_, index) => read[index]))
			if (!start.on) return
			const back = await stillReads(start.on)
			if (!live || !back) return
			setCode((held) => held ?? back)
			note('note', `code held from before the reload  ${back.text}`)
		})()
		return () => {
			live = false
		}
	}, [client, note, start])

	// Remembered whenever a channel opens, which also puts it at the top, and again once it says what
	// it is called.
	const hostnameEntry = connected
		? [...entries.values()].find((entry) => entry.fact && entry.name === 'hostname' && hasValue(entry))
		: undefined
	const hostname = hostnameEntry ? String(hostnameEntry.value) : undefined
	useEffect(() => {
		if (connected && code) setRecent((held) => remember(held ?? [], code.text, hostname))
	}, [connected, code, hostname])
	useEffect(() => {
		if (recent) saveRecent(recent)
	}, [recent])

	// The device the page is on, for as long as a reload should come back to it: while the channel is
	// open, and while the device carries out an act.
	const on = code && (connected || going) ? code.text : null
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
		(problem) => {
			returning.current = null
			clearTimeout(holding_.current)
			holding_.current = null
			going_.current = null
			setGoing(null)
			client.disconnect()
			setConnected(false)
			setPhase(null)
			toLanding()
			setEntries(new Map())
			setHistory(new Map())
			setProblem(problem)
		},
		[client, toLanding],
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
		setPhase(null)
		const act = going_.current
		if (act === 'power-off') {
			// Said for long enough to be read, then gone (WEB).
			holding_.current = setTimeout(() => endGoing(null), TIMINGS.hold)
			return
		}
		if (act) {
			comeBack()
			return
		}
		toLanding()
		setProblem({ kind: 'lost', detail: why })
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
					endGoing({ kind: 'lost' })
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
			toLanding()
			setConnected(true)
			note('note', 'the device is back')
			return
		}
		if (returning.current === attempt) endGoing({ kind: 'lost' })
	}

	async function connect() {
		const attempt = {}
		attempt_.current = attempt
		setPhase('choosing')
		setProblem(null)
		const picked = () => attempt_.current === attempt && setPhase('connecting')
		try {
			await client.connect(code.qr, { ...handlers(), onPicked: picked })
		} catch (error) {
			if (attempt_.current !== attempt) return
			attempt_.current = null
			const why = error.message ?? String(error)
			const nothing = error.name === NOTHING_PICKED
			note('note', nothing ? 'no device chosen' : `could not connect: ${why}`)
			setProblem(nothing ? { kind: 'not-found' } : { kind: 'failed', detail: why })
			setPhase(null)
			client.disconnect()
			return
		}
		// Cancelled after the channel opened anyway: let go of it, unless another attempt is under way.
		if (attempt_.current !== attempt) {
			if (!attempt_.current) client.disconnect()
			return
		}
		attempt_.current = null
		setPhase(null)
		toLanding()
		setConnected(true)
	}

	// Cancelling abandons the attempt, back at finding the device (WEB).
	function cancelConnect() {
		attempt_.current = null
		client.disconnect()
		setPhase(null)
		note('note', 'connecting cancelled')
	}

	function disconnect() {
		returning.current = null
		clearTimeout(holding_.current)
		holding_.current = null
		going_.current = null
		setGoing(null)
		client.disconnect()
		setConnected(false)
		toLanding()
		setPhase(null)
		setProblem(null)
		setEntries(new Map())
		setHistory(new Map())
		note('note', 'disconnected')
	}

	// Every screen: whatever it holds, and the activity record beneath it in dev mode (WEB).
	const shell = (body) => (
		<TitleTaps.Provider value={tap}>
			<main>
				{body}
				{dev && <Activity log={log} />}
			</main>
		</TitleTaps.Provider>
	)

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

	if (unsupported) return shell(<Connect unsupported={unsupported} />)

	if (connected && !going && screen === 'network') {
		return shell(
			<Network
				client={client}
				onActivity={note}
				onEvent={noteSession}
				onBack={goBack}
				onDisconnect={disconnect}
				joined={joined}
				guard={guard}
			/>,
		)
	}

	// A screen of a connected device: its title with its actions beside it, where every such screen
	// carries them (VIEW, CSCR).
	const screenOf = (title, actions, body) =>
		shell(
			<>
				<Title title={title}>{actions}</Title>
				{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}
				{body}
			</>,
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
			'All readings',
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

	if (connected && screen === 'power') {
		return screenOf(
			'Power and battery',
			<button className="secondary small" onClick={goBack}>
				Back
			</button>,
			<Power client={client} onActivity={note} onEvent={noteSession} onAccepted={startGoing} />,
		)
	}

	if (connected) {
		return screenOf(
			'All readings',
			<>
				<button className="secondary small" onClick={() => openScreen('power')}>
					Power and battery
				</button>
				<button className="secondary small" onClick={() => openScreen('network')}>
					Network settings
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
				<Readings entries={all} history={history} />
			</>,
		)
	}

	return shell(
		<Connect
				client={client}
				note={note}
				offlineReady={offlineReady}
				code={code}
				onCode={onCode}
				// The code held is a remembered device's, which is shown with its hostname (WEB).
				known={code && recent?.find((each) => each.code === code.text)}
				recent={recent}
				phase={phase}
				problem={problem}
				onFind={connect}
				onCancel={cancelConnect}
				onOtherCode={() => {
					setCode(null)
					setProblem(null)
				}}
			/>,
	)
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
