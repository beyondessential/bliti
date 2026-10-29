// A run, from its start to its end or its failure: every step with its state, and beneath each the
// progress and open prompts of it (PWEB, "The run").

import Prompt from './Prompt.jsx'

const STATE_NAMES = {
	passed: 'Done',
	failed: 'Failed',
	running: 'Running',
	waiting: 'Waiting',
	pending: 'To do',
	unchosen: 'Skipped',
}

// A step the device is working on or waiting on shows a spinner while live, and a still mark once
// the link has gone and nothing is under way any more.
function Mark({ state, live }) {
	const base = { passed: 'good', failed: 'bad', running: 'wait', waiting: 'wait' }[state] ?? 'todo'
	return (
		<span className={`mark ${base}${state === 'unchosen' ? ' skipped' : ''}`} role="img" aria-label={STATE_NAMES[state]}>
			{base === 'wait' && live && <span className="spinner" aria-hidden="true" />}
		</span>
	)
}

function Progress({ step }) {
	const percent = Math.round(Math.min(Math.max(step.progress, 0), 1) * 100)
	return (
		<div
			className="bar"
			role="progressbar"
			aria-label={`${step.title} progress`}
			aria-valuemin={0}
			aria-valuemax={100}
			aria-valuenow={percent}
		>
			<span style={{ width: `${percent}%` }} />
		</div>
	)
}

/// `live` is whether the device can be acted on: false for what was last seen once the channel has
/// closed, which offers nothing. `awaiting` is the acts sent that the device has not answered, and
/// `refusals` what it said of answers it refused, by prompt. `onAct(act, members, options)` sends an act.
export default function Run({ state, live, awaiting, refusals, onAct }) {
	const busy = awaiting.length > 0
	// The step to put Stop beneath while the device waits.
	const stopAt = state.run === 'waiting' ? state.steps.find((step) => step.state === 'waiting')?.id : undefined
	const stopBelow = live && state.run === 'waiting' && stopAt === undefined

	return (
		<section className={live ? undefined : 'stale'}>
			<h2>{live ? 'Provisioning' : 'Provisioning, as last seen'}</h2>
			<ul className="health">
				{state.steps.map((step) => {
					const prompts = state.prompts.filter((prompt) => prompt.step === step.id)
					const failed = step.state === 'failed'
					const waitingStop = live && step.id === stopAt
					const under = typeof step.progress === 'number' && step.state !== 'unchosen'
					const detail = step.state === 'unchosen' ? 'Skipped' : failed ? step.reason : step.why
					return (
						<li key={step.id} data-step={step.id} data-state={step.state} className={step.state === 'unchosen' ? 'skipped' : undefined}>
							<Mark state={step.state} live={live} />
							<div className="body">
								<span className="name">{step.title}</span>
								{detail && <span className={`detail${failed ? ' reason' : ''}`}>{detail}</span>}
							</div>
							{(under || prompts.length > 0 || waitingStop || (live && failed)) && (
								<div className="prompt">
									{under && <Progress step={step} />}
									{prompts.map((prompt) => (
										<Prompt
											key={prompt.id}
											prompt={prompt}
											live={live}
											busy={awaiting.some((each) => each.act === 'answer' && each.prompt === prompt.id)}
											refusal={refusals[prompt.id]}
											onAnswer={(value, options) => onAct('answer', { prompt: prompt.id, value }, options)}
										/>
									))}
									{/* spec: PWEB */}
									{live && failed && (
										<div className="bar-state failed">
											<p>Try this step again, or stop the run.</p>
											<div className="row">
												<button className="small" disabled={busy} onClick={() => onAct('retry')}>
													Try again
												</button>
												<button className="small secondary" disabled={busy} onClick={() => onAct('stop')}>
													Stop
												</button>
											</div>
										</div>
									)}
									{waitingStop && (
										<div className="row stop">
											<button className="small secondary" disabled={busy} onClick={() => onAct('stop')}>
												Stop
											</button>
										</div>
									)}
								</div>
							)}
						</li>
					)
				})}
			</ul>
			{/* spec: PWEB */}
			{stopBelow && (
				<div className="row stop">
					<button className="small secondary" disabled={busy} onClick={() => onAct('stop')}>
						Stop
					</button>
				</div>
			)}
			{live && <p className="muted away">You can close this page. Press USER to check back.</p>}
		</section>
	)
}
