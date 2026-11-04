// The assembly guide (ASM): the page at `/assemble`. The document is static; this adds what a reader
// does to it: the tab for their system, and the steps.

import { keepReady } from '../offline.js'
import '../styles.css'
import './assemble.css'
import steps from './steps.js'

// So a first visit here leaves the application ready offline, as a first visit to any page does (WEB).
keepReady()

const guide = document.querySelector('.guide')
const mobile = /Android|iPhone|iPad|iPod/.test(navigator.userAgent)
guide.classList.toggle('mobile', mobile)

// The way to write the image is shown for the visitor's own system, and they can pick another.
const tabs = [...document.querySelectorAll('[role="tab"]')]
function show(system) {
	for (const tab of tabs) {
		const on = tab.id === `tab-${system}`
		tab.setAttribute('aria-selected', on)
		document.getElementById(tab.getAttribute('aria-controls')).hidden = !on
	}
}
function guess() {
	const agent = navigator.userAgent
	if (mobile) return 'windows'
	if (/Mac/.test(agent)) return 'macos'
	if (/Linux|X11/.test(agent)) return 'linux'
	return 'windows'
}
for (const tab of tabs) tab.addEventListener('click', () => show(tab.id.slice('tab-'.length)))
show(guess())

steps()
