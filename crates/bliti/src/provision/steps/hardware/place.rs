//! Placing a board as a version, from the devices on its HAT (PHW, "Placing the board").

use std::sync::Arc;

use crate::{facts::i2c, facts::product::Version};

use super::super::blocking;

/// The chip ID of a Bosch BME680.
const BME680: u8 = 0x61;

/// What answers at the air sensor's address.
pub trait Air: Send + Sync {
	/// The chip ID the address answers with.
	fn chip_id(&self) -> Result<u8, i2c::Error>;
}

/// What the devices on the board showed.
struct Readings {
	air: Option<u8>,
}

/// A version, and where a board is placed as it.
type Row = (Version, fn(&Readings) -> bool);

/// Each version.
const TABLE: [Row; 1] = [(Version { major: 4, minor: 0 }, |seen| {
	seen.air == Some(BME680)
})];

/// The version of the one row of the table the board matches, and nothing where it matches none or
/// more than one.
// spec: PHW
pub async fn place<A: Air + ?Sized + 'static>(air: &Arc<A>) -> Option<Version> {
	let air = air.clone();
	let air = blocking(move || air.chip_id()).await.ok();
	fit(&TABLE, &Readings { air })
}

fn fit(table: &[Row], seen: &Readings) -> Option<Version> {
	let mut rows = table.iter().filter(|(_, matches)| matches(seen));
	let (version, _) = rows.next()?;
	rows.next().is_none().then_some(*version)
}

#[cfg(test)]
mod tests {
	use super::*;

	struct Chip(Result<u8, fn() -> i2c::Error>);

	impl Air for Chip {
		fn chip_id(&self) -> Result<u8, i2c::Error> {
			self.0.map_err(|err| err())
		}
	}

	async fn placed(chip: Result<u8, fn() -> i2c::Error>) -> Option<Version> {
		place(&(Arc::new(Chip(chip)) as Arc<dyn Air>)).await
	}

	// spec: PHW
	#[tokio::test]
	async fn a_bme680_places_the_board_as_4_0() {
		assert_eq!(placed(Ok(0x61)).await, Some(Version { major: 4, minor: 0 }));
	}

	// spec: PHW
	#[tokio::test]
	async fn another_chip_places_nothing() {
		assert_eq!(placed(Ok(0x58)).await, None);
		assert_eq!(placed(Ok(0x00)).await, None);
		assert_eq!(placed(Ok(0xff)).await, None);
	}

	// spec: PHW
	#[tokio::test]
	async fn nothing_answering_places_nothing() {
		assert_eq!(placed(Err(|| i2c::Error::NoAnswer)).await, None);
		assert_eq!(placed(Err(|| i2c::Error::NoBus)).await, None);
		assert_eq!(placed(Err(|| i2c::Error::Denied)).await, None);
	}

	/// Two rows matching one board are no placement, whichever is right.
	// spec: PHW
	#[test]
	fn a_board_two_rows_match_is_not_placed() {
		const TWICE: [Row; 2] = [
			(Version { major: 4, minor: 0 }, |_| true),
			(Version { major: 4, minor: 1 }, |_| true),
		];
		let seen = Readings { air: Some(BME680) };
		assert_eq!(fit(&TWICE, &seen), None);
		assert_eq!(fit(&TABLE, &seen), Some(Version { major: 4, minor: 0 }));
	}
}
