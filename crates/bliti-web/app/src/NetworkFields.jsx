// The fields of the network configuration screen (BLI-NSCR): one form per kind of candidate, the
// hotspot, and the country. What each offers is asked of capabilities.js, never read off the
// capabilities themselves.

import { useEffect, useId, useState } from 'react'

import {
	absent,
	acts,
	adapterName,
	adapters,
	bands,
	channels,
	countries,
	eapMethods,
	interfaces,
	offersHotspotSetting,
	offersMember,
	radios,
	scanners,
	securityKinds,
	widths,
} from './capabilities.js'
import { countryOptions } from './countries.js'
import { generatePassphrase, same } from './network.js'
import { pathOf, within } from './path.js'
import { ScanResults } from './Scan.jsx'
import { bandName, securityName, widthName } from './wireless.js'

// Field primitives

/// Where a field stands against what is marked: the part a failure named, and a problem found before
/// proposing, which is said beneath the field it is about.
function Marked({ path, marks }) {
	const problem = marks.problem
	return problem && within(problem.at, path) ? <p className="why">{problem.reason}</p> : null
}

function faultClass(path, marks) {
	return within(marks.at, path) ? 'field-fault' : undefined
}

export function TextField({ label, path, value, onChange, marks, hint, multiline, placeholder }) {
	const id = useId()
	const Input = multiline ? 'textarea' : 'input'
	return (
		<>
			<label htmlFor={id}>{label}</label>
			<Input
				id={id}
				className={faultClass(path, marks)}
				aria-invalid={within(marks.at, path) || undefined}
				value={value ?? ''}
				placeholder={placeholder}
				onChange={(event) => onChange(event.target.value)}
				autoComplete="off"
				autoCapitalize="off"
				spellCheck="false"
			/>
			{hint && <p className="muted hint">{hint}</p>}
			<Marked path={path} marks={marks} />
		</>
	)
}

const parseList = (text) => text.split(/[\s,]+/).filter(Boolean)

/// A list typed as one line, separated by commas or spaces. What is typed is kept as typed, so a comma
/// on its way to a second entry is not eaten; the list beneath it is what the document holds.
export function ListField({ value, onChange, ...rest }) {
	const [text, setText] = useState(() => (value ?? []).join(', '))
	useEffect(() => {
		setText((typed) => (same(parseList(typed), value ?? []) ? typed : (value ?? []).join(', ')))
	}, [value])
	return (
		<TextField
			{...rest}
			value={text}
			onChange={(typed) => {
				setText(typed)
				const list = parseList(typed)
				onChange(list.length > 0 ? list : undefined)
			}}
		/>
	)
}

export function SelectField({ label, path, value, options, onChange, marks, hideLabel }) {
	const id = useId()
	return (
		<>
			<label htmlFor={id} className={hideLabel ? 'sr-only' : undefined}>
				{label}
			</label>
			<select
				id={id}
				className={faultClass(path, marks)}
				aria-invalid={within(marks.at, path) || undefined}
				value={value ?? ''}
				onChange={(event) => onChange(event.target.value)}
			>
				{options.map((option) => (
					<option key={option.value} value={option.value}>
						{option.label}
					</option>
				))}
			</select>
			<Marked path={path} marks={marks} />
		</>
	)
}

function Check({ label, checked, onChange }) {
	return (
		<label className="check">
			<input type="checkbox" checked={Boolean(checked)} onChange={(event) => onChange(event.target.checked)} />{' '}
			{label}
		</label>
	)
}

/// A member set, or removed where the value is undefined.
export function withMember(object, member, value) {
	const next = { ...object }
	if (value === undefined || value === '') delete next[member]
	else next[member] = value
	return next
}

// Candidates

/// A new candidate of `kind`, carrying what that kind requires and every optional member whose absence
/// would mean something. It is checked when applied (NSCR).
export function blankCandidate(kind, capabilities) {
	switch (kind) {
		case 'wireless': {
			const security = securityKinds(capabilities)[0] ?? 'psk-sae'
			const candidate = { kind, label: '', verify: true, ssid: '', security: blankSecurity(security, capabilities) }
			if (offersMember(capabilities, kind, 'hidden')) candidate.hidden = false
			return candidate
		}
		case 'wired-dynamic': {
			const name = interfaces(capabilities, kind)?.[0] ?? ''
			return { kind, label: name ? `${name} automatic` : '', verify: true, interface: name }
		}
		default:
			return { kind, label: '', verify: true, interface: interfaces(capabilities, kind)?.[0] ?? '', addresses: [], gateway: '' }
	}
}

