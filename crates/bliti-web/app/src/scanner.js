// Capturing a code with the camera, for provisioning several devices in one session without leaving
// and re-entering the application for each one (WEB, "Reading a QR code").

export const cameraAvailable = () => 'BarcodeDetector' in window

// Read codes from the camera until one is a QR code, until `signal` aborts, or until the camera
// fails. Resolves to the code, or to null where the scan was cancelled.
//
// Everything from acquiring the stream onwards is inside the `try`, so the camera is released on
// every path out: a `BarcodeDetector` this browser will not build, a `play()` that is interrupted,
// and cancellation all reach the same `finally`. A camera left running behind a page that has
// stopped showing it is the one failure here nobody would see.
//
// `onRejected` is told about a code that is not a bliti code: saying nothing is
// indistinguishable from a code the camera cannot read at all, which leaves the operator holding a
// code up to a camera that looks broken. Reported once per code rather than on every frame it
// stays in view.
export async function scan(video, { read, onRejected, signal }) {
	const stream = await navigator.mediaDevices.getUserMedia({
		video: { facingMode: 'environment' },
	})

	const stop = () => {
		video.srcObject = null
		for (const track of stream.getTracks()) track.stop()
	}

	try {
		if (signal?.aborted) return null

		const detector = new BarcodeDetector({ formats: ['qr_code'] })
		video.srcObject = stream
		await video.play()

		let rejected = null
		while (video.srcObject && !signal?.aborted) {
			let codes = []
			try {
				codes = await detector.detect(video)
			} catch {
				// A frame that cannot be read is not worth reporting; the next one is along shortly.
			}
			for (const code of codes) {
				try {
					return await read(code.rawValue)
				} catch (error) {
					// A code that is not a bliti code does not stop the camera, because the next thing
					// in frame may well be one.
					if (code.rawValue !== rejected) {
						rejected = code.rawValue
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
