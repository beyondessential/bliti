// What the simple network screens' tests share: configurations in each simple state, the facts that
// say how the device stands, and the way from the Status screen to the Network screen (NSET).

import { answer, message, openChannel, sent, stateOf } from './fake-client.js'

export const WIRED = { kind: 'wired-dynamic', label: 'eth0', enabled: true, verify: true, interface: 'eth0' }
export const STAFF = {
	kind: 'wireless',
	label: 'Clinic-Staff',
	enabled: true,
	verify: true,
	ssid: 'Clinic-Staff',
	security: { kind: 'sae', passphrase: 'correct horse battery' },
}
export const HOTSPOT = { enabled: true, ssid: 'clinic-iti-07', passphrase: 'reef-otter-glass-71', 'share-upstream': true, 'isolate-clients': true }

const off = (thing) => ({ ...thing, enabled: false })

export const ON_CABLE = { attachments: [WIRED, off(STAFF)] }
export const SHARING = { attachments: [WIRED, off(STAFF)], hotspot: HOTSPOT }
export const ON_WIFI = { attachments: [off(WIRED), STAFF], hotspot: off(HOTSPOT) }
export const BOTH = { attachments: [WIRED, STAFF], hotspot: off(HOTSPOT) }
// Two wireless networks: an ordering only the network settings present.
export const OUTSIDE = { attachments: [WIRED, STAFF, { ...STAFF, label: 'BackupLink', ssid: 'BackupLink' }] }

const passed = { status: { is: 'passed' } }
const on = (iface) => ({ interface: { name: iface } })
export const fact = (name, value, traits = {}, kind = 'text') =>
	message({ type: 'fact', at: 1, fact: name, kind, value, traits: { ...passed, ...traits } })
export const carrier = (plugged = true, iface = 'eth0') => fact('wired-carrier', plugged, on(iface), 'boolean')
export const address = (iface, value = '10.4.2.20') => fact('network-address', value, on(iface), 'ipv4')
export const joined = (network, iface = 'wlan0') => fact('wireless-network', network, on(iface))
export const hostname = (name) => fact('hostname', name)
export const clients = (count) =>
	message({ type: 'reading', at: 1, measurement: 'hotspot-clients', kind: 'quantity', value: count, traits: passed })

export const title = (page) => page.getByRole('heading', { level: 1 })
export const networkRow = (page) => page.locator('.health li').first()
export const way = (page, name) => page.locator('.ways button').filter({ has: page.locator('.name', { hasText: name }) })
export const proposals = async (page) => (await sent(page)).filter((each) => each.type === 'configuration').map((each) => each.document)

/// Take the application to the Network screen of a device holding `document`, its feed's first batch
/// carrying `facts`.
export async function openSetup(page, { document, capabilities, states = [], busy, facts = [] }) {
	await openChannel(page, ...facts)
	await answer(page, 'configure', [message({ type: 'configuration', document, capabilities }), stateOf({ busy, attachments: states })])
	await networkRow(page).getByRole('button').click()
	await title(page).getByText('Network', { exact: true }).waitFor()
}

/// Open one of the Network screen's ways.
export async function openWay(page, name) {
	await way(page, name).click()
}
