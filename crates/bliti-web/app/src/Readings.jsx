// The device view of VIEW: a header naming the device, then a tile per reading in our own fixed
// order, with the detail behind a tap.
//
// Nothing here is blanked for a device newer than this build. What our application recognises it
// renders bespoke, in the order and wording it prefers; what it does not it renders generically, from
// the entry's own name, kind and traits, appended after everything recognised (VIEW).

import { useState } from 'react'

import {
	HEADER,
	IN_REVEAL,
	TILE_ORDER,
	formatValue,
	hasValue,
	isTrouble,
	labelOf,
	numberOf,
	polyline,
	qualifierOf,
	reasonOf,
	scaleOf,
	seriesKey,
	statusOf,
} from './readings.js'

export default function Readings({ entries, history }) {
	if (entries.length === 0) {
		return <p className="muted">Nothing reported yet.</p>
	}

	const byName = new Map()
	for (const entry of entries) {
		if (!byName.has(entry.name)) byName.set(entry.name, [])
		byName.get(entry.name).push(entry)
	}
	const single = (name) => byName.get(name)?.[0]

	// The recognised tiles, in our fixed order, skipping any the device did not send.
	const tiles = []
	for (const name of TILE_ORDER) {
		if (!byName.has(name)) continue
		tiles.push(renderTile(name, byName, history))
	}

	// Everything else it does not recognise, appended after the recognised. Header facts and
	// in-reveal entries are not tiles of their own.
	for (const [name, group] of byName) {
		if (HEADER.includes(name) || TILE_ORDER.includes(name) || IN_REVEAL.has(name)) continue
		tiles.push(<GenericTile key={name} label={labelOf(name)} entries={group} history={history} />)
	}

	return (
		<>
			<Header single={single} />
			<div className="tiles">{tiles}</div>
		</>
	)
}

/// The header names the device from the facts that say which device this is. They get no tiles of
/// their own (VIEW).
function Header({ single }) {
	const hostname = single('hostname')
	const board = single('board')
	const revision = single('board-revision')
	const os = single('os')
	const kernel = single('kernel')
	if (!hostname && !board && !os && !kernel) return null

	const sub = [
		board && [textOf(board), revision && textOf(revision)].filter(Boolean).join(' '),
		os && textOf(os),
		kernel && textOf(kernel),
	]
		.filter(Boolean)
		.join(' · ')

	return (
		<div className="device">
			<div className="name">{hostname ? textOf(hostname) : 'Device'}</div>
			{sub && <div className="sub muted">{sub}</div>}
		</div>
	)
}

function textOf(entry) {
	return hasValue(entry) ? String(entry.value) : ''
}

/// Route each recognised name to the tile that knows how to aggregate it.
function renderTile(name, byName, history) {
	const group = byName.get(name)
	switch (name) {
		case 'network-address':
			return <AddressTile key={name} addresses={group} />
		case 'filesystem-usage':
			return (
				<StorageTile key={name} filesystems={group} totals={byName.get('filesystem-total') ?? []} />
			)
		case 'network-throughput':
			return <NetworkTile key={name} throughputs={group} history={history} />
		case 'temperature':
			return <TemperatureTile key={name} sensors={group} history={history} />
		case 'memory-usage':
			return (
				<SimpleTile
					key={name}
					entry={group[0]}
					history={history}
					reveal={[byName.get('memory-total')?.[0]]}
				/>
			)
		case 'cpu-frequency':
			return (
				<SimpleTile
					key={name}
					entry={group[0]}
					history={history}
					total={numberOf(byName.get('cpu-frequency-max')?.[0])}
					reveal={[byName.get('cpu-frequency-max')?.[0]]}
				/>
			)
		case 'battery-charge':
			return (
				<BatteryTile
					key={name}
					charges={group}
					voltages={byName.get('battery-voltage') ?? []}
					directions={byName.get('battery-direction') ?? []}
					history={history}
				/>
			)
		default:
			return <SimpleTile key={name} entry={group[0]} history={history} />
	}
}

// A tile's shell: the face carries a label and the headline and nothing else; the rest is revealed by
// tapping, and an opened tile takes the whole row. A tile with nothing to reveal is not a tap target.
function Tile({ label, wide, more, tone, children, face }) {
	const [open, setOpen] = useState(false)
	const className = `tile${wide || open ? ' wide' : ''}${open ? ' expanded' : ''}${more ? '' : ' flat'}`
	const body = (
		<>
			<div className="label">{label}</div>
			<div className={`value${tone ? ` ${tone}` : ''}`}>{face}</div>
			{open && <div className="detail">{children}</div>}
		</>
	)
	if (!more) return <div className={className}>{body}</div>
	return (
		<button type="button" className={className} aria-expanded={open} onClick={() => setOpen(!open)}>
			{body}
		</button>
	)
}

