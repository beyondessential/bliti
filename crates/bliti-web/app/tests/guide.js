// What the specs of the guide pages (ASM) share.

/// The number of each step, a button.
export const numbers = (page) => page.locator('.build > li .num')

/// Every part of the list ticked, as a reader with everything to hand has it, so the order of the steps
/// is all that holds them.
export const tickParts = (page) =>
	page.evaluate(() => {
		for (const box of document.querySelectorAll('.parts input')) box.checked = true
		document.querySelector('.parts').dispatchEvent(new Event('change'))
	})

/// A step's details, and the line that opens it.
export const panel = (page, index) => page.locator('.build > li').nth(index).locator('details')
export const heading = (page, index) => page.locator('.build > li').nth(index).locator('summary')

/// The place of the step whose title has the text, among the steps of the page.
export const indexOf = (page, text) =>
	page.evaluate(
		(text) =>
			[...document.querySelectorAll('.build > li')].findIndex((step) => step.querySelector('.title').textContent.includes(text)),
		text,
	)
