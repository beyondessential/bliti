// The sign-in screen of TSCR, Connect to Tailscale: signing the device in by an auth key, typed or
// scanned, or by a login link someone approves, over a Tailscale stream of its own.

import { useEffect, useRef, useState } from 'react'

import { useActs } from './acts.js'
import { loadProtocol } from './protocol.js'
import { cameraAvailable, scan } from './scanner.js'
import { canShare, share, useMomentary } from './share.js'
import { text_qr as textQr } from './wasm/bliti_web.js'

/// The states a sign-in has reached once Tailscale stands in them, which is when the screen goes back
/// to where it was opened from (TSCR).
const DONE = new Set(['running', 'needs-approval'])

/// `hostname` is what the name the device registers under starts as, `state` the device's
/// `tailscale-state`, and `onDone` how the screen goes back once a sign-in has reached its end.
export default function SignIn({ client, channel, onActivity, onEvent, hostname, state, onDone }) {
	const stream = useActs(client, 'tailscale', channel, { onActivity, onEvent })
	const [by, setBy] = useState('key')
	const [name, setName] = useState(hostname ?? '')
	const left = useRef(false)

	useEffect(() => {
		if (!DONE.has(state) || left.current) return
		left.current = true
		onDone()
	}, [state, onDone])

	// Sent only where the operator changed it, so the device registers under its own name otherwise.
	const named = name.trim() && name.trim() !== hostname ? name.trim() : undefined
	const nameField = (disabled) => (
		<>
			<label className="flabel" htmlFor="device-name">
				Device name
			</label>
			<input id="device-name" value={name} disabled={disabled} onChange={(event) => setName(event.target.value)} />
		</>
	)

	return by === 'key' ? (
		<ByKey stream={stream} nameField={nameField} named={named} onLink={() => setBy('link')} />
	) : (
		<ByLink stream={stream} nameField={nameField} named={named} onKey={() => setBy('key')} />
	)
}

/// Signing in by auth key, which the screen lets go of as it closes (TSCR).
function ByKey({ stream, nameField, named, onLink }) {
	const [key, setKey] = useState('')
	const [shown, setShown] = useState(false)
	const [scanning, setScanning] = useState(false)
	const [cameraFailed, setCameraFailed] = useState(false)
	const video = useRef(null)
	const scanning_ = useRef(null)

	useEffect(() => () => scanning_.current?.abort(), [])

	async function startScan() {
		const controller = new AbortController()
		scanning_.current = controller
		setScanning(true)
		setCameraFailed(false)
		try {
			const found = await scan(video.current, { read: (text) => text.trim(), signal: controller.signal })
			if (found) setKey(found)
		} catch {
			setCameraFailed(true)
		} finally {
			scanning_.current = null
			setScanning(false)
		}
	}

	const asking = stream.asking !== null
	return (
		<section className="form">
			<label className="flabel" htmlFor="auth-key">
				Auth key
			</label>
			<div className="input-show">
				<input
					id="auth-key"
					type={shown ? 'text' : 'password'}
					value={key}
					autoComplete="off"
					spellCheck={false}
					onChange={(event) => setKey(event.target.value)}
				/>
				<button className="link" onClick={() => setShown(!shown)}>
					{shown ? 'Hide' : 'Show'}
				</button>
			</div>
			<video ref={video} hidden={!scanning} playsInline muted />
			{cameraAvailable() &&
				(scanning ? (
					<button className="secondary scan" onClick={() => scanning_.current?.abort()}>
						Cancel
					</button>
				) : (
					<button className="secondary scan" onClick={startScan}>
						Scan a QR code
					</button>
				))}
			{cameraFailed && <p className="why">Camera not available.</p>}
			{nameField(false)}
			{stream.reason && <p className="why reason">{stream.reason.text}</p>}
			<div className="row go">
				<button
					disabled={!key.trim() || asking || !stream.open}
					onClick={() => stream.ask('sign-in', { method: 'auth-key', key: key.trim(), name: named })}
				>
					{stream.awaiting === 'sign-in' ? 'Connecting…' : 'Connect'}
				</button>
			</div>
			<p className="muted alt">
				No key?{' '}
				<button className="link" onClick={onLink}>
					Use a login link
				</button>
			</p>
		</section>
	)
}

/// Signing in by login link: waiting for it, then the latest one the device sent, to approve from
/// another device, share, or open here (TSCR).
function ByLink({ stream, nameField, named, onKey }) {
	const [asked, setAsked] = useState(false)
	const [qr, setQr] = useState(null)
	const [copyLabel, copied] = useMomentary('Copy', 'Copied')
	const { link } = stream

	useEffect(() => {
		if (!link) return setQr(null)
		let live = true
		loadProtocol()
			.then(() => live && setQr(textQr(link)))
			.catch(() => live && setQr(null))
		return () => {
			live = false
		}
	}, [link])

	const get = () => {
		setAsked(true)
		stream.ask('sign-in', { method: 'login-link', name: named })
	}
	const waiting = asked && !link && stream.awaiting === 'sign-in'
	const ended = asked && !link && !waiting

	return (
		<section className="form">
			{nameField(asked && !ended)}
			{waiting && (
				<p className="muted waiting" role="status">
					<span className="spinner" aria-hidden="true" />
					Getting a link…
				</p>
			)}
			{link && (
				<>
					<div className="join link">
						<div className="qr" role="img" aria-label="QR code of the login link" dangerouslySetInnerHTML={qr ? { __html: qr } : undefined} />
						<span className="caption">The Iti connects once someone approves this link.</span>
					</div>
					<p className="code">{link}</p>
					<div className="row">
						<button onClick={() => window.open(link, '_blank', 'noopener')}>Open</button>
						{canShare() && (
							<button className="secondary" onClick={() => share({ url: link }, link)}>
								Share
							</button>
						)}
						<button
							className="secondary"
							onClick={async () => {
								await navigator.clipboard.writeText(link)
								copied()
							}}
						>
							{copyLabel}
						</button>
					</div>
				</>
			)}
			{ended && <p className="why reason">{stream.reason?.text ?? 'The link has ended.'}</p>}
			{!asked || ended ? (
				<div className="row go">
					<button onClick={get} disabled={stream.asking !== null || !stream.open}>
						{ended ? 'Get a new link' : 'Get a login link'}
					</button>
				</div>
			) : null}
			<p className="muted alt">
				<button className="link" onClick={onKey}>
					Use an auth key
				</button>
			</p>
		</section>
	)
}
