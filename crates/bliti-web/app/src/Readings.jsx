// The diagnostics view of BLI-SYS: a tile per reading, with the detail behind a tap.
//
// Nothing here matches on a reading's name. Every tile is built from what the reading says about
// itself, so a device that has gained a reading shows it without this file changing. Where a reading
// is recognised it may be treated specially, but recognition is never a precondition for display.

import { useState } from 'react'

import {
	formatValue,
	groupReadings,
	isTrouble,
	opposedPair,
	polyline,
	scaleOf,
	seriesOf,
} from './readings.js'

export default function Readings({ readings, window: history }) {
	if (readings.length === 0) {
		return <p className="muted">Nothing reported yet.</p>
	}
	return (
		<div className="tiles">
			{groupReadings(readings).map((entry) => (
				<Tile key={entry.key} entry={entry} history={history} />
			))}
		</div>
	)
}

/// One tile. The face carries a label and the headline value and nothing else; everything else is
/// revealed by tapping. Trouble colours the number rather than adding an element to the face.
function Tile({ entry, history }) {
	const [open, setOpen] = useState(false)
	const pair = opposedPair(entry)
	const wide = entry.readings.length > 1
	const trouble = entry.readings.some(isTrouble)

	return (
		<button
			type="button"
			className={`tile${wide ? ' wide' : ''}${open ? ' expanded' : ''}`}
			aria-expanded={open}
			onClick={() => setOpen(!open)}
		>
			<div className="label">{entry.label}</div>
			<Face entry={entry} />
			{open && (
				<div className="detail">
					{pair ? (
						<Mirrored inbound={pair[0]} outbound={pair[1]} history={history} />
					) : (
						entry.readings.map((reading) => (
							<Revealed key={reading.name} reading={reading} history={history} />
						))
					)}
				</div>
			)}
			{!open && trouble && <span className="sr-only">needs attention</span>}
		</button>
	)
}

/// The face: the headline, or an account of why there is none.
function Face({ entry }) {
	if (entry.readings.length === 1) {
		const [reading] = entry.readings
		return <div className={`value${isTrouble(reading) ? ` ${tone(reading)}` : ''}`}>{headline(reading)}</div>
	}
	// A group shows each member compactly rather than picking one to stand for the rest.
	return (
		<div className="value small">
			{entry.readings.map((reading) => (
				<span key={reading.name} className={isTrouble(reading) ? tone(reading) : undefined}>
					<span className="part">{reading.label}</span> {headline(reading)}
				</span>
			))}
		</div>
	)
}

function headline(reading) {
	if (reading.error) return 'unavailable'
	const shown = formatValue(reading.value)
	// A value of a kind this build does not know. The label has already said what the reading is, and
	// inventing a rendering for a unit we cannot interpret would be worse than saying so.
	return shown ?? 'not understood'
}

function tone(reading) {
	return reading.state === 'fault' ? 'bad' : 'warn'
}

/// What a tap reveals for one reading: why it failed, or its scale, history, detail and note.
function Revealed({ reading, history }) {
	const scale = scaleOf(reading.value)
	const series = seriesOf(history, reading.name)

	return (
		<div className="revealed">
			{reading.error && (
				<p className="bad">
					{reading.label} could not be read: {reading.error}
				</p>
			)}

			{scale !== null && (
				<div className={`bar${isTrouble(reading) ? ' warn' : ''}`}>
					<span style={{ width: `${scale * 100}%` }} />
					{reading.limits?.map((limit) => (
						<i key={limit.label} className="trip" style={{ left: `${mark(limit, reading)}%` }} />
					))}
				</div>
			)}

			{series.length > 1 && <Sparkline points={series} />}

			{(reading.detail?.length > 0 || reading.limits?.length > 0) && (
				<dl>
					{reading.detail?.map((each, index) => (
						<Entry key={`${each.label}-${index}`} label={each.label} value={each.value} />
					))}
					{reading.limits?.map((limit) => (
						<Entry
							key={`limit-${limit.label}`}
							label={limit.label}
							value={{ kind: 'quantity', number: limit.at, unit: unitOf(reading.value) }}
						/>
					))}
				</dl>
			)}

			{reading.note && <p className="note">{reading.note}</p>}
		</div>
	)
}

function Entry({ label, value }) {
	return (
		<>
			<dt>{label}</dt>
			<dd>{formatValue(value) ?? 'not understood'}</dd>
		</>
	)
}

function unitOf(value) {
	return value?.kind === 'quantity' ? value.unit : ''
}

/// Where a limit sits along the reading's scale, as a percentage of the bar.
function mark(limit, reading) {
	const value = reading.value
	if (value?.kind === 'quantity' && value.max > 0) {
		return Math.min(100, Math.max(0, (limit.at / value.max) * 100))
	}
	if (value?.kind === 'fraction') return Math.min(100, Math.max(0, limit.at * 100))
	return 0
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

/// Two opposed flows about one time axis, one above and one reflected below.
///
/// Each side is scaled to its own peak, and both peaks are stated: the directions routinely differ by
/// an order of magnitude, and sharing a scale would flatten the quieter one to a line, losing the
/// shape that makes a graph worth drawing at all.
function Mirrored({ inbound, outbound, history }) {
	const width = 240
	const half = 34
	const up = polyline(seriesOf(history, outbound.name), { width, height: half, flip: false })
	const down = polyline(seriesOf(history, inbound.name), { width, height: half, flip: true })

	return (
		<div className="revealed">
			<div className="mirror">
				<span className="axis-label up">
					{outbound.label} · peak {trimPeak(up.peak, outbound)}
				</span>
				<svg
					viewBox={`0 0 ${width} ${half * 2}`}
					preserveAspectRatio="none"
					aria-hidden="true"
					className="spark"
				>
					<polyline
						fill="none"
						stroke="var(--good)"
						strokeWidth="1.5"
						points={up.points}
						transform={`translate(0,0)`}
					/>
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
					{inbound.label} · peak {trimPeak(down.peak, inbound)}
				</span>
			</div>
			<p className="note">Each direction is drawn to its own scale.</p>
		</div>
	)
}

function trimPeak(peak, reading) {
	const unit = unitOf(reading.value)
	return `${Math.round(peak * 100) / 100} ${unit}`.trim()
}
