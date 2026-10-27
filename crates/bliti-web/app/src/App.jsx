import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import Connect from './Connect.jsx'
import JoinWifi from './JoinWifi.jsx'
import Network from './Network.jsx'
import { useOfflineReady } from './offline.js'
import Power from './Power.jsx'
import Readings, { identityOf } from './Readings.jsx'
import { Activity, Title, TitleTaps, useDevMode } from './Screen.jsx'
import { CableScreen, NetworkScreen } from './Setup.jsx'
import ShareHotspot from './ShareHotspot.jsx'
import Status from './Status.jsx'
import { CLIENT_VERSION, NEEDS_CHOOSER, NOTHING_PICKED, createClient } from './client.js'
import { applyBatch, entryOf, hasValue, isEnded } from './readings.js'
import { loadOn, loadRecent, remember, saveOn, saveRecent, suffix } from './remembered.js'
import { useSetupSession } from './session.js'
import { statusRows } from './status.js'

// How many notices are kept. The far end decides how many arrive.
const KEPT = 20

// How many activity lines are kept. Longer than the notices, because the log doubles as a debug
// surface and the interesting line is often several steps back.
const LOGGED = 100

// Readings stream continuously on the feed and would drown the log, so it records that the feed is
// running rather than every reading on it, nor the end of every batch. Facts and the hello are rare
// and are recorded.
const STREAMED = new Set(['reading', 'end-of-batch'])

// Nothing held of a device: no entries, and no history.
const NOTHING_HELD = { entries: new Map(), history: new Map() }

// What the application says while a device carries an act out. Only ever that it is under way: an
// act accepted is not an act done, and nothing is left connected to say which it was (WEB).
const UNDER_WAY = { restart: 'Restarting bliti…', reboot: 'Rebooting…', 'power-off': 'Shutting down…' }

