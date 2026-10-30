// Making the sheet of labels and handing it to the browser to download (PSHT, "The sheet"). The
// sheet itself is drawn in the wasm module, from the labels alone.

import { useEffect, useState } from 'react'

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

/// The PDF of `labels`, in order, and the browser asked to save it.
export async function download(labels, now = new Date()) {
	await loadProtocol()
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
