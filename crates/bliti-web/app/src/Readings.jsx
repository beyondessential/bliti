// The diagnostics view of NFO: a tile per reading, with the detail behind a tap.
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

export default function Readings({ readings, window: live, history }) {
	if (readings.length === 0) {
		return <p className="muted">Nothing reported yet.</p>
	}
	return (
		<div className="tiles">
			{groupReadings(readings).map((entry) => (
				<Tile key={entry.key} entry={entry} live={live} history={history} />
			))}
		</div>
	)
}

/// Whether a reading has anything behind its headline. A tile with nothing to reveal is not a tap
/// target: offering one that does nothing teaches an operator that tapping is not worth trying.
function hasMore(reading, live, history) {
	return Boolean(
		reading.error ||
			reading.note ||
			reading.detail?.length ||
			reading.limits?.length ||
			scaleOf(reading.value) !== null ||
			(reading.graph !== false && seriesOf(history, live, reading.name).length > 1),
	)
}

/// A headline long enough that half a row would wrap it. Text runs long where a number does not, so
/// this keeps a hostname or a board name on one line without giving every tile the full width.
const LONG_HEADLINE = 12

function isWide(entry) {
	if (entry.readings.length > 1) return true
	const shown = headline(entry.readings[0])
	return (shown?.length ?? 0) > LONG_HEADLINE
}

/// One tile. The face carries a label and the headline value and nothing else; everything else is
/// revealed by tapping. Trouble colours the number rather than adding an element to the face.
function Tile({ entry, live, history }) {
	const [open, setOpen] = useState(false)
	const pair = opposedPair(entry)
	const wide = isWide(entry)
	const trouble = entry.readings.some(isTrouble)
	const more = entry.readings.some((reading) => hasMore(reading, live, history))

	const className = `tile${wide || open ? ' wide' : ''}${open ? ' expanded' : ''}${more ? '' : ' flat'}`
	const body = (
		<>
			<div className="label">{entry.label}</div>
			<Face entry={entry} />
			{open && (
				<div className="detail">
					{pair ? (
						<Mirrored inbound={pair[0]} outbound={pair[1]} live={live} history={history} />
					) : (
						entry.readings.map((reading) => (
							<Revealed key={reading.name} reading={reading} live={live} history={history} />
						))
					)}
				</div>
			)}
			{trouble && <span className="sr-only">needs attention</span>}
		</>
	)

	if (!more) return <div className={className}>{body}</div>

	return (
		<button
			type="button"
			className={className}
			aria-expanded={open}
			onClick={() => setOpen(!open)}
		>
			{body}
		</button>
	)
}

/// The face: the headline, or an account of why there is none.
function Face({ entry }) {
	if (entry.readings.length === 1) {
		const [reading] = entry.readings
		const shown = headline(reading)
		// Nothing renderable: the label is the whole of what this reading says, and it carries no face
		// value at all rather than a placeholder standing in for one.
		if (shown === null) return null
		return <div className={`value${isTrouble(reading) ? ` ${tone(reading)}` : ''}`}>{shown}</div>
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
	// A value of a kind this build does not know: the reading is treated as carrying none, and renders
	// as its label alone (NFO). A token in its place would claim to have read something we did not.
	return formatValue(reading.value)
}

function tone(reading) {
	return reading.state === 'fault' ? 'bad' : 'warn'
}

/// What a tap reveals for one reading: why it failed, or its scale, history, detail and note.
function Revealed({ reading, live, history }) {
	const scale = scaleOf(reading.value)
	const series = seriesOf(history, live, reading.name)

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

			{reading.graph !== false && series.length > 1 && <Sparkline points={series} />}

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
function Mirrored({ inbound, outbound, live, history }) {
	const width = 240
	const half = 34
	const up = polyline(seriesOf(history, live, outbound.name), { width, height: half, flip: false })
	const down = polyline(seriesOf(history, live, inbound.name), { width, height: half, flip: true })

	return (
		<div className="revealed">
			<div className="mirror">
				<span className="axis-label up">
					<i className="key out" /> {outbound.label} · peak {trimPeak(up.peak, outbound)}
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
					<i className="key in" /> {inbound.label} · peak {trimPeak(down.peak, inbound)}
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
