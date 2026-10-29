// Choosing the steps of a run (PWEB, "Choosing the steps").

import { expect, test } from '@playwright/test'

import { acts, emitState, openChannel, stateOf, step } from './fake-provision-client.js'

const STEPS = [
	step('hardware', { title: 'Check the hardware', done: 'not-done' }),
	step('tpm', { title: 'Update the TPM firmware', done: 'done', why: 'Up to date' }),
	step('eeprom', { title: 'Program the EEPROM', done: 'unknown', why: 'Could not tell' }),
	step('disk', { title: 'Write the system to the disk', done: 'not-done', why: 'Empty' }),
]

const box = (page, title) => page.getByRole('checkbox', { name: new RegExp(`^${title}`) })
const start = (page) => page.getByRole('button', { name: 'Start' })

test('lists every step in order, each with a checkbox and its reason where the device gives one', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	const labels = page.locator('label.pick')
	await expect(labels).toHaveCount(4)
	await expect(labels.nth(0)).toHaveText('Check the hardware')
	await expect(labels.nth(1)).toHaveText('Update the TPM firmware' + 'Up to date')
	await expect(labels.nth(2)).toHaveText('Program the EEPROM' + 'Could not tell')
	await expect(labels.nth(3)).toHaveText('Write the system to the disk' + 'Empty')
	await expect(page.getByText('Choose what to do')).toBeVisible()
	await expect(page.locator('.chip')).toHaveText('K7QX')
})

test('ticks each step that is not done, and none that is', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	await expect(box(page, 'Check the hardware')).toBeChecked()
	await expect(box(page, 'Update the TPM firmware')).not.toBeChecked()
	await expect(box(page, 'Program the EEPROM')).toBeChecked()
	await expect(box(page, 'Write the system to the disk')).toBeChecked()
})

test('lets any step be ticked or unticked, and keeps that as the device sends the state again', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	await box(page, 'Update the TPM firmware').check()
	await box(page, 'Check the hardware').uncheck()
	await expect(box(page, 'Update the TPM firmware')).toBeChecked()
	await expect(box(page, 'Check the hardware')).not.toBeChecked()

	await emitState(page, { steps: STEPS })
	await expect(box(page, 'Update the TPM firmware')).toBeChecked()
	await expect(box(page, 'Check the hardware')).not.toBeChecked()
})

test('starts a run of the steps ticked, in order, as the primary action', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	await expect(start(page)).toBeEnabled()
	await expect(start(page)).not.toHaveClass(/secondary/)
	await box(page, 'Update the TPM firmware').check()
	await box(page, 'Program the EEPROM').uncheck()
	await start(page).click()
	await expect.poll(() => acts(page)).toEqual([{ act: 'start', steps: ['hardware', 'tpm', 'disk'] }])
})

test('does not offer starting a run while no step is ticked', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	for (const title of ['Check the hardware', 'Program the EEPROM', 'Write the system to the disk']) {
		await box(page, title).uncheck()
	}
	await expect(start(page)).toBeDisabled()
	await box(page, 'Update the TPM firmware').check()
	await expect(start(page)).toBeEnabled()
	expect(await acts(page)).toEqual([])
})

test('leaves choosing for the run once the device says it is running', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	await start(page).click()
	await emitState(page, {
		run: 'running',
		steps: STEPS.map((each, index) => ({ ...each, state: index === 0 ? 'running' : 'pending' })),
	})
	await expect(page.getByRole('checkbox')).toHaveCount(0)
	await expect(page.getByRole('heading', { name: 'Provisioning' })).toBeVisible()
})

test('does not send a second start while the first is awaiting its answer', async ({ page }) => {
	await openChannel(page, stateOf({ steps: STEPS }))
	await page.evaluate(() => (window.__blitiProvisionAnswers.start = [[]]))
	await start(page).click()
	await expect(start(page)).toBeDisabled()
	expect(await acts(page)).toHaveLength(1)
})