export function blankSecurity(kind, capabilities, candidate = {}) {
	if (kind === 'enterprise') return { kind, eap: eapMethods(capabilities, candidate)?.[0] ?? 'peap' }
	return { kind, passphrase: '' }
}

/// A wireless candidate with what its adapter does not carry taken out, and what it does carry and
/// would mean something by its absence written in. Security moves to the nearest kind offered,
/// keeping a passphrase where the new kind takes one.
function fitWireless(candidate, capabilities) {
	let next = candidate
	const kinds = securityKinds(capabilities, next)
	const current = next.security?.kind
	if (current && !kinds.includes(current)) {
		const replacement = kinds.find((kind) => (kind === 'enterprise') === (current === 'enterprise')) ?? kinds[0]
		const keeps = replacement && replacement !== 'enterprise' && current !== 'enterprise'
		next = { ...next, security: keeps ? { ...next.security, kind: replacement } : blankSecurity(replacement ?? current, capabilities, next) }
	} else if (current === 'enterprise') {
		const methods = eapMethods(capabilities, next)
		if (methods && methods.length > 0 && !methods.includes(next.security.eap)) {
			next = { ...next, security: { ...next.security, eap: methods[0] } }
		}
	}
	if (!offersMember(capabilities, 'wireless', 'hidden', next)) next = withMember(next, 'hidden', undefined)
	else if (next.hidden === undefined) next = { ...next, hidden: false }
	if (!offersMember(capabilities, 'wireless', 'nameservers', next)) next = withMember(next, 'nameservers', undefined)
	return next
}

/// Which adapter a wireless candidate or the hotspot runs on, where the device has more than one:
/// named by what each adapter is, and left to the device where none is picked.
function AdapterField({ part, value, path, capabilities, marks, onChange }) {
	const names = adapters(capabilities, part)
	if (names.length === 0) return null
	const options = [
		{ value: '', label: 'Device chooses' },
		...[...new Set([...names, value].filter(Boolean))].map((name) => ({ value: name, label: adapterName(capabilities, name) })),
	]
	return <SelectField label="Adapter" path={path} value={value ?? ''} options={options} onChange={(name) => onChange(name || undefined)} marks={marks} />
}

const KIND_NAMES = { wireless: 'Wireless', 'wired-dynamic': 'Wired, DHCP', 'wired-static': 'Wired, static' }

export function kindName(kind) {
	return KIND_NAMES[kind] ?? kind
}

/// The fields of one candidate, by its kind.
export function CandidateFields({ candidate, index, change, capabilities, marks, scan }) {
	const at = (...rest) => pathOf(['attachments', index, ...rest])
	const set = (member) => (value) => change((held) => withMember(held, member, value))
	const kind = candidate.kind

	return (
		<>
			<TextField label="Name" path={at('label')} value={candidate.label} onChange={set('label')} marks={marks} />
			{kind === 'wireless' && (
				<WirelessFields candidate={candidate} at={at} change={change} capabilities={capabilities} marks={marks} scan={scan} />
			)}
			{(kind === 'wired-dynamic' || kind === 'wired-static') && (
				<InterfaceField candidate={candidate} at={at} set={set} capabilities={capabilities} marks={marks} />
			)}
			{kind === 'wired-static' && (
				<>
					<ListField label="Address" path={at('addresses')} value={candidate.addresses} onChange={(list) => set('addresses')(list ?? [])} marks={marks} placeholder="192.168.1.20/24" />
					<TextField label="Gateway" path={at('gateway')} value={candidate.gateway} onChange={set('gateway')} marks={marks} placeholder="192.168.1.1" />
				</>
			)}
			{offersMember(capabilities, kind, 'nameservers', candidate) && (
				<ListField
					label="Resolvers"
					path={at('nameservers')}
					value={candidate.nameservers}
					onChange={set('nameservers')}
					marks={marks}
					hint={kind === 'wired-static' ? 'Queried in this order.' : 'Queried before DHCP-supplied resolvers.'}
				/>
			)}
		</>
	)
}

function InterfaceField({ candidate, at, set, capabilities, marks }) {
	const names = interfaces(capabilities, candidate.kind)
	if (names === null) {
		return <TextField label="Interface" path={at('interface')} value={candidate.interface} onChange={set('interface')} marks={marks} />
	}
	const options = [...new Set([...names, candidate.interface].filter(Boolean))].map((name) => ({ value: name, label: name }))
	return <SelectField label="Interface" path={at('interface')} value={candidate.interface} options={options} onChange={set('interface')} marks={marks} />
}

const EAP_NAMES = { peap: 'PEAP', ttls: 'TTLS', tls: 'TLS' }