// How long to wait between attempts to reach a device coming back, how long to go on trying before
// saying it has not come back, how long to go on trying a device the phone let go of while the page
// was hidden, how long after the page is shown a drop still counts as one while hidden, and how long
// "Shutting down…" stays up once the channel has closed. The harness shortens them, which is the only
// thing it changes.
const TIMINGS = {
	retry: 3_000,
	giveUp: 180_000,
	away: 30_000,
	grace: 5_000,
	hold: 3_000,
	...(__TEST_SEAM__ ? window.__blitiTimings : undefined),
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

// The simple network screens, which share one configuration session (NSET).
const SIMPLE = new Set(['network', 'cable', 'wifi', 'hotspot'])

// Said on the network row where another session holds the configuration (NSET).
const BUSY = 'Someone else is making changes'

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
	// Which screen of a connected device is showing: its status, all its readings, its power and
	// battery, its network settings, or one of the simple network screens. Every screen but the status
	// has a history entry of its own, carrying how many levels below the status it is (WEB, "Moving
	// between screens"), and the ref is what the browser's way back is checked against.
	const [screen, setScreen] = useState('status')
	const screen_ = useRef('status')
	// Whether Advanced is open on the Status screen, kept while the operator goes down a level and back,
	// and closed whenever a channel opens (STAT).
	const [advanced, setAdvanced] = useState(false)
	// Set while the application itself is going back, so that going back does not ask again.
	const leaving_ = useRef(false)
	// How the screen showing asks before the operator leaves it with edits not applied (NSCR, NSET).
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
	// Where the channel closed while the page was hidden: `hidden` until the page is shown again, then
	// `reconnecting` until the device answers or is unlikely to (WEB, "While the page is hidden").
	const [away, setAway] = useState(null)
	const away_ = useRef(null)
	// When the page was last shown after being hidden. A browser that froze a hidden page may end its
	// link only as the page comes back, so a drop just after this counts as one while hidden (WEB).
	const shown_ = useRef(-Infinity)
	// How many channels have opened, so a screen holding a stream of its own opens it again on the next.
	const [channel, setChannel] = useState(0)
	const holding_ = useRef(null)
	// Every fact and reading the device has sent, the latest of each kept under its identity, and the
	// history each reading accumulates forward from when the feed opened (VIEW). No history is sent by
	// the device; a graph fills forward from connection. Both change together, a batch at a time (NFO).
	const [held, setHeld] = useState(NOTHING_HELD)
	const { entries, history } = held
	// What the feed has sent since its last `end-of-batch`, and whether the batch it is in is the
	// feed's first, which carries everything the device holds and replaces what is held (NFO).
	const pending_ = useRef([])
	const first_ = useRef(true)
	// Whether the first batch since the channel opened has ended: until it has, the Status screen is
	// still checking (STAT).
	const [settled, setSettled] = useState(false)
	const [notices, setNotices] = useState([])
	// The message types a notice has been raised for since the channel opened (STAT).
	const refused_ = useRef(new Set())
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

	// A channel about to open: its feed's first batch still to come, which replaces whatever is held,
	// and no notices (STAT). Called before the channel opens, so nothing it sends is lost to it.
	const opening = useCallback(() => {
		pending_.current = []
		first_.current = true
		refused_.current = new Set()
		setSettled(false)
		setNotices([])
	}, [])
	// A channel let go of: nothing of the device held any more, and not reaching for it either.
	const letGo = useCallback(() => {
		opening()
		setHeld(NOTHING_HELD)
		away_.current = null
		setAway(null)
	}, [opening])

	// Opening a screen goes one level down from the one showing, and the browser's way back brings the
	// operator up again, as the screen's Back does (WEB, "Moving between screens").
	const openScreen = useCallback((next) => {
		window.history.pushState({ screen: next, depth: (window.history.state?.depth ?? 0) + 1 }, '')
		screen_.current = next
		setScreen(next)
	}, [])
	const goBack = () => {
		leaving_.current = true
		window.history.back()
	}
	// Back at the status however the operator got there, with the entries of the screens they were on
	// taken out of the history.
	const toLanding = useCallback(() => {
		const depth = window.history.state?.depth ?? 0
		if (depth > 0) {
			leaving_.current = true
			window.history.go(-depth)
		}
		screen_.current = 'status'
		setScreen('status')
	}, [])

	useEffect(() => {
		const onPop = (event) => {
			const next = event.state?.screen ?? 'status'
			const confirmed = leaving_.current
			leaving_.current = false
			// An entry gone forward to with no device to show it on is nothing to go to.
			if ((!connected_.current && !away_.current) || going_.current) {
				if (next !== 'status') window.history.replaceState(null, '')
				return
			}
			if (!confirmed && next !== screen_.current && guard.current) {
				window.history.pushState({ screen: screen_.current, depth: (event.state?.depth ?? 0) + 1 }, '')
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
						pending_.current.push(entryOf(message))
					} else if (message.type === 'end-of-batch') {
						// Applied whole, so the moment between an entry ending and the one that replaces
						// it is never shown (NFO).
						const batch = pending_.current
						const first = first_.current
						pending_.current = []
						first_.current = false
						setHeld((held) => applyBatch(held, batch, first))
						setSettled(true)
					}
					if (!STREAMED.has(message.type)) note('in', describe(message))
					break
				}
				case 'skipped':
					note('in', `skipped  ${event.detail}`)
					break
				case 'refused':
					note('in', `refused  ${event.detail}`)
					// Said once for each message type, however often it arrives (STAT).
					if (refused_.current.has(event.type)) break
					refused_.current.add(event.type)
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

	// The session the simple network screens share, opened from the Status screen's network row and
	// closed on returning to it (NSET). `asking` is whether the application is waiting on its first
	// `state`: `open` from the Status screen, or `resume` under a simple screen already showing.
	const setup = useSetupSession(client, { onActivity: note, onEvent: noteSession })
	const [asking, setAsking] = useState(null)
	// What the network row says of the session, until the operator next asks for it or leaves.
	const [networkNote, setNetworkNote] = useState(null)

	const changeNetwork = () => {
		setNetworkNote(null)
		setAsking('open')
		setup.open()
	}
	const reopenSetup = () => {
		setAsking('resume')
		setup.resume()
	}
	const openSettings = () => {
		setup.close()
		openScreen('settings')
	}

	useEffect(() => {
		if (!asking || setup.state.status === 'opening') return
		setAsking(null)
		const { status, busy, why } = setup.state
		if (status === 'closed') {
			if (asking === 'open') setNetworkNote(why ? `Couldn't open the network settings: ${why}` : "Couldn't open the network settings.")
			return
		}
		if (busy) {
			setup.close()
			setNetworkNote(BUSY)
			if (asking === 'resume') toLanding()
			return
		}
		if (asking === 'open') openScreen('network')
	}, [asking, setup, toLanding, openScreen])

	// Held across the simple screens, opened again on coming back to one from the network settings,
	// and closed everywhere else, but on the Status screen while it opens (NSET).
	useEffect(() => {
		if (screen !== 'status') setNetworkNote(null)
		if (SIMPLE.has(screen)) {
			if (!setup.active) reopenSetup()
		} else if (screen !== 'status' || asking !== 'open') {
			setup.close()
		}
	}, [screen, asking, setup.active])

	// A channel reconnected under a simple screen opens the session again (NSET, WEB).
	const seenChannel_ = useRef(channel)
	useEffect(() => {
		if (seenChannel_.current === channel) return
		seenChannel_.current = channel
		if (setup.active && SIMPLE.has(screen_.current)) reopenSetup()
	}, [channel])

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
	// it is called. Nothing is held of a device once it is let go, so this is always the one the page
	// is on.
	const hostnameEntry = [...entries.values()].find(
		(entry) => entry.fact && entry.name === 'hostname' && hasValue(entry),
	)
	const hostname = hostnameEntry ? String(hostnameEntry.value) : undefined
	useEffect(() => {
		if (connected && code) setRecent((held) => remember(held ?? [], code.text, hostname))
	}, [connected, code, hostname])
	useEffect(() => {
		if (recent) saveRecent(recent)
	}, [recent])

	// The device the page is on, for as long as a reload should come back to it: while the channel is
	// open, while the device carries out an act, and while it is being reached again.
	const on = code && (connected || going || away) ? code.text : null
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
				// A resumed feed opens on everything the device holds, which replaces what was held:
				// an entry that ended while the feed was declined is never sent ended (NFO).
				pending_.current = []
				first_.current = true
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
			letGo()
			setProblem(problem)
		},
		[client, toLanding, letGo],
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
		if (document.hidden || Date.now() - shown_.current < TIMINGS.grace) {
			// A phone ends the link of a browser it has put away, or as it brings it back. The operator is
			// held where they were, and the device reached again once they are back (WEB).
			away_.current = 'hidden'
			setAway('hidden')
			return
		}
		toLanding()
		letGo()
		setProblem({ kind: 'lost', detail: why })
	}

	// A channel open, a first or a reconnected one.
	function opened() {
		setConnected(true)
		setChannel((count) => count + 1)
	}

	// Reach the device again on its own and without the chooser, until it answers or is unlikely to
	// (WEB). True once it answered, false where it did not or needs the chooser, and null where
	// something else took over from this attempt meanwhile.
	async function reachAgain(attempt, giveUp, { waitFirst }) {
		const deadline = Date.now() + giveUp
		for (let tries = 0; Date.now() < deadline; tries++) {
			if (waitFirst || tries > 0) await sleep(TIMINGS.retry)
			if (returning.current !== attempt) return null
			// What the device was is kept on screen while it is reached, and replaced by its first batch.
			opening()
			try {
				await client.reconnect(code_.current.qr, handlers())
			} catch (error) {
				if (returning.current !== attempt) return null
				if (error.name === NEEDS_CHOOSER) return false
				note('note', `not back yet: ${error.message ?? error}`)
				continue
			}
			if (returning.current !== attempt) return null
			returning.current = null
			return true
		}
		return returning.current === attempt ? false : null
	}

	// Reach the device again once it is back from a restart or a reboot (WEB).
	async function comeBack() {
		const attempt = {}
		returning.current = attempt
		const back = await reachAgain(attempt, TIMINGS.giveUp, { waitFirst: true })
		if (back === null) return
		if (!back) {
			endGoing({ kind: 'lost' })
			return
		}
		going_.current = null
		setGoing(null)
		toLanding()
		opened()
		note('note', 'the device is back')
	}

	// Reach the device again once the page is shown, back on the screen the operator was on (WEB).
	async function reconnectAway() {
		const attempt = {}
		returning.current = attempt
		away_.current = 'reconnecting'
		setAway('reconnecting')
		note('note', 'reconnecting to the device')
		const back = await reachAgain(attempt, TIMINGS.away, { waitFirst: false })
		if (back === null) return
		away_.current = null
		setAway(null)
		if (!back) {
			client.disconnect()
			toLanding()
			letGo()
			setProblem({ kind: 'lost' })
			return
		}
		opened()
		note('note', 'the device is back')
	}

	useEffect(() => {
		const onVisibility = () => {
			if (!document.hidden) shown_.current = Date.now()
		}
		document.addEventListener('visibilitychange', onVisibility)
		return () => document.removeEventListener('visibilitychange', onVisibility)
	}, [])

	useEffect(() => {
		if (away !== 'hidden') return
		const onVisibility = () => {
			if (!document.hidden) reconnectAway()
		}
		onVisibility()
		document.addEventListener('visibilitychange', onVisibility)
		return () => document.removeEventListener('visibilitychange', onVisibility)
	}, [away])

	async function connect() {
		const attempt = {}
		attempt_.current = attempt
		setPhase('choosing')
		setProblem(null)
		opening()
		setAdvanced(false)
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
		opened()
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
		letGo()
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

	// A device reached or being reached again: every screen of it stays up while it is (WEB).
	const live = connected || away
	// Said while the device is reached again, in place of what needs the channel to show.
	const reconnecting = (
		<section className="bar-state working" role="status">
			<p>
				<span className="spinner" aria-hidden="true" />
				Reconnecting to your Iti…
			</p>
		</section>
	)

	if (live && !going && screen === 'settings') {
		return shell(
			<Network
				client={client}
				onActivity={note}
				onEvent={noteSession}
				onBack={goBack}
				onDisconnect={disconnect}
				joined={joined}
				guard={guard}
				channel={channel}
				reconnecting={!!away}
			/>,
		)
	}

	if (live && !going && SIMPLE.has(screen)) {
		const frame = { session: setup, entries: all, onBack: goBack, reconnecting: !!away, onReopen: reopenSetup }
		switch (screen) {
			case 'network':
				return shell(<NetworkScreen {...frame} onOpen={openScreen} onSettings={openSettings} />)
			case 'cable':
				return shell(<CableScreen {...frame} onDone={toLanding} />)
			case 'wifi':
				return shell(<JoinWifi {...frame} onDone={toLanding} onSettings={openSettings} guard={guard} />)
			default:
				return shell(<ShareHotspot {...frame} name={hostname ?? suffix(code.text)} guard={guard} />)
		}
	}

	// A screen of a connected device: its title with its actions beside it, where every such screen
	// carries them (STAT, VIEW, CSCR).
	const screenOf = (title, actions, body, sub) =>
		shell(
			<>
				<Title title={title} sub={sub}>
					{actions}
				</Title>
				{import.meta.env.DEV && <p className="muted">bliti-web {CLIENT_VERSION}</p>}
				{body}
			</>,
		)
	const back = (
		<button className="secondary small" onClick={goBack}>
			Back
		</button>
	)

	// The Status screen: titled with the device's hostname, or the end of its code where it reports
	// none, its identity in short beneath and Disconnect beside it (STAT).
	const statusOf = (body) =>
		screenOf(
			hostname ?? suffix(code.text),
			<button className="secondary small" onClick={disconnect}>
				Disconnect
			</button>,
			body,
			identityOf((name) => all.find((entry) => entry.name === name)),
		)

	// The device carrying an act out: what it is doing in place of the rows and Advanced, until it is
	// over (WEB, STAT).
	if (going) {
		return statusOf(
			<section className="bar-state working" role="status">
				<p>
					<span className="spinner" aria-hidden="true" />
					{UNDER_WAY[going]}
				</p>
				{cause === 'low-battery' && <p>Its battery is low.</p>}
			</section>,
		)
	}

	if (live && screen === 'power') {
		return screenOf(
			'Power and battery',
			back,
			away ? (
				reconnecting
			) : (
				<Power key={channel} client={client} onActivity={note} onEvent={noteSession} onAccepted={startGoing} />
			),
		)
	}

	if (live && screen === 'readings') {
		return screenOf(
			'All readings',
			back,
			<>
				{away && reconnecting}
				<h2>Device</h2>
				{device && (
					<p className="muted software">
						{device.name} {device.version}
					</p>
				)}
				<Readings entries={all} history={history} />
			</>,
		)
	}

	if (away) return statusOf(reconnecting)

	if (connected) {
		return statusOf(
			<>
				{notices.map((each) => (
					<p key={each.id} className={`notice ${each.kind}`}>
						{each.detail}
					</p>
				))}
				<Status
					rows={statusRows(all, settled)}
					onChange={changeNetwork}
					opening={asking === 'open'}
					note={networkNote}
					advanced={advanced}
					onAdvanced={setAdvanced}
					onOpen={openScreen}
				/>
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
