// The fields of the network configuration screen (BLI-NSCR): one form per kind of candidate, the
// hotspot, and the country. What each offers is asked of capabilities.js, never read off the
// capabilities themselves.

import { useEffect, useId, useState } from 'react'

import {
	absent,
	acts,
	bands,
	channels,
	countries,
	eapMethods,
	interfaces,
	joinWith,
	offersHotspotSetting,
	offersMember,
	securityKinds,
	widths,
} from './capabilities.js'
import { countryOptions } from './countries.js'
import { generatePassphrase, same } from './network.js'
import { pathOf, within } from './path.js'
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
function withMember(object, member, value) {
	const next = { ...object }
	if (value === undefined || value === '') delete next[member]
	else next[member] = value
	return next
}

// Candidates

/// A new candidate of `kind`, carrying what that kind requires and every optional member whose absence
/// would mean something.
export function blankCandidate(kind, capabilities) {
	switch (kind) {
		case 'wireless': {
			const security = securityKinds(capabilities)[0] ?? 'psk-sae'
			const candidate = { kind, label: '', ssid: '', security: blankSecurity(security, capabilities) }
			if (offersMember(capabilities, kind, 'hidden')) candidate.hidden = false
			return candidate
		}
		case 'wired-dynamic': {
			const name = interfaces(capabilities, kind)?.[0] ?? ''
			return { kind, label: name ? `${name} automatic` : '', interface: name }
		}
		default:
			return { kind, label: '', interface: interfaces(capabilities, kind)?.[0] ?? '', addresses: [], gateway: '' }
	}
}

function blankSecurity(kind, capabilities) {
	if (kind === 'enterprise') return { kind, eap: eapMethods(capabilities)?.[0] ?? 'peap' }
	return { kind, passphrase: '' }
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
			{offersMember(capabilities, kind, 'nameservers') && (
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
	const offered = securityKinds(capabilities)
	const kinds = [...new Set([...offered, security.kind].filter(Boolean))]
	const methods = eapMethods(capabilities) ?? ['peap', 'ttls', 'tls']
	const eapFields = security.eap === 'tls' ? EAP_FIELDS.tls : EAP_FIELDS.tunnelled

	// The name follows the SSID until the operator gives it one of its own.
	const setSsid = (ssid) =>
		change((held) => ({ ...held, ssid, label: !held.label || held.label === held.ssid ? ssid : held.label }))

	return (
		<>
			<SsidField candidate={candidate} at={at} onChange={setSsid} change={change} capabilities={capabilities} marks={marks} scan={scan} />
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
								? blankSecurity(kind, capabilities)
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
			{offersMember(capabilities, 'wireless', 'hidden') && (
				<Check label="Hidden network" checked={candidate.hidden} onChange={(hidden) => change((held) => ({ ...held, hidden }))} />
			)}
		</>
	)
}

const UNJOINABLE = { open: 'Open', owe: 'OWE', wep: 'WEP' }

function SsidField({ candidate, at, onChange, change, capabilities, marks, scan }) {
	const id = useId()
	const path = at('ssid')
	const scanning = scan && acts(capabilities).scan
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
					<button type="button" className="secondary" onClick={scan.start} disabled={scan.busy}>
						{scan.busy ? 'Scanning' : 'Scan'}
					</button>
				)}
			</div>
			<Marked path={path} marks={marks} />
			{scanning && scan.networks && (
				<ul className="networks">
					{scan.networks.length === 0 && <li className="muted">No networks in range.</li>}
					{scan.networks.map((network) => {
						const kind = joinWith(capabilities, network.security)
						const pick = () =>
							change((held) => ({
								...held,
								ssid: network.ssid,
								label: !held.label || held.label === held.ssid ? network.ssid : held.label,
								security:
									kind === held.security?.kind
										? held.security
										: blankSecurity(kind, capabilities),
							}))
						return (
							<li key={`${network.ssid}-${network.band}`}>
								<button type="button" className="secondary small" onClick={pick} disabled={!kind}>
									{network.ssid}
								</button>
								<span className="muted">
									{kind
										? securityName(kind)
										: `Cannot join: ${(network.security ?? []).map((each) => UNJOINABLE[each] ?? securityName(each)).join(', ')}`}
									{network.signal !== undefined && ` · ${network.signal} dBm`}
								</span>
							</li>
						)
					})}
				</ul>
			)}
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
function hotspotAbsences(capabilities, members) {
	const sentences = new Map()
	for (const member of members) {
		const why = absent(capabilities, `hotspot.${member}`)
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

export function HotspotFields({ hotspot, change, capabilities, marks, survey }) {
	const at = (member) => pathOf(['hotspot', member])
	const set = (member) => (value) => change((held) => withMember(held, member, value))
	const offers = (member) => offersHotspotSetting(capabilities, member)
	const radio = ['band', 'channel', 'channel-width'].filter(offers)
	const usableBands = bands(capabilities)
	const usableChannels = channels(capabilities, hotspot.band)
	const usableWidths = widths(capabilities, hotspot.band)
	const absences = hotspotAbsences(capabilities, ['share-upstream', 'isolate-clients', 'band', 'channel', 'channel-width', 'dhcp-range'])
	const picks = (values, name) => [{ value: '', label: 'Device picks' }, ...(values ?? []).map((value) => ({ value: String(value), label: name(value) }))]

	// A band carries its own channels and widths, so moving to another drops the ones it does not.
	const setBand = (band) =>
		change((held) => {
			let next = withMember(held, 'band', band || undefined)
			const keeps = (member, values) => values === null || values.includes(next[member])
			if (!keeps('channel', channels(capabilities, next.band))) next = withMember(next, 'channel', undefined)
			if (!keeps('channel-width', widths(capabilities, next.band))) next = withMember(next, 'channel-width', undefined)
			return next
		})
	const setNumber = (member) => (value) => set(member)(value === '' ? undefined : Number(value))

	return (
		<>
			<TextField label="SSID" path={at('ssid')} value={hotspot.ssid} onChange={set('ssid')} marks={marks} />
			<PassphraseField value={hotspot.passphrase} path={at('passphrase')} onChange={set('passphrase')} marks={marks} />
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
				{survey && radio.includes('channel') && <Survey survey={survey} />}
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

function Survey({ survey }) {
	const channels = survey.spectrum?.channels
	return (
		<>
			<button type="button" className="secondary small survey" onClick={survey.start} disabled={survey.busy}>
				{survey.busy ? 'Surveying' : 'Survey the spectrum'}
			</button>
			{survey.failure && <p className="why">{survey.failure.reason}</p>}
			{Array.isArray(channels) && (
				<ul className="spectrum">
					{channels.map((each) => (
						<li key={`${each.band}-${each.channel}`}>
							<span>
								{each.channel} · {bandName(each.band)}
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
