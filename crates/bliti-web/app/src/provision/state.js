// What the provisioning page decides from a `provision-state`, with no browser in it: whether the
// medium may be driven, which screen a state shows, what is ticked, and where the board's record is
// (PWEB, PRS). Dates are civil dates, `YYYY-MM-DD`, as the stream sends them.

const DAY = 86_400_000

/// The screen each state of the device shows (PWEB).
const SCREENS = { idle: 'choosing', running: 'run', waiting: 'run', failed: 'run', finished: 'done' }

/// A date as `YYYY-MM-DD` in the phone's own time.
export function civilDate(date = new Date()) {
	const pad = (n) => String(n).padStart(2, '0')
	return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

const days = (date) => {
	const [year, month, day] = date.split('-').map(Number)
	return Date.UTC(year, month - 1, day) / DAY
}

/// How many days `date` is before `today`, negative where it is after.
export function daysBefore(date, today) {
	return days(today) - days(date)
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/// A civil date for reading: `12 Sep 2026`.
export function showDate(date) {
	const [year, month, day] = date.split('-').map(Number)
	return `${day} ${MONTHS[month - 1]} ${year}`
}

/// When a record was saved, in the phone's time: `29 Oct 2026, 14:32`. The text as it came where it
/// is not a time.
export function showSaved(at) {
	const when = new Date(at)
	if (Number.isNaN(when.getTime())) return at
	const pad = (n) => String(n).padStart(2, '0')
	return `${showDate(civilDate(when))}, ${pad(when.getHours())}:${pad(when.getMinutes())}`
}

/// Why the medium may not be driven, or null where it may (PRS, "The revision" and "The image's
/// date"): `page` where its revision is above the range this page implements, and `medium` where it
/// is below it or the image was built `lifetime` days or more before `today`, with `built` where
/// the date is what refused it. The page is out of date whatever the image's date.
export function refusal(state, { revisions: [lowest, highest], lifetime, today }) {
	if (state.revision > highest) return { kind: 'page' }
	const old = daysBefore(state.built, today) >= lifetime
	if (state.revision < lowest || old) return { kind: 'medium', built: old ? state.built : null }
	return null
}

/// The screen a state shows: `choosing`, `run` or `done` (PWEB).
export function screenOf(state) {
	return SCREENS[state.run] ?? 'run'
}

/// Whether a step is ticked unless the operator has said otherwise: any not `done` (RUN).
export const ticked = (step) => step.done !== 'done'

/// The record some step reports in its result, where one does (PRS, "The record").
export function recordOf(steps) {
	for (const step of steps) {
		const record = step.result?.record
		if (record && typeof record === 'object') return record
	}
	return null
}

/// Whether the step `tailscale` has passed and waits on a tailnet administrator's approval (PTS).
export function awaitsApproval(steps) {
	const step = steps.find((each) => each.id === 'tailscale')
	return step?.state === 'passed' && step.result?.['needs-approval'] === true
}

/// What to say of it.
export const APPROVAL = 'A tailnet administrator has to approve this Iti before it joins the tailnet.'

/// The pictures of the board the page carries, for the step and prompt each belongs beneath (PWEB,
/// "Pictures"). The USER button's is shown while finding a device, and has no step.
const BENEATH = { 'hardware/case': 'case-switch', 'eeprom/id-wp': 'wp-pads' }

/// The picture beneath prompt `prompt` of step `step`, or null.
export function pictureFor(step, prompt) {
	return BENEATH[`${step}/${prompt}`] ?? null
}

/// What the activity record says of an act the page sent. A `secret` answer is said without its
/// value, and an answer to any prompt names the prompt and not what was typed (PWEB).
export function actSaid(act, { steps, prompt, value, secret } = {}) {
	const parts = ['provision-act', act]
	if (steps) parts.push(`steps ${steps.join(', ')}`)
	if (prompt !== undefined) {
		parts.push(`prompt ${prompt}`)
		if (!secret && value !== undefined) parts.push(`value ${value}`)
	}
	return parts.join('  ')
}

/// What the activity record says of a `provision-state`: the run's state and each step's, without
/// progress, so a long step does not fill it.
export function stateSaid(state) {
	const steps = state.steps.map((step) => `${step.id} ${step.state}`).join(', ')
	const prompts = state.prompts.length ? `, prompts ${state.prompts.map((prompt) => prompt.id).join(', ')}` : ''
	return `provision-state  ${state.run}  ${steps}${prompts}`
}
