// Capturing a code with the camera, for provisioning several devices in one session without leaving
// and re-entering the application for each one (BLI-WEB, "Reading a sticker").

export const cameraAvailable = () => 'BarcodeDetector' in window

// Read codes from the camera until one is a sticker, or until `stop()` is called. `onRejected` is
// told about a code that is not a bliti sticker: saying nothing is indistinguishable from a code the
// camera cannot read at all, which leaves the operator holding a sticker up to a camera that looks
// broken. Reported once per code rather than on every frame it stays in view.
export async function scan(video, { read, onRejected }) {
	const stream = await navigator.mediaDevices.getUserMedia({
		video: { facingMode: 'environment' },
	})

	const detector = new BarcodeDetector({ formats: ['qr_code'] })
	let rejected = null
	video.srcObject = stream
	await video.play()

	const stop = () => {
		video.srcObject = null
		for (const track of stream.getTracks()) track.stop()
	}

	try {
		while (video.srcObject) {
			let codes = []
			try {
				codes = await detector.detect(video)
			} catch {
				// A frame that cannot be read is not worth reporting; the next one is along shortly.
			}
			for (const code of codes) {
				try {
					const sticker = await read(code.rawValue)
					stop()
					return sticker
				} catch (error) {
					// A code that is not a bliti sticker does not stop the camera, because the next thing
					// in frame may well be one.
					if (code.rawValue !== rejected) {
						rejected = code.rawValue
						onRejected?.(error.message ?? String(error))
					}
				}
			}
			await new Promise((resolve) => setTimeout(resolve, 200))
		}
	} finally {
		stop()
	}
	return null
}
