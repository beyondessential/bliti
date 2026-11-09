// The steps of a guide (ASM): each opens to its photo or video, one at a time, and is ticked off by its
// number. They are done in order, and each is held until the parts it needs are ticked. Starting over
// clears the steps and the parts of one Iti, and keeps the parts marked to keep, which are those a
// reader has for a whole batch.

export default function steps() {
	const guide = document.querySelector('.guide')
	guide.classList.add('ticks')
	const list = [...document.querySelectorAll('.build > li')]
	const panels = list.map((step) => step.querySelector('details'))
	const again = document.getElementById('again')
	const parts = new Map([...document.querySelectorAll('.parts input')].map((box) => [box.id, box]))
	const nameOf = (id) => document.querySelector(`label[for="${id}"]`).firstChild.textContent.trim()
	const done = (step) => step.classList.contains('done')

	function mark(step, number, on) {
		const button = step.querySelector('.num')
		step.classList.toggle('done', on)
		button.setAttribute('aria-pressed', on)
		button.textContent = on ? '✓' : number
		if (on) step.querySelector('details').open = false
	}

	// Only the next step can be ticked, and only once the parts it needs are ticked. Only the last one
	// done can be unticked. Any step can still be opened to read.
	const hasParts = (step) => !step.dataset.needs || step.dataset.needs.split(' ').every((id) => parts.get(id).checked)

	function sync() {
		const first = list.findIndex((step) => !done(step))
		const next = first === -1 ? list.length : first
		list.forEach((step, index) => {
			const button = step.querySelector('.num')
			const tickable = index === next && hasParts(step)
			button.disabled = !tickable && index !== next - 1
			button.title = !button.disabled || index < next ? '' : index > next ? 'Do the steps before this first' : 'Tick the parts it needs first'
		})
		again.disabled = first !== -1
	}

	// Each step says how many of its parts the reader has, and names what is missing, so they can do
	// the steps they can and fetch the rest as they come to it.
	function sortParts() {
		for (const step of list) {
			if (!step.dataset.needs) continue
			const needs = step.dataset.needs.split(' ')
			const lacking = needs.filter((id) => !parts.get(id).checked)
			step.classList.toggle('short', lacking.length > 0)
			const pill = step.querySelector('.pill')
			const missing = step.querySelector('.missing')
			pill.hidden = missing.hidden = lacking.length === 0
			pill.textContent = `${needs.length - lacking.length} of ${needs.length} parts`
			missing.textContent = `Still needed: ${lacking.map(nameOf).join(', ')}.`
		}
	}

	// Ticking a step opens the one after it, so the reader follows the guide down.
	function openNext(index) {
		const next = list[index + 1]
		if (!next) return
		next.querySelector('details').open = true
		next.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
	}

	// One step is open at a time, and a video plays while its step is open.
	for (const panel of panels) {
		panel.addEventListener('toggle', () => {
			if (panel.open) for (const other of panels) if (other !== panel) other.open = false
			for (const video of panel.querySelectorAll('video')) {
				if (panel.open) video.play()?.catch(() => {})
				else video.pause()
			}
		})
	}

	list.forEach((step, index) => {
		const number = index + 1
		const button = document.createElement('button')
		button.className = 'num'
		button.setAttribute('aria-label', `Step ${number} done`)
		step.prepend(button)
		mark(step, number, false)
		button.addEventListener('click', () => {
			const on = !done(step)
			mark(step, number, on)
			if (on) openNext(index)
			sync()
		})
		if (step.dataset.needs) {
			const pill = document.createElement('span')
			pill.className = 'pill'
			step.querySelector('summary').append(pill)
			const missing = document.createElement('p')
			missing.className = 'missing'
			step.querySelector('.detail').prepend(missing)
		}
	})

	document.querySelector('.parts').addEventListener('change', () => {
		sortParts()
		sync()
	})

	again.addEventListener('click', () => {
		list.forEach((step, index) => mark(step, index + 1, false))
		for (const box of document.querySelectorAll('.parts > ul:not(.keep) input')) box.checked = false
		for (const panel of panels) panel.open = false
		panels[0].open = true
		sortParts()
		sync()
		document.getElementById('parts').scrollIntoView({ behavior: 'smooth' })
	})

	panels[0].open = true
	sortParts()
	sync()
}
