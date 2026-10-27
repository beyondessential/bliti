// The Join wifi screen of WIFI: a network, its password, and connecting, proposed as soon as the
// operator asks to connect.

import { useCallback, useEffect, useId, useState } from 'react'

import { acts } from './capabilities.js'
import { passphraseFits } from './network.js'
import { LeaveConfirm, useLeaveGuard } from './Screen.jsx'
import { SetupFrame, useDone, useOwnProposal } from './Setup.jsx'
import {
	joinFailure,
	joinWifi,
	joinedNetworks,
	namedSecurity,
	networksHeard,
	signalOf,
	waysOn,
	wifiCandidate,
	wifiOf,
} from './setup.js'

const STEPS = ['Network', 'Password', 'Connect']
// Which of the steps each part of the screen is.
const STEP_OF = { network: 0, name: 0, enterprise: 1, password: 1, connect: 2 }

const LENGTH = 'Use 8 to 63 characters.'

export default function JoinWifi({ session, entries, onBack, onDone, onSettings, guard, ...frame }) {
	const { state } = session
	const caps = state.capabilities
	const own = useOwnProposal(session)
	// Where the operator is: `at` names the part of the screen, with the network once one is chosen,
	// how it is secured, whether the operator named it, and where it sits in the ordering once
	// proposed.
	const [step, setStep] = useState({ at: 'network' })
	const [name, setName] = useState('')
	const [passphrase, setPassphrase] = useState('')
	const [shown, setShown] = useState(false)
	const [wrong, setWrong] = useState(false)
	const [problem, setProblem] = useState(null)
	// Whether the operator has typed a name or password not yet proposed (WIFI).
	const [typed, setTyped] = useState(false)
	const [leaving, setLeaving] = useState(false)
	// Counts each time the network step opens, which asks the device to scan.
	const [scans, setScans] = useState(0)
	const open = state.status === 'open'
	const canScan = acts(caps).scan

	// On opening the network step, and on Refresh.
	useEffect(() => {
		if (open && canScan && step.at === 'network') session.scan()
	}, [scans, open, canScan])

	const lost = typed && (step.at === 'name' || step.at === 'password')
	const ask = useCallback(() => setLeaving(true), [])
	useLeaveGuard(lost, guard, ask)

	const failure = own.phase === 'failed' ? joinFailure(state.failure, step.index) : null

	// A refused passphrase is asked for again (WIFI).
	useEffect(() => {
		if (failure === 'passphrase' && step.at === 'connect') {
			setStep({ ...step, at: 'password' })
			setWrong(true)
		}
	}, [failure, step])

	// Joined, or out of range and then heard: back to the Status screen (WIFI).
	const reached = failure === 'range' && state.states?.[step.index]?.is === 'default-route'
	const done = useDone(onDone)
	useEffect(() => {
		if (own.phase === 'applied' || reached) done()
	})

	const toNetworks = () => {
		own.forget()
		setTyped(false)
		setWrong(false)
		setProblem(null)
		setStep({ at: 'network' })
		setScans((count) => count + 1)
	}

	// The password step for `ssid`, holding the passphrase wifi holds where it names that network.
	const toPassword = (ssid, kind, named) => {
		const wifi = wifiOf(state.inForce).candidate
		setPassphrase(wifi?.ssid === ssid ? (wifi.security?.passphrase ?? '') : '')
		setWrong(false)
		setProblem(null)
		setShown(false)
		setStep({ at: 'password', ssid, kind, named })
	}

	const pick = (network) => {
		if (network.security === 'enterprise') setStep({ at: 'enterprise', ssid: network.ssid })
		else toPassword(network.ssid, network.security, false)
	}

	const connect = () => {
		if (!passphraseFits(passphrase)) {
			setProblem(LENGTH)
			return
		}
		const candidate = wifiCandidate(caps, { ssid: step.ssid, kind: step.kind, passphrase, named: step.named })
		const document = joinWifi(state.inForce, candidate)
		const found = own.propose(document)
		if (found) {
			setProblem(found.reason)
			return
		}
		setTyped(false)
		setWrong(false)
		setProblem(null)
		setStep({ ...step, at: 'connect', index: wifiOf(document).index })
	}

	const ways = waysOn(state.inForce)
	const current = STEP_OF[step.at]
	const fieldId = useId()

	let body
	switch (step.at) {
		case 'network': {
			const joined = new Set(joinedNetworks(entries))
			const heard = state.networks && networksHeard(state.networks, caps)
			const scanning = state.act?.type === 'scan' && !state.act.failure
			body = (
				<>
					<p className="lead">Choose a wifi network</p>
					{ways?.hotspot && <p className="muted">Joining turns the hotspot off.</p>}
					{scanning && (
						<p className="muted looking">
							<span className="spinner" aria-hidden="true" />
							Looking for networks…
						</p>
					)}
					{state.act?.type === 'scan' && state.act.failure && (
						<p className="why">Couldn't look for networks: {state.act.failure.reason}</p>
					)}
					{heard && heard.length === 0 && !scanning && <p className="muted looking">No wifi networks found nearby.</p>}
					{heard && heard.length > 0 && (
						<ul className="choices">
							{heard.map((network) => (
								<NetworkChoice key={network.ssid} network={network} joined={joined.has(network.ssid)} onPick={() => pick(network)} />
							))}
						</ul>
					)}
					<div className="links">
						{canScan && !scanning && (
							<button className="link" onClick={() => setScans((count) => count + 1)}>
								Refresh
							</button>
						)}
						{namedSecurity(caps) && (
							<button
								className="link"
								onClick={() => {
									setName('')
									setStep({ at: 'name' })
								}}
							>
								My network isn't listed
							</button>
						)}
					</div>
				</>
			)
			break
		}
		case 'name':
			body = (
				<>
					<p className="lead">Enter the network name</p>
					<p className="muted">Type it exactly, including capitals.</p>
					<label htmlFor={fieldId}>Network name</label>
					<input
						id={fieldId}
						value={name}
						autoComplete="off"
						onChange={(event) => {
							setName(event.target.value)
							setTyped(true)
						}}
					/>
					<button className="block" disabled={!name} onClick={() => toPassword(name, namedSecurity(caps), true)}>
						Continue
					</button>
					<div className="links">
						<button className="link" onClick={toNetworks}>
							Choose another network
						</button>
					</div>
				</>
			)
			break
		case 'enterprise':
			body = (
				<>
					<p className="lead problem">
						<span className="alert" aria-hidden="true">
							!
						</span>
						{step.ssid} needs more settings
					</p>
					<p className="muted">Networks that need a username are set up under Advanced.</p>
					<button className="block" onClick={onSettings}>
						Open network settings
					</button>
					<div className="links">
						<button className="link" onClick={toNetworks}>
							Choose another network
						</button>
					</div>
				</>
			)
			break
		case 'password': {
			const fault = wrong || problem
			body = (
				<>
					<p className="lead">Enter the password for {step.ssid}</p>
					<p className="muted">Ask your IT contact if you don't know it.</p>
					<label htmlFor={fieldId}>Password</label>
					<div className="input-show">
						<input
							id={fieldId}
							type={shown ? 'text' : 'password'}
							value={passphrase}
							autoComplete="off"
							className={fault ? 'field-fault' : undefined}
							aria-invalid={fault ? true : undefined}
							onChange={(event) => {
								setPassphrase(event.target.value)
								setTyped(true)
								setProblem(null)
							}}
						/>
						<button className="link" onClick={() => setShown(!shown)}>
							{shown ? 'Hide' : 'Show'}
						</button>
					</div>
					{problem && <p className="why">{problem}</p>}
					{!problem && wrong && <p className="why">That password didn't work. Check it and try again.</p>}
					<button className="block" onClick={connect}>
						Connect
					</button>
					<div className="links">
						<button className="link" onClick={toNetworks}>
							Choose another network
						</button>
					</div>
				</>
			)
			break
		}
		case 'connect':
			body = failure ? (
				<Failed
					failure={failure}
					ssid={step.ssid}
					reason={state.failure?.reason}
					onChoose={toNetworks}
					onPassword={() => toPassword(step.ssid, step.kind, step.named)}
				/>
			) : (
				<>
					<p className="lead">
						<span className="spinner" aria-hidden="true" />
						Connecting to {step.ssid}
					</p>
					<p className="muted">This can take a moment.</p>
				</>
			)
			break
	}

	return (
		<SetupFrame
			title="Join wifi"
			onBack={() => (lost ? setLeaving(true) : onBack())}
			session={session}
			leaving={
				leaving &&
				lost && (
					<LeaveConfirm
						question="Leave without connecting?"
						lost="What you have typed will be lost."
						onLeave={onBack}
						onStay={() => setLeaving(false)}
					/>
				)
			}
			{...frame}
		>
			<ol className="steps" aria-label="Progress">
				{STEPS.map((label, index) => (
					<li
						key={label}
						className={index < current ? 'done' : index === current ? 'current' : undefined}
						aria-current={index === current ? 'step' : undefined}
					>
						<span className="dot">{index < current ? '✓' : index + 1}</span>
						<span className="name">{label}</span>
					</li>
				))}
			</ol>
			<section>{body}</section>
		</SetupFrame>
	)
}

/// One network a scan heard: picked to join it, unless the device cannot join it at all.
function NetworkChoice({ network, joined, onPick }) {
	const { word, bars } = signalOf(network.signal)
	const joinable = network.security !== null
	return (
		<li>
			<button className={joined ? 'joined' : undefined} disabled={!joinable} onClick={onPick}>
				<span className="body">
					<span className="name">{network.ssid}</span>
					{joined && <span className="now">Connected</span>}
					{!joinable && <span className="now">The Iti can't join this network</span>}
				</span>
				<span className="signal">
					<svg viewBox="0 0 16 12" aria-hidden="true">
						{[0, 1, 2, 3].map((bar) => (
							<rect
								key={bar}
								x={bar * 4.2}
								y={9 - bar * 3}
								width="3"
								height={3 + bar * 3}
								rx="0.8"
								opacity={bar < bars ? undefined : 0.25}
							/>
						))}
					</svg>
					{word}
				</span>
				{joined ? (
					<span className="tick" role="img" aria-label="Joined" />
				) : (
					joinable && (
						<span className="chevron" aria-hidden="true">
							›
						</span>
					)
				)}
			</button>
		</li>
	)
}

/// Joining that failed other than at the passphrase: out of range, waiting for the device to hear the
/// network, or the device's reason.
function Failed({ failure, ssid, reason, onChoose, onPassword }) {
	return (
		<>
			{failure === 'range' ? (
				<>
					<p className="lead problem">
						<span className="alert" aria-hidden="true">
							!
						</span>
						Couldn't reach {ssid}
					</p>
					<p className="muted">Move the Iti closer to the wifi router. It joins as soon as it can.</p>
					<p className="muted">
						<span className="spinner" aria-hidden="true" />
						Waiting for {ssid}…
					</p>
				</>
			) : (
				<>
					<p className="lead problem">
						<span className="alert" aria-hidden="true">
							!
						</span>
						Couldn't join {ssid}
					</p>
					<p className="muted reason">{reason}</p>
				</>
			)}
			<div className="links">
				<button className="link" onClick={onChoose}>
					Choose another network
				</button>
				<button className="link" onClick={onPassword}>
					Change password
				</button>
			</div>
		</>
	)
}
