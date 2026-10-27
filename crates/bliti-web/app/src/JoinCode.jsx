// The QR code a phone joins the hotspot by, drawn by the protocol module, so it appears once that has
// loaded (VIEW, SHARE).

import { useEffect, useState } from 'react'

import { loadProtocol } from './protocol.js'
import { hotspot_qr as hotspotQr } from './wasm/bliti_web.js'

export default function JoinCode({ ssid, passphrase, className }) {
	const [drawn, setDrawn] = useState(null)
	useEffect(() => {
		let current = true
		setDrawn(null)
		loadProtocol()
			.then(() => current && setDrawn({ svg: hotspotQr(ssid, passphrase) }))
			.catch((error) => current && setDrawn({ failed: String(error?.message ?? error) }))
		return () => {
			current = false
		}
	}, [ssid, passphrase])

	if (!drawn) return null
	if (drawn.failed) return <p className="bad">The join code cannot be drawn: {drawn.failed}</p>
	return (
		<div className={className ? `${className} join` : 'join'}>
			{/* The SVG is the protocol module's own drawing of the code, and carries no text. */}
			<div className="qr" role="img" aria-label={`QR code to join ${ssid}`} dangerouslySetInnerHTML={{ __html: drawn.svg }} />
			<div className="caption">Scan to join</div>
		</div>
	)
}
