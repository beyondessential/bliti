// The client a page holds where it reaches a real device or the fake one (WEB, PWEB, "The fake
// device"), as a plain function: no page, because none of it touches one.

import { expect, test } from '@playwright/test'

import { routed } from '../src/route.js'

const client = (name) => ({ open: () => `${name} open`, close: () => `${name} close` })

test('sends each member of the real client to whichever is active, but those it is given', () => {
	const real = client('real')
	const fake = client('fake')
	let active = real
	const page = routed(real, fake, () => active, { close: () => 'own close' })
	expect(page.open()).toBe('real open')
	active = fake
	expect(page.open()).toBe('fake open')
	expect(page.close()).toBe('own close')
})

test('throws as it is built where the fake lacks a member it would forward', () => {
	const real = { ...client('real'), scan: () => 'real scan' }
	expect(() => routed(real, client('fake'), () => real, {})).toThrow('The fake device lacks scan.')
	expect(() => routed(real, client('fake'), () => real, { scan: real.scan })).not.toThrow()
})
