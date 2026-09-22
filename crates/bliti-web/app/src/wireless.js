// Our wording for the wireless vocabulary the device view and the network screen share, so a band
// or a security kind reads the same wherever an operator meets it.

const SECURITY = { psk: 'WPA2', sae: 'WPA3', 'psk-sae': 'WPA2/WPA3', enterprise: 'Enterprise' }

/// A security kind as a technician names it, or the kind as sent where this build does not know it.
export function securityName(kind) {
	return SECURITY[kind] ?? String(kind)
}

/// A band as written on an access point's label: `5ghz` reads as 5 GHz.
export function bandName(band) {
	const match = /^(\d+(?:\.\d+)?)ghz$/.exec(String(band))
	return match ? `${match[1]} GHz` : String(band)
}

/// A channel width, which the wire carries in megahertz.
export function widthName(width) {
	return `${width} MHz`
}

/// A channel trait (NFO) in one line: its number, band and width, whichever are present.
export function channelText(channel) {
	if (channel === null || channel === undefined) return ''
	if (typeof channel !== 'object') return String(channel)
	return [
		channel.number !== undefined && String(channel.number),
		channel.band !== undefined && bandName(channel.band),
		channel.width !== undefined && widthName(channel.width),
	]
		.filter(Boolean)
		.join(' · ')
}
