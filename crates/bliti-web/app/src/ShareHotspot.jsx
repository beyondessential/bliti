// The Share hotspot screen of SHARE: a switch letting other devices join, and the name, password and
// QR code they join by, each change proposed as soon as the operator makes it.

import { useCallback, useId, useState } from 'react'

import { absent, hotspotOn, offersHotspot } from './capabilities.js'
import JoinCode from './JoinCode.jsx'
import { passphraseFits } from './network.js'
import { LeaveConfirm, useLeaveGuard } from './Screen.jsx'
import { SetupFrame, useOwnProposal } from './Setup.jsx'
import {
	cablePlugged,
	devices,
	freshHotspot,
	hotspotClients,
	setHotspotPassphrase,
	ssidOf,
	startHotspot,
	stopHotspot,
	waysOn,
	wifiOf,
} from './setup.js'

const LENGTH = 'Use 8 to 63 characters.'

/// `name` is what the Status screen titles the device by, which a hotspot not yet set up is named after.
export default function ShareHotspot({ session, entries, name, onBack, guard, ...frame }) {
	const { state } = session
	const caps = state.capabilities
	const own = useOwnProposal(session)
	// What the proposal under way changes: the switch or the passphrase.
	const [changing, setChanging] = useState(null)
	const [shown, setShown] = useState(false)
	// The passphrase being typed, while the operator is changing it.
	const [editing, setEditing] = useState(null)
	const [problem, setProblem] = useState(null)
	const [leaving, setLeaving] = useState(false)
	const fieldId = useId()

	const fresh = freshHotspot(caps, ssidOf(name), session.generated)
	const held = session.held
	const hotspot = held?.hotspot ?? fresh
	const on = hotspotOn(held)
	const applying = own.phase === 'applying'
	const failed = own.phase === 'failed' ? state.failure?.reason : null

	const lost = editing !== null && editing !== hotspot.passphrase
	const ask = useCallback(() => setLeaving(true), [])
	useLeaveGuard(lost, guard, ask)

	const propose = (what, document) => {
		const found = own.propose(document)
		setChanging(what)
		setProblem(found ? { what, reason: found.reason } : null)
		return !found
	}

	const turn = () => propose('switch', on ? stopHotspot(state.inForce) : startHotspot(state.inForce, fresh))

	const save = () => {
		if (!passphraseFits(editing)) {
			setProblem({ what: 'passphrase', reason: LENGTH })
			return
		}
		propose('passphrase', setHotspotPassphrase(state.inForce, editing, fresh))
	}

	// A passphrase saved is the one shown, and the field closes.
	if (editing !== null && changing === 'passphrase' && own.phase === 'applied') {
		setEditing(null)
		setChanging(null)
	}

	const wifi = waysOn(state.inForce).wifi && wifiOf(state.inForce).candidate
	const clients = hotspotClients(entries)
	const why = (what) => (problem?.what === what ? problem.reason : changing === what ? failed : null)

	let sub = null
	if (applying && changing === 'switch') {
		sub = (
			<>
				<span className="spinner" aria-hidden="true" />
				{on ? 'Turning on…' : 'Turning off…'}
			</>
		)
	} else if (on && clients !== null) {
		sub = `${devices(clients)} connected`
	}

	return (
		<SetupFrame
			title="Share hotspot"
			onBack={() => (lost ? setLeaving(true) : onBack())}
			session={session}
			leaving={
				leaving &&
				lost && (
					<LeaveConfirm
						question="Leave without saving the password?"
						lost="The new password will be lost."
						onLeave={onBack}
						onStay={() => setLeaving(false)}
					/>
				)
			}
			{...frame}
		>
			<section>
				{!offersHotspot(caps) ? (
					<p className="muted">{absent(caps, 'hotspot').sentence}</p>
				) : (
					<>
						<div className={`toggle${applying ? ' disabled' : ''}`}>
							<span className="body">
								<span className="name" id={`${fieldId}-switch`}>
									Let other devices join
								</span>
								{sub && <span className="sub">{sub}</span>}
							</span>
							<button
								className={`switch${on ? ' on' : ''}`}
								role="switch"
								aria-checked={on}
								aria-labelledby={`${fieldId}-switch`}
								disabled={applying}
								onClick={turn}
							/>
						</div>
						{why('switch') && <p className="why">{why('switch')}</p>}
						{!on && wifi && (
							<p className="muted running">
								The Iti will leave {wifi.ssid} and {cablePlugged(entries) ? 'use its cable' : 'go offline'}.
							</p>
						)}
						<div className="details">
							<dl>
								<dt>Network name</dt>
								<dd>
									<strong>{hotspot.ssid}</strong>
								</dd>
								{editing === null && (
									<>
										<dt>Password</dt>
										<dd className="secret">
											<span>{shown ? hotspot.passphrase : '••••••••'}</span>
											<button className="link" onClick={() => setShown(!shown)}>
												{shown ? 'Hide' : 'Show'}
											</button>
										</dd>
									</>
								)}
							</dl>
							{editing !== null && (
								<>
									<label htmlFor={fieldId}>Password</label>
									<input
										id={fieldId}
										value={editing}
										autoComplete="off"
										className={why('passphrase') ? 'field-fault' : undefined}
										aria-invalid={why('passphrase') ? true : undefined}
										onChange={(event) => {
											setEditing(event.target.value)
											setProblem(null)
										}}
									/>
									{why('passphrase') && <p className="why">{why('passphrase')}</p>}
									{on && !why('passphrase') && (
										<p className="muted hint">Devices already connected will need to reconnect with the new password.</p>
									)}
									<div className="row save">
										<button onClick={save} disabled={applying}>
											{applying && changing === 'passphrase' && <span className="spinner" aria-hidden="true" />}
											Save
										</button>
										<button
											className="secondary"
											disabled={applying}
											onClick={() => {
												setEditing(null)
												setProblem(null)
											}}
										>
											Cancel
										</button>
									</div>
								</>
							)}
							<JoinCode ssid={hotspot.ssid} passphrase={hotspot.passphrase} />
							{editing === null && (
								<div className="links">
									<button className="link" disabled={applying} onClick={() => setEditing(hotspot.passphrase)}>
										Change password
									</button>
								</div>
							)}
						</div>
					</>
				)}
			</section>
		</SetupFrame>
	)
}
