// The Status screen of STAT: the landing a channel opens on, answering whether the device is on a
// network and how Tailscale and Tamanu stand, with the rest of the application under Advanced.

import { Fragment } from 'react'

import { IN_PROGRESS, NOT_WELL, WARNING, WELL } from './status.js'

/// The rows and Advanced, beneath the title the application gives the screen.
export default function Status({ rows, onChange, advanced, onAdvanced, onOpen }) {
	return (
		<>
			<section>
				<h2>Status</h2>
				<ul className="health">
					<Row row={rows.network} onAction={onChange} />
					{rows.tailscale && <Row row={rows.tailscale} />}
					<Row row={rows.tamanu} />
				</ul>
			</section>
			<div className={`advanced${advanced ? ' open' : ''}`}>
				<button className="summary" aria-expanded={advanced} onClick={() => onAdvanced(!advanced)}>
					Advanced
				</button>
				{advanced && (
					<div className="inside">
						<button className="secondary" onClick={() => onOpen('readings')}>
							All readings
						</button>
						<button className="secondary" onClick={() => onOpen('network')}>
							Network settings
						</button>
						<button className="secondary" onClick={() => onOpen('power')}>
							Power and battery
						</button>
					</div>
				)}
			</div>
		</>
	)
}

// The class each mark is drawn with, and what it says to a screen reader.
const MARKS = {
	[WELL]: ['good', 'Working'],
	[WARNING]: ['warn', 'Working, with a problem'],
	[NOT_WELL]: ['bad', 'Not working'],
	[IN_PROGRESS]: ['wait', 'In progress'],
}

function Row({ row, onAction }) {
	const [mark, said] = MARKS[row.mark]
	return (
		<li>
			<span className={`mark ${mark}`} role="img" aria-label={said}>
				{row.mark === IN_PROGRESS && <span className="spinner" aria-hidden="true" />}
			</span>
			<div className="body">
				<span className="name">{row.headline}</span>
				{row.reason && (
					<span className={`detail reason${row.mark === WARNING ? ' warn' : ''}`}>{row.reason}</span>
				)}
				{row.lines.map((parts, index) => (
					<Line key={index} parts={parts} />
				))}
			</div>
			{row.action && onAction && (
				<button className="secondary" onClick={onAction}>
					{row.action}
				</button>
			)}
		</li>
	)
}

function Line({ parts }) {
	const icons = parts.some((part) => part.icon)
	return (
		<span className={`detail${icons ? ' fact' : ''}`}>
			{parts.map((part, index) => (
				<Fragment key={index}>
					{index > 0 && <span className="sep">·</span>}
					{part.icon && <Way way={part.icon} />}
					{part.label && <span className="k">{part.label}</span>}
					{part.text}
				</Fragment>
			))}
		</span>
	)
}

// Each way's icon, drawn in the accent colour beside its short name.
const WAYS = {
	cable: (
		<>
			<path d="M10 8v1" />
			<path d="M14 8v1" />
			<path d="M18 8v1" />
			<path d="M19 17a2 2 0 00-1.765 1.059l-.47.882A2 2 0 0115 20H9a2 2 0 01-1.765-1.059l-.47-.882A2 2 0 005 17H4a2 2 0 01-2-2V6a2 2 0 012-2h16a2 2 0 012 2v9a2 2 0 01-2 2z" />
			<path d="M6 8v1" />
		</>
	),
	wifi: (
		<>
			<path d="M12 20h.01" />
			<path d="M2 8.82a15 15 0 0 1 20 0" />
			<path d="M5 12.859a10 10 0 0 1 14 0" />
			<path d="M8.5 16.429a5 5 0 0 1 7 0" />
		</>
	),
	hotspot: (
		<>
			<path d="M4.9 16.1C1 12.2 1 5.8 4.9 1.9" />
			<path d="M7.8 4.7a6.14 6.14 0 0 0-.8 7.5" />
			<circle cx="12" cy="9" r="2" />
			<path d="M16.2 4.8c2 2 2.26 5.11.8 7.47" />
			<path d="M19.1 1.9a9.96 9.96 0 0 1 0 14.1" />
			<path d="M9.5 18h5" />
			<path d="m8 22 4-11 4 11" />
		</>
	),
}

function Way({ way }) {
	return (
		<span className="way" aria-hidden="true">
			<svg
				xmlns="http://www.w3.org/2000/svg"
				viewBox="0 0 24 24"
				fill="none"
				stroke="currentColor"
				strokeWidth="2"
				strokeLinecap="round"
				strokeLinejoin="round"
			>
				{WAYS[way]}
			</svg>
		</span>
	)
}
