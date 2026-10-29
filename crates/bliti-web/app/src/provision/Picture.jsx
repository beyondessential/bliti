import { PICTURES } from './pictures.js'

/// A photograph of the board, with the part marked, and the words that say where it is and what to do
/// with it (PWEB, "Pictures").
export default function Picture({ name }) {
	const picture = PICTURES[name]
	return (
		<>
			<img className="board" src={picture.src} alt={picture.alt} />
			<p className="pic-caption">{picture.caption}</p>
		</>
	)
}
