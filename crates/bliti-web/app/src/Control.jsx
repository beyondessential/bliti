// The Control screen of CSCR: the way into the network screen, and the acts of CTL the device
// offers, each confirmed before it is asked for.

import { useEffect, useRef, useState } from 'react'

/// The acts this build knows, in the order it offers them, and what it calls each (CSCR).
export const ACTS = [
	{
		act: 'restart',
		name: 'Restart bliti',
		hint: 'Only the provisioning service.',
		button: 'Restart',
		question: 'Restart bliti?',
	},
	{
		act: 'reboot',
		name: 'Reboot',
		hint: 'The whole device.',
		button: 'Reboot',
		question: 'Reboot the device?',
	},
	{
		act: 'power-off',
		name: 'Power off',
		hint: 'Stays off until turned on at the device.',
		button: 'Power off',
		question: 'Power off the device?',
		warning: 'It stays off until it is turned on at the device.',
		danger: true,
	},
]

export default function Control({ client, onNetwork, onActivity, onEvent, onAccepted, provisional, unapplied }) {
	// The acts the device listed that this build knows, or null until it has listed them. A device older
	// than this build never answers, and looks like one offering nothing (CSCR).
	const [acts, setActs] = useState(null)
	const [confirming, setConfirming] = useState(null)
	const [asking, setAsking] = useState(null)
	const [refused, setRefused] = useState(null)
	const stream = useRef(null)
	const asked = useRef(null)

	// The stream lasts as long as this is mounted (CSCR).
	useEffect(() => {
		let live = true
		let opened = null
		client
			.power({
				onEvent: (event) => {
					if (!live) return
					onEvent?.(event)
					if (event.kind !== 'message') return
					const message = event.message
					if (message.type === 'acts') {
						setActs(ACTS.filter((each) => message.acts.includes(each.act)))
					} else if (message.type === 'accepted' && asked.current) {
						onAccepted?.(asked.current)
					} else if (message.type === 'refused') {
						asked.current = null
						setAsking(null)
						setRefused(message.reason)
					}
				},
				onClosed: () => {
					if (!live) return
					stream.current = null
					setActs(null)
				},
				onActivity,
			})
			.then((handle) => {
				if (live) stream.current = opened = handle
				else handle.close()
			})
			.catch((error) => live && onActivity?.('note', `could not open a power stream: ${error.message ?? error}`))
		return () => {
			live = false
			opened?.close()
			stream.current = null
		}
	}, [client, onActivity, onEvent, onAccepted])

	function ask(act) {
		setConfirming(null)
		setRefused(null)
		try {
			stream.current?.act(act)
			asked.current = act
			setAsking(act)
		} catch (error) {
			setRefused(error.message ?? String(error))
		}
	}

	const offered = acts ?? []
	const pending = offered.find((each) => each.act === confirming)

	return (
		<>
			<section>
				<h2>Network</h2>
				<button className="secondary" onClick={onNetwork}>
					Network settings
				</button>
			</section>
			{offered.length > 0 && (
				<section>
					<h2>Power</h2>
					{pending ? (
						<div className="bar-state" role="alertdialog" aria-label={pending.question}>
							<p>
								<strong>{pending.question}</strong>
							</p>
							{pending.warning && <p>{pending.warning}</p>}
							{provisional && <p>The network settings it is trying are not saved, and will be lost.</p>}
							{unapplied && <p>The network changes not applied will be lost.</p>}
							<div className="row">
								<button className={pending.danger ? 'danger-fill' : undefined} onClick={() => ask(pending.act)}>
									{pending.button}
								</button>
								<button className="secondary" onClick={() => setConfirming(null)}>
									Cancel
								</button>
							</div>
						</div>
					) : (
						<ul className="acts">
							{offered.map((each) => (
								<li key={each.act}>
									<div>
										<span className="name">{each.name}</span>
										<span className="muted">{each.hint}</span>
									</div>
									<button
										className={`secondary${each.danger ? ' danger' : ''}`}
										onClick={() => setConfirming(each.act)}
										disabled={asking !== null}
									>
										{each.button}
									</button>
								</li>
							))}
						</ul>
					)}
					{refused && <p className="why reason">{refused}</p>}
				</section>
			)}
		</>
	)
}
