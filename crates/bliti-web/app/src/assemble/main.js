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

// A step opens to its photo or video, and one is open at a time. Its number ticks it off, which closes
// it. Starting over clears the steps and the parts of one Iti, and keeps the image and the parts of
// provisioning, because a stick is written once for a batch.
guide.classList.add('ticks')
const steps = [...document.querySelectorAll('.build > li')]
const panels = steps.map((step) => step.querySelector('details'))
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
	if (on) step.querySelector('details').open = false
}

for (const panel of panels) {
	panel.addEventListener('toggle', () => {
		if (panel.open) for (const other of panels) if (other !== panel) other.open = false
		for (const video of panel.querySelectorAll('video')) {
			if (panel.open) video.play()?.catch(() => {})
			else video.pause()
		}
	})
}

// Steps are done in order, so the done ones are always a run from the first. Only the next step can
// be ticked, and only the last one done can be unticked. Any step can still be opened to read.
function sync() {
	const first = steps.findIndex((step) => !step.classList.contains('done'))
	const next = first === -1 ? steps.length : first
	steps.forEach((step, index) => {
		const button = step.querySelector('.num')
		button.disabled = index !== next && index !== next - 1
		button.title = button.disabled && index > next ? 'Do the steps before this first' : ''
	})
}

// Ticking a step opens the one after it, so the reader follows the guide down.
function openNext(index) {
	const next = steps[index + 1]
	if (!next) return
	next.querySelector('details').open = true
	next.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
}

steps.forEach((step, index) => {
	const number = index + 1
	const button = document.createElement('button')
	button.className = 'num'
	button.setAttribute('aria-label', `Step ${number} done`)
	step.prepend(button)
	mark(step, number, false)
	button.addEventListener('click', () => {
		const on = !step.classList.contains('done')
		mark(step, number, on)
		if (on) openNext(index)
		sync()
		count()
	})
})

// Once the reader has ticked a part of this Iti, each step says how many of its parts they have, and
// names what is missing, so they can do the steps they can and fetch the rest as they come to it.
const parts = new Map([...document.querySelectorAll('.parts input')].map((box) => [box.id, box]))
const nameOf = (id) => document.querySelector(`label[for="${id}"]`).firstChild.textContent.trim()

for (const step of steps) {
	if (!step.dataset.needs) continue
	const pill = document.createElement('span')
	pill.className = 'pill'
	pill.hidden = true
	step.querySelector('summary').append(pill)
	const missing = document.createElement('p')
	missing.className = 'missing'
	missing.hidden = true
	step.querySelector('.detail').prepend(missing)
}

function sortParts() {
	const started = [...parts.values()].some((box) => box.checked && !box.closest('.keep'))
	for (const step of steps) {
		if (!step.dataset.needs) continue
		const needs = step.dataset.needs.split(' ')
		const lacking = needs.filter((id) => !parts.get(id).checked)
		const short = started && lacking.length > 0
		step.classList.toggle('short', short)
		const pill = step.querySelector('.pill')
		const missing = step.querySelector('.missing')
		pill.hidden = !short
		missing.hidden = !short
		pill.textContent = `${needs.length - lacking.length} of ${needs.length} parts`
		missing.textContent = `Still needed: ${lacking.map(nameOf).join(', ')}.`
	}
}
document.querySelector('.parts').addEventListener('change', sortParts)

again.addEventListener('click', () => {
	steps.forEach((step, index) => mark(step, index + 1, false))
	for (const box of document.querySelectorAll('.parts > ul:not(.keep) input, .ship input')) box.checked = false
	for (const panel of panels) panel.open = false
	panels[0].open = true
	sortParts()
	sync()
	count()
	document.getElementById('parts').scrollIntoView({ behavior: 'smooth' })
})

panels[0].open = true
sortParts()
sync()
count()

// The link in the page's index opens the shipping section it points at.
const shipping = document.querySelector('#shipping details')
const openShipping = () => {
	if (location.hash === '#shipping') shipping.open = true
}
window.addEventListener('hashchange', openShipping)
openShipping()
