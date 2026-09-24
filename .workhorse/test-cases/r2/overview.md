# Offline use after installation

## Automated

- [x] A page loaded online, reloaded offline, is served and can fetch its wasm module (verifies spec: WEB)
- [x] A next version installed while a page is open waits rather than taking over, and the open page can still fetch its wasm module offline (verifies spec: WEB)

## Manual

- [ ] On Chrome for Android, install the application, open it once online after a new deploy, go offline without closing it, then read a code and connect: no "Failed to fetch" (verifies spec: WEB)
- [ ] Close the installed application fully and reopen it online: the new version is what runs
