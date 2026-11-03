// The assembly guide (ASM): the page at `/assemble`. The document is static; this adds what a reader
// does to it: the tab for their system, steps ticked off by their number, and starting over.

import { keepReady } from '../offline.js'
import '../styles.css'
import './assemble.css'

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

// A step is ticked by its number. Starting over clears the steps and the parts of one Iti, and keeps
// the image and the parts of provisioning, because a stick is written once for a batch.
guide.classList.add('ticks')
const steps = [...document.querySelectorAll('.build > li')]
const again = document.getElementById('again')
const progress = document.getElementById('progress')

function count() {
	const done = steps.filter((step) => step.classList.contains('done')).length
	progress.textContent = `${done} of ${steps.length} steps done`
	again.disabled = done !== steps.length
}

function mark(step, number, on) {
	const button = step.querySelector('.num')
	step.classList.toggle('done', on)
	button.setAttribute('aria-pressed', on)
	button.textContent = on ? '✓' : number
}

steps.forEach((step, index) => {
	const number = index + 1
	const button = document.createElement('button')
	button.className = 'num'
	button.setAttribute('aria-label', `Step ${number} done`)
	step.prepend(button)
	mark(step, number, false)
	button.addEventListener('click', () => {
		mark(step, number, !step.classList.contains('done'))
		count()
	})
})

again.addEventListener('click', () => {
	steps.forEach((step, index) => mark(step, index + 1, false))
	for (const box of document.querySelectorAll('.parts > ul:not(.keep) input')) box.checked = false
	count()
	window.scrollTo({ top: 0, behavior: 'smooth' })
})
count()
