// A page of its own with a script of its own, so the build gives the harness a second document and
// a second chunk. It is built under `--mode test` alone, to cover what any page after the operator
// app needs of the build and the service worker.
document.getElementById('second-page').textContent = 'Second page loaded'
