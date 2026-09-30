// Capturing a code with the camera, for provisioning several devices in one session without leaving
// and re-entering the application for each one (WEB, "Reading a QR code").

import { loadProtocol } from './protocol.js'
import { decode_qr as decodeQr } from './wasm/bliti_web.js'

export const cameraAvailable = () => Boolean(navigator.mediaDevices?.getUserMedia)

// Something that reads the texts of the QR codes in a video's current frame.
//
// The browser's own `BarcodeDetector` where it has one that reads QR codes, which is on phones and a
// few desktops. Everywhere else, which is most desktops, the frame is drawn to a canvas and decoded
// in the wasm module. Asked of the browser rather than assumed from `BarcodeDetector` existing, since
// a browser may expose it with no formats behind it.
async function detector() {
	if ('BarcodeDetector' in window) {
		const formats = await BarcodeDetector.getSupportedFormats().catch(() => [])
		if (formats.includes('qr_code')) {
			const native = new BarcodeDetector({ formats: ['qr_code'] })
			return async (video) => (await native.detect(video)).map((code) => code.rawValue)
		}
	}

	await loadProtocol()
	const canvas = document.createElement('canvas')
	const context = canvas.getContext('2d', { willReadFrequently: true })
	return async (video) => {
		const { videoWidth: width, videoHeight: height } = video
		if (!width || !height) return []
		if (canvas.width !== width || canvas.height !== height) Object.assign(canvas, { width, height })
		context.drawImage(video, 0, 0)
		return decodeQr(width, height, context.getImageData(0, 0, width, height).data)
	}
}

// Keep the camera focusing as the code moves, where the camera lets the page ask. A camera that
// settles focus once, on whatever was in view when it opened, blurs a code brought in close.
async function focusContinuously(stream) {
	const [track] = stream.getVideoTracks()
	if (!track?.getCapabilities?.().focusMode?.includes('continuous')) return
	await track.applyConstraints({ advanced: [{ focusMode: 'continuous' }] }).catch(() => {})
}

// Read codes from the camera until one is a QR code, until `signal` aborts, or until the camera
// fails. Resolves to the code, or to null where the scan was cancelled.
//
// Everything from acquiring the stream onwards is inside the `try`, so the camera is released on
// every path out: a detector this browser will not build, a `play()` that is interrupted,
// and cancellation all reach the same `finally`. A camera left running behind a page that has
// stopped showing it is the one failure here nobody would see.
//
// `onRejected` is told about a code that is not a bliti code: saying nothing is
// indistinguishable from a code the camera cannot read at all, which leaves the operator holding a
// code up to a camera that looks broken. Reported once per code rather than on every frame it
// stays in view.
export async function scan(video, { read, onRejected, signal }) {
	// Left to itself a phone browser opens the camera at 640 by 480, which leaves a module of a code
	// on an enclosure a pixel or two across at arm's length: too few for any decoder. The camera app
	// reads the same code from a stream several times larger, so ask for one.
	const stream = await navigator.mediaDevices.getUserMedia({
		video: { facingMode: 'environment', width: { ideal: 3840 }, height: { ideal: 2160 } },
	})

	const stop = () => {
		video.srcObject = null
		for (const track of stream.getTracks()) track.stop()
	}

	try {
		if (signal?.aborted) return null
		await focusContinuously(stream)

		const detect = await detector()
		if (signal?.aborted) return null
		video.srcObject = stream
		await video.play()

		let rejected = null
		while (video.srcObject && !signal?.aborted) {
			let texts = []
			try {
				texts = await detect(video)
			} catch {
				// A frame that cannot be read is not worth reporting; the next one is along shortly.
			}
			for (const text of texts) {
				try {
					return await read(text)
				} catch (error) {
					// A code that is not a bliti code does not stop the camera, because the next thing
					// in frame may well be one.
					if (text !== rejected) {
						rejected = text
						onRejected?.(error.message ?? String(error))
					}
				}
			}
			await new Promise((resolve) => setTimeout(resolve, 200))
		}
		return null
	} finally {
		stop()
	}
}
