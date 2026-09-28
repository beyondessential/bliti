# Camera button on desktop

## Offering the camera

- [x] A browser with a camera and no `BarcodeDetector` offers "Scan with camera" (verifies spec: WEB)
- [ ] A browser that exposes `BarcodeDetector` without the `qr_code` format still reads a code with the camera (verifies spec: WEB)

## Reading with the camera

- [x] A code held up to the camera is read in a browser with no QR detector of its own, and treated as a typed one is (verifies spec: WEB)
- [x] A frame with no code in it, or not yet sized, reads nothing rather than failing
- [ ] Manual: desktop Chrome on Windows or Linux reads a printed device code from a webcam
- [ ] Manual: desktop Firefox reads a printed device code from a webcam
- [ ] Manual: Chrome on Android still reads a code with the camera as before
