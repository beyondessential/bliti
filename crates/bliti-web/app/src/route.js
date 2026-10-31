// The client a page holds where it reaches either a real device or the fake one (WEB, PWEB, "The fake
// device"): one client to the page, each call going to whichever the channel is with.

/// A client with every member of `real`, each going to the client `active()` names, apart from those
/// `own` gives. Built from `real` itself, so a member added to it is routed with no list to keep.
export function routed(real, active, own) {
	const each = Object.keys(real).map((name) => [name, (...args) => active()[name](...args)])
	return { ...Object.fromEntries(each), ...own }
}