// Which credentials each EAP method asks for. TLS proves the client by certificate; the tunnelled
// methods by a password inside the tunnel.
const EAP_FIELDS = {
	tls: ['identity', 'ca-certificate', 'domain', 'client-certificate', 'client-key', 'client-key-passphrase'],
	tunnelled: ['identity', 'anonymous-identity', 'password', 'phase2', 'ca-certificate', 'domain'],
}

const CREDENTIALS = {
	identity: { label: 'Identity' },
	'anonymous-identity': { label: 'Anonymous identity' },
	password: { label: 'Password' },
	phase2: { label: 'Phase 2', placeholder: 'mschapv2' },
	'ca-certificate': { label: 'CA certificate', multiline: true, placeholder: '-----BEGIN CERTIFICATE-----' },
	domain: { label: 'Domain', placeholder: 'radius.example.org' },
	'client-certificate': { label: 'Client certificate', multiline: true, placeholder: '-----BEGIN CERTIFICATE-----' },
	'client-key': { label: 'Client key', multiline: true, placeholder: '-----BEGIN PRIVATE KEY-----' },
	'client-key-passphrase': { label: 'Client key passphrase' },
}

function WirelessFields({ candidate, at, change, capabilities, marks, scan }) {
	const security = candidate.security ?? {}
	const setSecurity = (member) => (value) =>
		change((held) => ({ ...held, security: withMember(held.security ?? {}, member, value) }))
	const offered = securityKinds(capabilities, candidate)
	const kinds = [...new Set([...offered, security.kind].filter(Boolean))]
	const methods = eapMethods(capabilities, candidate) ?? ['peap', 'ttls', 'tls']
	const eapFields = security.eap === 'tls' ? EAP_FIELDS.tls : EAP_FIELDS.tunnelled

	// The name follows the SSID until the operator gives it one of its own.
	const setSsid = (ssid) =>
		change((held) => ({ ...held, ssid, label: !held.label || held.label === held.ssid ? ssid : held.label }))

	return (
		<>
			<SsidField candidate={candidate} at={at} onChange={setSsid} change={change} capabilities={capabilities} marks={marks} scan={scan} />
			<AdapterField
				part="wireless"
				value={candidate.interface}
				path={at('interface')}
				capabilities={capabilities}
				marks={marks}
				onChange={(name) => change((held) => fitWireless(withMember(held, 'interface', name), capabilities))}
			/>
			<SelectField
				label="Security"
				path={at('security', 'kind')}
				value={security.kind}
				options={kinds.map((kind) => ({ value: kind, label: securityName(kind) }))}
				onChange={(kind) =>
					change((held) => ({
						...held,
						security:
							kind === 'enterprise' || held.security?.kind === 'enterprise'
								? blankSecurity(kind, capabilities, held)
								: { ...held.security, kind },
					}))
				}
				marks={marks}
			/>
			{security.kind !== 'enterprise' && (
				<TextField label="Passphrase" path={at('security', 'passphrase')} value={security.passphrase} onChange={setSecurity('passphrase')} marks={marks} />
			)}
			{security.kind === 'enterprise' && (
				<>
					<SelectField
						label="EAP method"
						path={at('security', 'eap')}
						value={security.eap}
						options={[...new Set([...methods, security.eap].filter(Boolean))].map((method) => ({
							value: method,
							label: EAP_NAMES[method] ?? method,
						}))}
						onChange={setSecurity('eap')}
						marks={marks}
					/>
					{eapFields.map((member) => (
						<TextField key={member} {...CREDENTIALS[member]} path={at('security', member)} value={security[member]} onChange={setSecurity(member)} marks={marks} />
					))}
				</>
			)}
			{offersMember(capabilities, 'wireless', 'hidden', candidate) && (
				<Check label="Hidden network" checked={candidate.hidden} onChange={(hidden) => change((held) => ({ ...held, hidden }))} />
			)}
		</>
	)
}

function SsidField({ candidate, at, onChange, change, capabilities, marks, scan }) {
	const id = useId()
	const [adapter, setAdapter] = useState('')
	const path = at('ssid')
	const scanning = scan && acts(capabilities).scan
	const able = scanning ? scanners(capabilities) : []
	return (
		<>
			<label htmlFor={id}>SSID</label>
			<div className="pair">
				<input
					id={id}
					className={faultClass(path, marks)}
					aria-invalid={within(marks.at, path) || undefined}
					value={candidate.ssid ?? ''}
					onChange={(event) => onChange(event.target.value)}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck="false"
				/>
				{scanning && (
					<button type="button" className="secondary" onClick={() => scan.start(adapter || undefined)} disabled={scan.busy}>
						{scan.busy ? 'Scanning' : 'Scan'}
					</button>
				)}
			</div>
			<Marked path={path} marks={marks} />
			{able.length > 1 && (
				<SelectField
					label="Scan with"
					value={adapter}
					options={[{ value: '', label: 'All adapters' }, ...able.map((name) => ({ value: name, label: adapterName(capabilities, name) }))]}
					onChange={setAdapter}
					marks={marks}
				/>
			)}
			{scanning && scan.failure && <p className="why">{scan.failure.reason}</p>}
			{scanning && scan.points && <ScanResults points={scan.points} candidate={candidate} change={change} capabilities={capabilities} />}
		</>
	)
}

