// Making the sheet of labels and handing it to the browser to download (PSHT, "The sheet"). The
// sheet itself is drawn in the wasm module, from the labels alone.

import { useCallback, useEffect, useRef, useState } from 'react'

import { loadProtocol } from '../protocol.js'
import { sheet_fill as sheetFill, sheet_pdf as sheetPdf } from '../wasm/bliti_web.js'
import { civilDate } from './state.js'

/// Whether the wasm module is loaded, which how full a sheet is needs to be worded.
export function useSheetReady() {
	const [ready, setReady] = useState(false)
	useEffect(() => {
		let wanted = true
		loadProtocol()
			.then(() => wanted && setReady(true))
			.catch(() => {})
		return () => {
			wanted = false
		}
	}, [])
	return ready
}

/// The action that downloads the sheet of `count` labels: how full it is (PSHT).
export function downloadText(count, ready) {
	return ready ? `Download sheet (${sheetFill(count)})` : 'Download sheet'
}

/// Making sheets one at a time (PSHT): `make(labels, onMade)` downloads the sheet of `labels` and
/// hands them to `onMade` once it has, and does nothing while another is being made. `making` holds from
/// when one is asked for until it is ready, and `failed` says why the last one could not be made.
export function useSheet() {
	const ready = useSheetReady()
	const [making, setMaking] = useState(false)
	const [failed, setFailed] = useState(null)
	const held = useRef(false)
	const make = useCallback(async (labels, onMade) => {
		if (held.current) return
		held.current = true
		setMaking(true)
		setFailed(null)
		try {
			await download(labels)
			onMade(labels)
		} catch (error) {
			setFailed(error.message ?? String(error))
		} finally {
			held.current = false
			setMaking(false)
		}
	}, [])
	return { ready, making, failed, make }
}

/// The PDF of `labels`, in order, and the browser asked to save it.
export async function download(labels, now = new Date()) {
	await loadProtocol()
	// Drawing the sheet holds the page, so whatever says it is being made is painted first.
	await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)))
	if (__TEST_SEAM__ && window.__blitiSheetHold) await window.__blitiSheetHold
	const bytes = sheetPdf(JSON.stringify(labels.map(({ payload, version }) => ({ payload, version }))))
	const url = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }))
	const link = document.createElement('a')
	link.href = url
	link.download = `iti-labels-${civilDate(now)}.pdf`
	document.body.append(link)
	link.click()
	link.remove()
	setTimeout(() => URL.revokeObjectURL(url), 10_000)
}
