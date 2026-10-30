// The photographs of the board the page shows, each with the part it is about marked in a circle
// (PWEB, "Pictures"). Imported by URL so the build hashes them and the service worker holds them.

import caseSwitch from './pictures/case-switch.jpg?url'
import userButton from './pictures/user-button.jpg?url'
import wpPads from './pictures/wp-pads.jpg?url'

/// Each picture, by the name `pictureFor` gives it, with its caption. The words are in the caption and
/// not the picture, so they stay legible at a phone's width.
export const PICTURES = {
	'user-button': {
		src: userButton,
		alt: 'The board with the USER button circled',
		caption: 'USER is the button at the top, next to the TPM chip.',
	},
	'case-switch': {
		src: caseSwitch,
		alt: 'The board with the CASE switch circled',
		caption: 'CASE is the lever in the corner. Push it in, then let it spring back.',
	},
	'wp-pads': {
		src: wpPads,
		alt: 'The board with the ID WP pads circled',
		caption: 'The two round pads marked ID WP, beside the header.',
	},
}
