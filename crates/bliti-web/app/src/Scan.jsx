// What a scan heard, read two ways (BLI-NSCR): by network, to pick one to join, and by access point,
// to site a new one. What the scan means is in scan.js; what the device can join is asked of
// capabilities.js.

import { useState } from 'react'

import { adapterName, joinWith, offersMember, radioBands, radios, wpsMethodsFor } from './capabilities.js'
import { blankSecurity } from './NetworkFields.jsx'
import { hiddenCount, networksOf, siting } from './scan.js'
import { bandName, securityName, widthName } from './wireless.js'

const UNJOINABLE = { open: 'Open', owe: 'OWE', wep: 'WEP' }

const aps = (count) => `${count} ${count === 1 ? 'AP' : 'APs'}`
const dbm = (signal) => (Number.isFinite(signal) ? `${signal} dBm` : '')
const named = (ssid) => ssid ?? 'Hidden network'
const where = (point) => [point.channel !== undefined && `ch ${point.channel}`, point.band && bandName(point.band)].filter(Boolean).join(', ')

function advertised(security) {
	return (security ?? []).map((each) => UNJOINABLE[each] ?? securityName(each)).join(', ')
}

export function ScanResults({ points, candidate, change, capabilities, onPicked, joinByWps }) {
	const [view, setView] = useState('networks')
	return (
		<div className="scan">
			<div className="row views" role="group" aria-label="Scan view">
				{[
					['networks', 'Networks'],
					['siting', 'Siting'],
				].map(([name, label]) => (
					<button key={name} type="button" className="secondary small" aria-pressed={view === name} onClick={() => setView(name)}>
						{label}
					</button>
				))}
			</div>
			{view === 'networks' ? (
				<Networks
					points={points}
					candidate={candidate}
					change={change}
					capabilities={capabilities}
					onPicked={onPicked}
					joinByWps={joinByWps}
				/>
			) : (
				<Siting points={points} capabilities={capabilities} />
			)}
		</div>
	)
}

/// The networks heard, by SSID, each opening onto the access points behind it, and onto joining it
/// by WPS where the device can join a named network that way.
function Networks({ points, candidate, change, capabilities, onPicked, joinByWps }) {
	const [hidden, setHidden] = useState(false)
	const [open, setOpen] = useState(() => new Set())
	const [wpsOpen, setWpsOpen] = useState(null)
	const networks = networksOf(points, { hidden })
	const unnamed = hiddenCount(points)
	const several = radios(capabilities).length > 1

	const pick = (network, kind) => {
		onPicked?.(network)
		change((held) => {
			const security = kind === held.security?.kind ? held.security : blankSecurity(kind, capabilities, held)
			if (network.ssid === null) {
				// Its name is not in the beacons, so it is left to type.
				const next = { ...held, security }
				return offersMember(capabilities, 'wireless', 'hidden', held) ? { ...next, hidden: true } : next
			}
			const next = {
				...held,
				ssid: network.ssid,
				label: !held.label || held.label === held.ssid ? network.ssid : held.label,
				security,
			}
			// Heard by name, so it is not hidden.
			return offersMember(capabilities, 'wireless', 'hidden', held) ? { ...next, hidden: false } : next
		})
	}

	const toggle = (key) =>
		setOpen((was) => {
			const next = new Set(was)
			if (next.has(key)) next.delete(key)
			else next.add(key)
			return next
		})

	return (
		<>
			{unnamed > 0 && (
				<label className="check">
					<input type="checkbox" checked={hidden} onChange={(event) => setHidden(event.target.checked)} /> Show hidden ({unnamed})
				</label>
			)}
			<ul className="networks">
				{networks.length === 0 && <li className="muted">No networks in range.</li>}
				{networks.map((network) => {
					const kind = joinWith(capabilities, network.security, candidate)
					const key = network.ssid ?? `hidden ${network.points[0].bssid}`
					const expanded = open.has(key)
					// A network the device cannot join is not joined by WPS either. On the adapter the candidate is
					// pinned to, where it is.
					const wps = joinByWps && kind && network.ssid !== null ? wpsMethodsFor(capabilities, network.ssid, candidate?.interface) : []
					return (
						<li key={key} className="network-row">
							<div className="line">
								<button type="button" className="secondary small" onClick={() => pick(network, kind)} disabled={!kind}>
									{named(network.ssid)}
								</button>
								<span className="muted">
									{kind ? securityName(kind) : `Cannot join: ${advertised(network.security)}`}
									{` · ${dbm(network.signal)}`}
								</span>
								<button
									type="button"
									className="link"
									aria-expanded={expanded}
									aria-label={`Access points of ${named(network.ssid)}`}
									onClick={() => toggle(key)}
								>
									{aps(network.count)}
								</button>
								{wps.length > 0 && (
									<button
										type="button"
										className="link"
										aria-expanded={wpsOpen === key}
										aria-label={`Join ${network.ssid} by WPS`}
										onClick={() => setWpsOpen(wpsOpen === key ? null : key)}
									>
										WPS
									</button>
								)}
							</div>
							{wpsOpen === key && wps.length > 0 && (
								<div className="row wps">
									{wps.map((method) => (
										<button
											key={method}
											type="button"
											className="secondary small"
											onClick={() => joinByWps(method, network.ssid, candidate?.interface ?? undefined)}
										>
											{method === 'pin' ? 'WPS PIN' : 'WPS button'}
										</button>
									))}
								</div>
							)}
							{expanded && (
								<ul className="points">
									{network.points.map((point) => (
										<li key={`${point.bssid}-${point.interface}`}>
											<span className="code">{point.bssid}</span>
											<span className="muted">
												{[where(point), advertised(point.security), dbm(point.signal), several && adapterName(capabilities, point.interface)]
													.filter(Boolean)
													.join(' · ')}
											</span>
										</li>
									))}
								</ul>
							)}
						</li>
					)
				})}
			</ul>
		</>
	)
}

/// Where the device's signal comes from, and which channels are taken on the bands its radios use.
function Siting({ points, capabilities }) {
	const read = siting(points, radioBands(capabilities))
	return (
		<div className="siting">
			<h3>By signal</h3>
			<ul className="points">
				{read.points.length === 0 && <li className="muted">Nothing heard.</li>}
				{read.points.map((point) => (
					<li key={`${point.bssid}-${point.interface}`}>
						<span>
							{dbm(point.signal)} <span className="muted">{named(point.ssid)}</span>
						</span>
						<span className="muted">
							{[where(point), point['channel-width'] && widthName(point['channel-width']), adapterName(capabilities, point.interface)]
								.filter(Boolean)
								.join(' · ')}
						</span>
					</li>
				))}
			</ul>
			{read.bands.map(({ band, channels }) => (
				<div key={band} className="band" aria-label={`Channels taken on ${bandName(band)}`}>
					<h3>{bandName(band)} taken</h3>
					<ul className="points">
						{channels.length === 0 && <li className="muted">None heard.</li>}
						{channels.map((each) => (
							<li key={each.channel}>
								<span>ch {each.channel}</span>
								<span className="muted">{[aps(each.count), each.width && widthName(each.width)].filter(Boolean).join(', ')}</span>
							</li>
						))}
					</ul>
				</div>
			))}
		</div>
	)
}