/// The tone a face is coloured by its status. A passed face carries no colour (VIEW).
function tone(entry) {
	switch (statusOf(entry)) {
		case 'warning':
			return 'warn'
		case 'failed':
		case 'broken':
			return 'bad'
		default:
			return ''
	}
}

/// The headline for an entry with a value, or a marker for one without. Skipped and broken both have
/// no value and are distinguished: skipped shows a dash, broken shows it is unavailable (VIEW).
function headline(entry) {
	if (hasValue(entry)) return formatValue(entry)
	return statusOf(entry) === 'broken' ? 'unavailable' : '—'
}

/// A single-instance reading: cpu, memory, power, battery and the like. Its reveal carries its scale,
/// its history, its reason, and any entries VIEW folds into it.
function SimpleTile({ entry, history, total = null, reveal = [] }) {
	const revealed = reveal.filter(Boolean)
	const scale = scaleOf(entry, total)
	const series = history.get(seriesKey(entry)) ?? []
	const graphable = !entry.fact && series.length > 1
	const more =
		reasonOf(entry) || scale !== null || graphable || revealed.length > 0

	return (
		<Tile label={labelOf(entry.name)} wide={isLong(headline(entry))} more={more} tone={tone(entry)} face={headline(entry)}>
			<Reveal entry={entry} scale={scale} series={graphable ? series : null} />
			{revealed.map((sub) => (
				<Folded key={sub.name} entry={sub} />
			))}
		</Tile>
	)
}

/// What a tap reveals for one reading: its reason, its scale, and its history.
function Reveal({ entry, scale, series }) {
	return (
		<div className="revealed">
			{reasonOf(entry) && <p className={statusOf(entry) === 'broken' ? 'bad' : 'note'}>{reasonOf(entry)}</p>}
			{scale !== null && (
				<div className={`bar${isTrouble(entry) ? ' warn' : ''}`}>
					<span style={{ width: `${scale * 100}%` }} />
					{limitsOf(entry).map((limit) => (
						<i key={limit.label} className="trip" style={{ left: `${markAt(limit, entry, scale)}%` }} />
					))}
				</div>
			)}
			{series && <Sparkline points={series} />}
		</div>
	)
}

/// An entry folded into another's reveal: a labelled figure, e.g. the memory total or the cell
/// voltage. It carries no tile of its own (VIEW).
function Folded({ entry }) {
	return (
		<div className="revealed">
			<dl>
				<dt>{labelOf(entry.name)}</dt>
				<dd>{foldedValue(entry)}</dd>
			</dl>
		</div>
	)
}

/// A folded entry's figure, or its reason where it has no value: a skipped voltage says why it is
/// not there rather than showing a bare dash (VIEW).
function foldedValue(entry) {
	return hasValue(entry) ? formatValue(entry) : (reasonOf(entry) ?? '—')
}

/// Which battery a reading is about.
function batteryName(entry) {
	return entry.traits?.battery?.name ?? ''
}

/// Battery: headline the cell named `built-in` where the device reports one and the first by name
/// otherwise, pair each battery's voltage and direction with its own charge, and show every battery
/// in the reveal (VIEW).
///
/// A device with one battery is the ordinary case and keeps the plain reading tile: its scale, its
/// history, and the voltage and direction folded in beneath.
function BatteryTile({ charges, voltages, directions, history }) {
	const ordered = [...charges].sort((a, b) => batteryName(a).localeCompare(batteryName(b)))
	const headlined = ordered.find((entry) => batteryName(entry) === 'built-in') ?? ordered[0]
	if (!headlined) return null

	const partner = (entries, battery) =>
		entries.find((entry) => batteryName(entry) === batteryName(battery))

	if (ordered.length === 1) {
		return (
			<SimpleTile
				entry={headlined}
				history={history}
				reveal={[partner(voltages, headlined), partner(directions, headlined)]}
			/>
		)
	}

	return (
		<Tile
			label={labelOf('battery-charge')}
			more
			tone={tone(headlined)}
			face={headline(headlined)}
		>
			{ordered.map((entry) => {
				const volts = partner(voltages, entry)
				const way = partner(directions, entry)
				const scale = scaleOf(entry)
				return (
					<div className="revealed" key={batteryName(entry)}>
						<dl>
							<Line
								label={batteryName(entry)}
								value={hasValue(entry) ? formatValue(entry) : headline(entry)}
							/>
							{volts && <Line label={labelOf(volts.name)} value={foldedValue(volts)} />}
							{way && <Line label={labelOf(way.name)} value={foldedValue(way)} />}
						</dl>
						{scale !== null && (
							<div className={`bar${isTrouble(entry) ? ' warn' : ''}`}>
								<span style={{ width: `${scale * 100}%` }} />
							</div>
						)}
					</div>
				)
			})}
		</Tile>
	)
}