// The hotspot

const HOTSPOT_NAMES = {
	'share-upstream': 'upstream sharing',
	'isolate-clients': 'client isolation',
	'dhcp-range': 'DHCP range',
	band: 'band',
	channel: 'channel',
	'channel-width': 'width',
}

/// The sentences said in place of the hotspot settings the device did not offer. Settings absent for
/// one reason share its sentence; those it did not report at all are named.
function hotspotAbsences(capabilities, document, members) {
	const sentences = new Map()
	for (const member of members) {
		const why = absent(capabilities, `hotspot.${member}`, document)
		if (!why) continue
		if (!sentences.has(why.sentence)) sentences.set(why.sentence, { why, members: [] })
		sentences.get(why.sentence).members.push(member)
	}
	return [...sentences.values()].map(({ why, members }) =>
		why.reason === 'unreported'
			? `${capitalise(members.map((member) => HOTSPOT_NAMES[member]).join(', '))}: ${why.sentence.toLowerCase()}`
			: why.sentence,
	)
}

function capitalise(text) {
	return text.charAt(0).toUpperCase() + text.slice(1)
}

/// A hotspot with what its adapter and band do not carry taken out: an adapter carries its own bands,
/// and a band its own channels and widths. Sharing and isolation are on where unset, so an adapter
/// offering them has them written on.
function fitHotspot(hotspot, capabilities) {
	let next = hotspot
	const keeps = (member, values) => next[member] === undefined || values === null || values.includes(next[member])
	if (!keeps('band', bands(capabilities, next))) next = withMember(next, 'band', undefined)
	if (!keeps('channel', channels(capabilities, next))) next = withMember(next, 'channel', undefined)
	if (!keeps('channel-width', widths(capabilities, next))) next = withMember(next, 'channel-width', undefined)
	for (const member of ['share-upstream', 'isolate-clients', 'dhcp-range']) {
		if (!offersHotspotSetting(capabilities, member, next)) next = withMember(next, member, undefined)
		else if (member !== 'dhcp-range' && next[member] === undefined) next = { ...next, [member]: true }
	}
	return next
}

