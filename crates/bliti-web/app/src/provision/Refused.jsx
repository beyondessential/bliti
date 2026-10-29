// A medium the page will not drive, which shows no steps and sends no act (PWEB, "Refusing a medium").

import { showDate } from './state.js'

/// `refusal` is what `refusal()` of state.js gave.
export default function Refused({ refusal }) {
	if (refusal.kind === 'page') {
		// spec: PWEB
		return (
			<section>
				<div className="notice fault">
					<p className="lead">This page is out of date</p>
					<p>Reload it to provision this Iti.</p>
				</div>
				<button className="block" onClick={() => window.location.reload()}>
					Reload
				</button>
			</section>
		)
	}
	// spec: PWEB
	return (
		<section>
			<div className="notice fault">
				<p className="lead">This USB stick is out of date</p>
				{refusal.built && <p>Its image was built on {showDate(refusal.built)}.</p>}
				<p>Write the latest provisioning image to it, then start again.</p>
			</div>
		</section>
	)
}