/// Address: headline the default-route address together with the overlay address, and both where an
/// interface holds a v4 and a v6; show every address in the reveal (VIEW).
function AddressTile({ addresses }) {
	const isDefault = (entry) => entry.traits?.interface?.route === 'default'
	const isOverlay = (entry) => Boolean(entry.traits?.interface?.overlay)
	const headlined = addresses.filter((entry) => isDefault(entry) || isOverlay(entry))
	const shown = headlined.length > 0 ? headlined : addresses.slice(0, 1)

	return (
		<Tile
			label={labelOf('network-address')}
			wide
			more={addresses.length > shown.length}
			face={
				<span className="small">
					{shown.map((entry) => (
						<span key={interfaceName(entry) + entry.value}>
							<span className="part">{qualifierOf(entry)}</span> {String(entry.value)}
						</span>
					))}
				</span>
			}
		>
			<div className="revealed">
				<dl>
					{addresses.map((entry) => (
						<Line key={interfaceName(entry) + entry.value} label={qualifierOf(entry)} value={String(entry.value)} />
					))}
				</dl>
			</div>
		</Tile>
	)
}

function interfaceName(entry) {
	const iface = entry.traits?.interface
	return typeof iface === 'object' ? (iface?.name ?? '') : String(iface ?? '')
}

/// Storage: headline the fullest filesystem that is not a boot partition, and show each filesystem in
/// the reveal against its own total. No history is drawn (VIEW).
function StorageTile({ filesystems, totals }) {
	const totalFor = (entry) => {
		const mount = entry.traits?.filesystem?.mount
		const match = totals.find((t) => t.traits?.filesystem?.mount === mount)
		return match ? numberOf(match) : null
	}
	const isBoot = (entry) => entry.traits?.filesystem?.role === 'boot'
	const nonBoot = filesystems.filter((entry) => !isBoot(entry) && hasValue(entry))
	const pool = nonBoot.length > 0 ? nonBoot : filesystems.filter(hasValue)
	const fullest = pool.reduce((worst, entry) => (numberOf(entry) > numberOf(worst) ? entry : worst), pool[0])

	return (
		<Tile
			label={labelOf('filesystem-usage')}
			more={filesystems.length > 0}
			tone={fullest ? tone(fullest) : ''}
			face={fullest ? headline(fullest) : '—'}
		>
			{filesystems.map((entry) => (
				<div className="revealed" key={mountOf(entry)}>
					<dl>
						<Line label={mountOf(entry)} value={hasValue(entry) ? formatValue(entry) : '—'} />
					</dl>
					{scaleOf(entry, totalFor(entry)) !== null && (
						<div className={`bar${isTrouble(entry) ? ' warn' : ''}`}>
							<span style={{ width: `${scaleOf(entry, totalFor(entry)) * 100}%` }} />
						</div>
					)}
				</div>
			))}
			<p className="note">Filesystem use does not move fast enough for a graph to say anything.</p>
		</Tile>
	)
}

function mountOf(entry) {
	return entry.traits?.filesystem?.mount ?? qualifierOf(entry)
}

/// Network: headline the sum of every direction and interface, and show each interface in the reveal
/// as a mirrored graph, one direction above the axis and one below (VIEW).
function NetworkTile({ throughputs, history }) {
	const total = throughputs.filter(hasValue).reduce((sum, entry) => sum + (numberOf(entry) ?? 0), 0)

	// Group by interface, keeping the in and out of each together.
	const interfaces = new Map()
	for (const entry of throughputs) {
		const name = interfaceName(entry)
		if (!interfaces.has(name)) interfaces.set(name, {})
		interfaces.get(name)[entry.traits?.direction] = entry
	}

	return (
		<Tile
			label={labelOf('network-throughput')}
			more={throughputs.length > 0}
			face={formatValue({ kind: 'quantity', value: total, unit: 'bytes/second' })}
		>
			{[...interfaces].map(([name, pair]) => (
				<Interface key={name} name={name} pair={pair} history={history} />
			))}
		</Tile>
	)
}

/// One interface's throughput: a mirrored graph where both directions read, or the per-reading reveal
/// with its reason where one direction is broken (VIEW).
function Interface({ name, pair, history }) {
	const { in: inbound, out: outbound } = pair
	const drawable = inbound && outbound && hasValue(inbound) && hasValue(outbound)

	return (
		<div className="revealed">
			<dl>
				<Line label={name} value={summary(pair)} />
			</dl>
			{drawable ? (
				<Mirror inbound={inbound} outbound={outbound} history={history} />
			) : (
				[inbound, outbound].filter(Boolean).map((entry) => (
					reasonOf(entry) && (
						<p key={entry.traits?.direction} className="bad">
							{entry.traits?.direction} could not be read: {reasonOf(entry)}
						</p>
					)
				))
			)}
		</div>
	)
}

