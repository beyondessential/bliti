// The countries a radio's regulatory domain is set by, as ISO 3166-1 alpha-2 codes (BLI-NET), named
// in the operator's language by the browser.

const CODES = (
	'AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS ' +
	'BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE ' +
	'EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM ' +
	'HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC ' +
	'LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA ' +
	'NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW ' +
	'SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO ' +
	'TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW'
).split(' ')

let names

/// A country's name for a code, or the code itself where the browser cannot name it.
export function countryName(code) {
	names ??= new Intl.DisplayNames(undefined, { type: 'region' })
	try {
		return names.of(code) ?? code
	} catch {
		return code
	}
}

/// The countries to offer, sorted by name: `codes` where the device lists them, every country where
/// it takes any, and always the one in force so the field shows what it is.
export function countryOptions(codes, current) {
	const offered = new Set(codes === 'any' ? CODES : codes)
	if (current) offered.add(current)
	return [...offered]
		.map((code) => ({ value: code, label: countryName(code) }))
		.sort((a, b) => a.label.localeCompare(b.label))
}
