// The Tailscale screen of TSCR: how the device stands in Tailscale, every Tailscale act the device
// lists, the network check in full, and a ping, over a Tailscale stream of its own.

import { useState } from 'react'

import { useActs } from './acts.js'
import { share, useMomentary } from './share.js'
import { netcheckFigures, netcheckShare, pingSays, TAILSCALE_ACTS, verdict } from './services.js'
import { Row } from './Status.jsx'
import { fromNow } from './status.js'

const DATE = new Intl.DateTimeFormat('en-AU', { day: 'numeric', month: 'short', year: 'numeric' })

/// `row` is the Status screen's Tailscale row, `facts` the device's name, tailnet and sign-in lapse
/// where each carries a value, and `netcheck` the latest network check sent while the channel has
/// been open, which `onResult` is told of as each arrives.
export default function Tailscale({ client, channel, onActivity, onEvent, row, facts, hostname, netcheck, onResult, onSignIn }) {
	const stream = useActs(client, 'tailscale', channel, { onActivity, onEvent, onResult })
	const listed = stream.acts ?? []
	const offered = TAILSCALE_ACTS.filter((each) => listed.includes(each.act))
	const reasonOf = (...acts) => (stream.reason && acts.includes(stream.reason.act) ? stream.reason.text : null)

	return (
		<>
			<Details row={row} facts={facts} />
			{offered.length > 0 && (
				<Acts offered={offered} stream={stream} reason={reasonOf('sign-in', 'turn-on', 'turn-off', 'log-out')} onSignIn={onSignIn} />
			)}
			{(listed.includes('netcheck') || netcheck) && (
				<Netcheck stream={stream} listed={listed.includes('netcheck')} held={netcheck} hostname={hostname} reason={reasonOf('netcheck')} />
			)}
			{listed.includes('ping') && <Ping stream={stream} reason={reasonOf('ping')} />}
		</>
	)
}

const capitalised = (text) => text.charAt(0).toUpperCase() + text.slice(1)

function Details({ row, facts }) {
	const lines = [
		facts.name && ['Device name', facts.name],
		facts.tailnet && ['Tailnet', facts.tailnet],
		facts.expiry && ['Sign-in lapses', `${DATE.format(facts.expiry)}, ${fromNow(facts.expiry.getTime(), Date.now())}`],
	].filter(Boolean)
	return (
		<section>
			<h2>Details</h2>
			{row && (
				<ul className="health">
					<Row row={{ ...row, headline: capitalised(row.says ?? row.headline), lines: [] }} />
				</ul>
			)}
			{lines.length > 0 && <Figures figures={lines} />}
		</section>
	)
}

function Figures({ figures }) {
	return (
		<dl className="facts">
			{figures.map(([label, text]) => (
				<Fact key={label} label={label} text={text} />
			))}
		</dl>
	)
}

function Fact({ label, text }) {
	return (
		<>
			<dt>{label}</dt>
			<dd>{text}</dd>
		</>
	)
}

/// The acts listed, each confirmed beneath its row where it takes the device out of reach (TSCR).
function Acts({ offered, stream, reason, onSignIn }) {
	const [confirming, setConfirming] = useState(null)
	const ask = (act) => {
		setConfirming(null)
		stream.ask(act)
	}
	const press = (each) => {
		if (each.act === 'sign-in') onSignIn()
		else if (each.question) setConfirming(each.act)
		else ask(each.act)
	}
	return (
		<section>
			<h2>Acts</h2>
			<ul className="acts">
				{offered.map((each) => (
					<li key={each.act} className={each.act === confirming ? 'asking' : undefined}>
						<div>
							<span className="name">{each.name}</span>
							<span className="muted">{each.hint}</span>
						</div>
						{each.act === confirming ? (
							<div className="bar-state" role="alertdialog" aria-label={each.question}>
								<p>
									<strong>{each.question}</strong>
								</p>
								<p>{each.warning}</p>
								<div className="row">
									<button className={each.danger ? 'danger-fill' : undefined} onClick={() => ask(each.act)}>
										{each.button}
									</button>
									<button className="secondary" onClick={() => setConfirming(null)}>
										Cancel
									</button>
								</div>
							</div>
						) : (
							<button className={`secondary${each.danger ? ' danger' : ''}`} onClick={() => press(each)} disabled={stream.asking !== null}>
								{each.button}
							</button>
						)}
					</li>
				))}
			</ul>
			{reason && <p className="why reason">{reason}</p>}
		</section>
	)
}

/// The network check: run, shown in full, and shared (TSCR).
function Netcheck({ stream, listed, held, hostname, reason }) {
	const [shareLabel, shared] = useMomentary('Share', 'Copied')
	const checking = stream.awaiting === 'netcheck'
	const shown = held && verdict(held.result)
	const { figures, relays } = held ? netcheckFigures(held.result) : { figures: [], relays: [] }

	async function onShare() {
		const text = netcheckShare(hostname, held.at, held.result)
		if ((await share({ title: 'Network check', text }, text)) === 'copied') shared()
	}

	return (
		<section>
			<div className="heading">
				<h2>Network check</h2>
				<div className="actions">
					{held && (
						<button className="secondary small" onClick={onShare}>
							{shareLabel}
						</button>
					)}
					{listed && (
						<button className="secondary small" onClick={() => stream.ask('netcheck')} disabled={stream.asking !== null}>
							{held ? 'Run again' : 'Run'}
						</button>
					)}
				</div>
			</div>
			{checking && (
				<p className="muted" role="status">
					<span className="spinner" aria-hidden="true" />
					Checking…
				</p>
			)}
			{shown && !checking && (
				<>
					<p className={`verdict${shown.connects ? '' : ' bad'}`}>{shown.text}</p>
					<Figures figures={figures} />
					{relays.length > 0 && (
						<ul className="relays">
							{relays.map((relay) => (
								<li key={relay.id}>
									<span>{relay.name}</span>
									<span>{relay.latency}</span>
								</li>
							))}
						</ul>
					)}
				</>
			)}
			{reason && <p className="why reason">{reason}</p>}
		</section>
	)
}

/// A ping of a machine on the tailnet, or of the device's own choosing where none is named (TSCR).
function Ping({ stream, reason }) {
	const [peer, setPeer] = useState('')
	const pinging = stream.awaiting === 'ping'
	const said = stream.result?.act === 'ping' && !pinging ? pingSays(stream.result.result) : null
	return (
		<section>
			<h2>Ping</h2>
			<label className="flabel" htmlFor="peer">
				Machine
			</label>
			<div className="row inline">
				<input id="peer" value={peer} placeholder="Chosen by the Iti" onChange={(event) => setPeer(event.target.value)} />
				<button className="secondary" onClick={() => stream.ask('ping', { peer: peer.trim() || undefined })} disabled={stream.asking !== null}>
					Ping
				</button>
			</div>
			{pinging && (
				<p className="muted" role="status">
					<span className="spinner" aria-hidden="true" />
					Pinging…
				</p>
			)}
			{said && (
				<>
					<p className="verdict">{said.text}</p>
					{said.figures.length > 0 && <Figures figures={said.figures} />}
				</>
			)}
			{reason && <p className="why reason">{reason}</p>}
		</section>
	)
}