function summary(pair) {
	return ['in', 'out']
		.map((direction) => {
			const entry = pair[direction]
			if (!entry) return null
			return hasValue(entry) ? formatValue(entry) : 'unavailable'
		})
		.filter(Boolean)
		.join(' · ')
}

/// Temperature: headline the cpu sensor, and show every sensor in the reveal, each with its own scale
/// and limits (VIEW).
function TemperatureTile({ sensors, history }) {
	const cpu = sensors.find((entry) => entry.traits?.sensor === 'cpu') ?? sensors[0]
	return (
		<Tile
			label={labelOf('temperature')}
			more={sensors.length > 0}
			tone={cpu ? tone(cpu) : ''}
			face={cpu ? headline(cpu) : '—'}
		>
			{sensors.map((entry) => {
				const series = history.get(seriesKey(entry)) ?? []
				return (
					<div className="revealed" key={sensorName(entry)}>
						<dl>
							<Line label={sensorName(entry)} value={hasValue(entry) ? formatValue(entry) : (reasonOf(entry) ?? '—')} />
						</dl>
						<Reveal entry={entry} scale={scaleOf(entry)} series={series.length > 1 ? series : null} />
					</div>
				)
			})}
		</Tile>
	)
}

function sensorName(entry) {
	return typeof entry.traits?.sensor === 'string' ? entry.traits.sensor : qualifierOf(entry)
}

/// An entry this build does not recognise, rendered from its own name, traits and value. A fact gets
/// no scale and no history; a reading still colours by status and still accumulates a history (VIEW).
function GenericTile({ label, entries, history }) {
	return entries.map((entry) => {
		const series = history.get(seriesKey(entry)) ?? []
		const graphable = !entry.fact && series.length > 1
		const scale = scaleOf(entry)
		const qualifier = qualifierOf(entry)
		const face = hasValue(entry) ? formatValue(entry) : headline(entry)
		const more = reasonOf(entry) || scale !== null || graphable

		return (
			<Tile
				key={label + qualifier}
				label={qualifier ? `${label} · ${qualifier}` : label}
				wide={isLong(String(face))}
				more={more}
				tone={tone(entry)}
				face={face}
			>
				<Reveal entry={entry} scale={scale} series={graphable ? series : null} />
			</Tile>
		)
	})
}

function Line({ label, value }) {
	return (
		<>
			<dt>{label}</dt>
			<dd>{value}</dd>
		</>
	)
}

function limitsOf(entry) {
	const limits = entry.traits?.limits
	return Array.isArray(limits) ? limits : []
}

/// Where a limit sits along the drawn scale, as a percentage of the bar.
function markAt(limit, entry, scale) {
	const value = numberOf(entry)
	if (value === null || scale === null || value === 0) return 0
	// The bar is drawn to the same top the scale used, so the limit maps by the same ratio.
	const top = value / scale
	return top > 0 ? Math.min(100, Math.max(0, (limit.at / top) * 100)) : 0
}

const LONG_HEADLINE = 12

function isLong(text) {
	return typeof text === 'string' && text.length > LONG_HEADLINE
}

function Sparkline({ points }) {
	const width = 240
	const height = 32
	const line = polyline(points, { width, height })
	return (
		<svg className="spark" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
			<polyline fill="none" stroke="var(--accent)" strokeWidth="1.5" points={line.points} />
		</svg>
	)
}

/// Two opposed flows about one time axis, one above and one reflected below, each scaled to its own
/// peak with the peak stated (VIEW).
function Mirror({ inbound, outbound, history }) {
	const width = 240
	const half = 34
	const up = polyline(history.get(seriesKey(outbound)) ?? [], { width, height: half, flip: false })
	const down = polyline(history.get(seriesKey(inbound)) ?? [], { width, height: half, flip: true })

	return (
		<div className="mirror">
			<span className="axis-label up">
				<i className="key out" /> Out · peak {peak(up.peak)}
			</span>
			<svg viewBox={`0 0 ${width} ${half * 2}`} preserveAspectRatio="none" aria-hidden="true" className="spark">
				<polyline fill="none" stroke="var(--good)" strokeWidth="1.5" points={up.points} />
				<line x1="0" y1={half} x2={width} y2={half} stroke="var(--line)" strokeWidth="1" />
				<polyline
					fill="none"
					stroke="var(--accent)"
					strokeWidth="1.5"
					points={down.points}
					transform={`translate(0,${half})`}
				/>
			</svg>
			<span className="axis-label down">
				<i className="key in" /> In · peak {peak(down.peak)}
			</span>
		</div>
	)
}

function peak(number) {
	return formatValue({ kind: 'quantity', value: number, unit: 'bytes/second' })
}
