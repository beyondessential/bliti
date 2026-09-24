# QR SVG export

## The image

- [x] The SVG scans back to the QR code's URL, and so to the payload (verifies spec: QR)
- [x] The SVG carries the code alone, with no rendering drawn into it (verifies spec: QR)
- [x] The code is at error correction level H (verifies spec: QR)
- [x] A payload produces the same SVG every time
- [ ] A printed SVG at label size scans with a phone camera

## The generator

- [x] `bliti qr --svg` writes the image alone to stdout, and the rendering to stderr (verifies spec: QR)
- [x] `bliti qr` without `--svg` draws the code, the URL and the rendering to stdout
- [ ] `bliti qr --svg > code.svg` on a board gives a file an image viewer opens

## The web application

- [x] A code that has been read offers Download SVG, and the file carries the image the client produced (verifies spec: WEB)
- [x] The file is named `bliti-` and the last group of the rendering (verifies spec: WEB)
- [x] The real client produces the image from a typed payload through its wasm module (verifies spec: WEB)
- [ ] The download works in Chrome for Android