export function HotspotFields({ document, change, capabilities, marks, survey }) {
	const hotspot = document.hotspot
	const at = (member) => pathOf(['hotspot', member])
	const set = (member) => (value) => change((held) => withMember(held, member, value))
	const offers = (member) => offersHotspotSetting(capabilities, member, hotspot)
	const radio = ['band', 'channel', 'channel-width'].filter(offers)
	const usableBands = bands(capabilities, hotspot)
	const usableChannels = channels(capabilities, hotspot)
	const usableWidths = widths(capabilities, hotspot)
	const absences = hotspotAbsences(capabilities, document, ['share-upstream', 'isolate-clients', 'band', 'channel', 'channel-width', 'dhcp-range'])
	const picks = (values, name) => [{ value: '', label: 'Device picks' }, ...(values ?? []).map((value) => ({ value: String(value), label: name(value) }))]

	const setBand = (band) => change((held) => fitHotspot(withMember(held, 'band', band || undefined), capabilities))
	const setAdapter = (name) => change((held) => fitHotspot(withMember(held, 'interface', name), capabilities))
	const setNumber = (member) => (value) => set(member)(value === '' ? undefined : Number(value))

	return (
		<>
			<TextField label="SSID" path={at('ssid')} value={hotspot.ssid} onChange={set('ssid')} marks={marks} />
			<PassphraseField value={hotspot.passphrase} path={at('passphrase')} onChange={set('passphrase')} marks={marks} />
			<AdapterField part="hotspot" value={hotspot.interface} path={at('interface')} capabilities={capabilities} marks={marks} onChange={setAdapter} />
			{offers('share-upstream') && (
				<Check label="Share upstream connection" checked={hotspot['share-upstream']} onChange={set('share-upstream')} />
			)}
			{offers('isolate-clients') && (
				<Check label="Keep clients isolated" checked={hotspot['isolate-clients']} onChange={set('isolate-clients')} />
			)}
			<details open={Boolean(marks.at && ['band', 'channel', 'channel-width', 'dhcp-range'].some((member) => within(marks.at, at(member)))) || undefined}>
				<summary>Radio and addressing</summary>
				{radio.includes('band') && (
					<SelectField label="Band" path={at('band')} value={hotspot.band ?? ''} options={picks(usableBands, bandName)} onChange={setBand} marks={marks} />
				)}
				{(radio.includes('channel') || radio.includes('channel-width')) && (
					<div className="pair">
						{radio.includes('channel') && (
							<div>
								{usableChannels === null ? (
									<TextField label="Channel" path={at('channel')} value={hotspot.channel === undefined ? '' : String(hotspot.channel)} onChange={setNumber('channel')} marks={marks} />
								) : (
									<SelectField label="Channel" path={at('channel')} value={hotspot.channel === undefined ? '' : String(hotspot.channel)} options={picks(usableChannels, String)} onChange={setNumber('channel')} marks={marks} />
								)}
							</div>
						)}
						{radio.includes('channel-width') && (
							<div>
								<SelectField
									label="Width"
									path={at('channel-width')}
									value={hotspot['channel-width'] === undefined ? '' : String(hotspot['channel-width'])}
									options={picks(usableWidths ?? [20, 40, 80, 160], widthName)}
									onChange={setNumber('channel-width')}
									marks={marks}
								/>
							</div>
						)}
					</div>
				)}
				{absences.map((sentence) => (
					<p key={sentence} className="muted absent">
						{sentence}
					</p>
				))}
				{survey && radio.includes('channel') && <Survey survey={survey} capabilities={capabilities} />}
				{offers('dhcp-range') && (
					<TextField label="DHCP range" path={at('dhcp-range')} value={hotspot['dhcp-range']} onChange={set('dhcp-range')} marks={marks} placeholder="10.42.0.0/24" />
				)}
			</details>
		</>
	)
}

function PassphraseField({ value, path, onChange, marks }) {
	const id = useId()
	return (
		<>
			<label htmlFor={id}>Passphrase</label>
			<div className="pair">
				<input
					id={id}
					className={faultClass(path, marks)}
					aria-invalid={within(marks.at, path) || undefined}
					value={value ?? ''}
					onChange={(event) => onChange(event.target.value)}
					autoComplete="off"
					autoCapitalize="off"
					spellCheck="false"
				/>
				<button type="button" className="secondary" onClick={() => onChange(generatePassphrase())}>
					Generate
				</button>
			</div>
			<Marked path={path} marks={marks} />
		</>
	)
}

function Survey({ survey, capabilities }) {
	const channels = survey.spectrum?.channels
	const several = radios(capabilities).length > 1
	return (
		<>
			<button type="button" className="secondary small survey" onClick={survey.start} disabled={survey.busy}>
				{survey.busy ? 'Surveying' : 'Survey the spectrum'}
			</button>
			{survey.failure && <p className="why">{survey.failure.reason}</p>}
			{Array.isArray(channels) && (
				<ul className="spectrum">
					{channels.map((each) => (
						<li key={`${each.interface}-${each.band}-${each.channel}`}>
							<span>
								{each.channel} · {bandName(each.band)}
								{several && each.interface && ` · ${adapterName(capabilities, each.interface)}`}
							</span>
							<span className="muted">
								{each.networks} {each.networks === 1 ? 'network' : 'networks'}
								{typeof each.busy === 'number' && `, ${Math.round(each.busy * 100)}% busy`}
							</span>
						</li>
					))}
				</ul>
			)}
		</>
	)
}

/// A hotspot turned on, carrying what it requires and every setting whose absence would mean
/// something: sharing and isolation are on where unset, so they are written on.
export function blankHotspot(capabilities) {
	const hotspot = { ssid: '', passphrase: '' }
	if (offersHotspotSetting(capabilities, 'share-upstream')) hotspot['share-upstream'] = true
	if (offersHotspotSetting(capabilities, 'isolate-clients')) hotspot['isolate-clients'] = true
	return hotspot
}

// The country

export function CountryField({ value, onChange, capabilities, marks }) {
	const codes = countries(capabilities)
	const options = [{ value: '', label: 'Unset, world-safe channels only' }, ...countryOptions(codes ?? [], value)]
	return (
		<SelectField
			label="Country"
			path={pathOf(['regulatory-domain'])}
			value={value ?? ''}
			options={options}
			onChange={(code) => onChange(code || undefined)}
			marks={marks}
			hideLabel
		/>
	)
}
