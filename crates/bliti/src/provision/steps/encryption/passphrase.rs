//! The recovery passphrase (PENC, "The recovery passphrase").

use diceware_wordlists::MINILOCK_WORDLIST;
use rand::seq::IndexedRandom as _;

/// How many words a recovery passphrase has.
const WORDS: usize = 6;

/// The fewest characters a word has.
const SHORTEST: usize = 5;

/// A new recovery passphrase: six words drawn at random from the miniLock diceware word list, which
/// have five characters or more and only ASCII letters and digits, in lowercase, joined by hyphens.
pub fn generate() -> String {
	let mut rng = rand::rng();
	let mut words = Vec::with_capacity(WORDS);
	while words.len() < WORDS {
		let word = MINILOCK_WORDLIST
			.choose(&mut rng)
			.expect("the word list is not empty");
		if usable(word) {
			words.push(word.to_ascii_lowercase());
		}
	}
	words.join("-")
}

fn usable(word: &str) -> bool {
	word.len() >= SHORTEST && word.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
	use super::*;

	// spec: PENC#the-recovery-passphrase
	#[test]
	fn six_lowercase_words_of_five_characters_or_more_joined_by_hyphens() {
		for _ in 0..200 {
			let passphrase = generate();
			let words: Vec<&str> = passphrase.split('-').collect();
			assert_eq!(words.len(), WORDS, "{passphrase}");
			for word in words {
				assert!(word.len() >= SHORTEST, "{passphrase}");
				assert!(
					word.chars()
						.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
					"{passphrase}"
				);
			}
		}
	}

	// spec: PENC#the-recovery-passphrase
	#[test]
	fn every_word_comes_from_the_list() {
		let passphrase = generate();
		for word in passphrase.split('-') {
			assert!(
				MINILOCK_WORDLIST
					.iter()
					.any(|listed| listed.eq_ignore_ascii_case(word)),
				"{word}"
			);
		}
	}

	// spec: PENC#the-recovery-passphrase
	#[test]
	fn two_passphrases_differ() {
		assert_ne!(generate(), generate());
	}
}
