// One open prompt, beneath the row of the step that asked it (PWEB, "The run").

import { useState } from 'react'

import Picture from './Picture.jsx'
import { pictureFor } from './state.js'

/// The prompt's text, the picture of the board where it has one, and what it offers to answer it by
/// its kind. `live` is whether the device can be answered, `busy` whether an answer is awaiting the
/// device's reply, and `refusal` why the device refused the last one.
export default function Prompt({ prompt, live, busy, refusal, onAnswer }) {
	const picture = pictureFor(prompt.step, prompt.id)
	return (
		<div className="question">
			<p className="lead">{prompt.text}</p>
			{picture && <Picture name={picture} />}
			{live && <Answer prompt={prompt} busy={busy} onAnswer={onAnswer} />}
			{refusal && (
				<p className="why" role="alert">
					{refusal}
				</p>
			)}
		</div>
	)
}

// spec: PWEB
function Answer({ prompt, busy, onAnswer }) {
	switch (prompt.kind) {
		case 'confirm':
			return (
				<div className="row">
					<button className="small" disabled={busy} onClick={() => onAnswer(true)}>
						Yes
					</button>
					<button className="small secondary" disabled={busy} onClick={() => onAnswer(false)}>
						No
					</button>
				</div>
			)
		case 'text':
			return <Field prompt={prompt} busy={busy} onAnswer={onAnswer} />
		case 'secret':
			return <Field prompt={prompt} busy={busy} onAnswer={onAnswer} secret />
		// A notice has nothing to answer, and a kind this page does not know is not to be answered (PRS).
		default:
			return null
	}
}

/// A field and saving it. A secret is hidden until revealed, and is cleared from the page as it is
/// sent (PWEB).
function Field({ prompt, busy, onAnswer, secret = false }) {
	const [value, setValue] = useState('')
	const [shown, setShown] = useState(false)

	function save() {
		onAnswer(value, { secret })
		if (secret) {
			setValue('')
			setShown(false)
		}
	}

	const input = (
		<input
			type={secret && !shown ? 'password' : 'text'}
			aria-label={prompt.text}
			value={value}
			autoComplete="off"
			autoCapitalize="off"
			spellCheck={false}
			disabled={busy}
			onChange={(event) => setValue(event.target.value)}
			onKeyDown={(event) => event.key === 'Enter' && !busy && save()}
		/>
	)

	return (
		<>
			{secret ? (
				<div className="input-show">
					{input}
					<button type="button" className="link" onClick={() => setShown(!shown)}>
						{shown ? 'Hide' : 'Show'}
					</button>
				</div>
			) : (
				input
			)}
			<button className="block save" disabled={busy} onClick={save}>
				Save
			</button>
		</>
	)
}
