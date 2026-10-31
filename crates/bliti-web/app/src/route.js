// The client a page holds where it reaches either a real device or the fake one (WEB, PWEB, "The fake
// device"): one client to the page, each call going to whichever the channel is with.

/// A client with every member of `real`, each going to the client `active()` names, either `real` or
/// `fake`, apart from those `own` gives. Built from `real` itself, so a member added to it is routed
/// with no list to keep, and checked against `fake`, so one the fake lacks fails as the page loads.
/// A member that starts a channel decides which client is active, so it is one `own` gives.
export function routed(real, fake, active, own) {
	const forwarded = Object.keys(real).filter((name) => !Object.hasOwn(own, name))
	const missing = forwarded.filter((name) => typeof fake[name] !== 'function')
	if (missing.length) throw new Error(`The fake device lacks ${missing.join(', ')}.`)
	const each = forwarded.map((name) => [name, (...args) => active()[name](...args)])
	return { ...Object.fromEntries(each), ...own }
}
