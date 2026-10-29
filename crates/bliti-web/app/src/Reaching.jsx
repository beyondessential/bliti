// The pieces the pages that reach a device share: where the operator is in reaching it, and a
// problem reaching it (WEB, PWEB).

/// The dots of the steps of reaching a device, `names` in order, the ones before `at` done and `at`
/// the one the operator is on.
export function Steps({ names, at }) {
	return (
		<ol className="steps" aria-label="Progress">
			{names.map((name, index) => (
				<li
					key={name}
					className={index < at ? 'done' : index === at ? 'current' : undefined}
					aria-current={index === at ? 'step' : undefined}
				>
					<span className="dot">{index < at ? '✓' : index + 1}</span>
					<span className="name">{name}</span>
				</li>
			))}
		</ol>
	)
}

/// A problem, said as the one line a screen leads with.
export function Problem({ children }) {
	return (
		<p className="lead problem">
			<span className="alert" aria-hidden="true">
				!
			</span>
			{children}
		</p>
	)
}
