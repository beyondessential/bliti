//! nl80211 itself: fetching what each radio can do, and the three settings the applier makes.
//!
//! Reading takes `NL80211_CMD_GET_INTERFACE`, `NL80211_CMD_GET_WIPHY` and `NL80211_CMD_GET_SURVEY`,
//! none of which changes anything. Setting takes `NL80211_CMD_REQ_SET_REG`,
//! `NL80211_CMD_NEW_INTERFACE` and `NL80211_CMD_DEL_INTERFACE`, which are what `iw reg set`,
//! `iw dev … interface add` and `iw dev … del` send.

use std::{collections::BTreeMap, io, path::Path};

use anyhow::{Context as _, bail};
use futures::{StreamExt as _, TryStreamExt as _};
use wl_nl80211::{
	Nl80211Attr, Nl80211Command, Nl80211Error, Nl80211Handle, Nl80211InterfaceType, Nl80211Message,
	Nl80211Survey, Nl80211SurveyInfo,
	packet_core::{DefaultNla, NLM_F_ACK, NLM_F_REQUEST, NetlinkMessage, NetlinkPayload},
	packet_generic::GenlMessage,
};

use super::{RadioInfo, model::Sysfs, wiphy};

/// `NL80211_ATTR_REG_ALPHA2`, which wl-nl80211 does not name.
#[expect(
	dead_code,
	reason = "the applier sets this through iw until its System speaks nl80211"
)]
const REG_ALPHA2: u16 = 33;

/// `EOPNOTSUPP`, which a driver with no survey to give answers `NL80211_CMD_GET_SURVEY` with.
const EOPNOTSUPP: i32 = 95;

/// A connection to nl80211.
#[derive(Debug, Clone)]
pub struct Nl80211 {
	handle: Nl80211Handle,
}

/// A wireless interface, as `NL80211_CMD_GET_INTERFACE` describes it.
#[derive(Debug, Clone)]
struct Interface {
	index: u32,
	name: String,
	wiphy: u32,
	kind: Nl80211InterfaceType,
}

impl Nl80211 {
	/// Open a connection, driven by a task on the current tokio runtime.
	pub fn connect() -> io::Result<Self> {
		let (connection, handle, _) = wl_nl80211::new_connection()?;
		tokio::spawn(connection);
		Ok(Self { handle })
	}

	/// What each radio with a station interface can do, in wiphy order.
	///
	/// A wiphy with no station interface is left out: a document names a radio by its station
	/// interface, so one without is a radio no document can name.
	///
	/// Whether a radio surveys is taken from `surveyed`, keyed by station interface, where it is
	/// there. Only a radio missing from it is asked, since a survey dump retunes the radio across
	/// every channel on some drivers (brcmfmac spends a tenth of a second on each), taking whatever
	/// runs on it off its channel for as long.
	pub async fn radios(
		&self,
		surveyed: &BTreeMap<String, bool>,
	) -> anyhow::Result<Vec<RadioInfo>> {
		let interfaces = self.interfaces().await?;
		let wiphys = self.wiphys().await?;
		let mut radios = Vec::new();
		for (index, attributes) in &wiphys {
			let Some(station) = interfaces.iter().find(|interface| {
				interface.wiphy == *index && interface.kind == Nl80211InterfaceType::Station
			}) else {
				tracing::debug!(
					wiphy = index,
					"no station interface, so not a radio a document can name"
				);
				continue;
			};
			let survey = match surveyed.get(&station.name) {
				Some(survey) => *survey,
				None => self.surveys(station).await,
			};
			let adapter = Sysfs::read(Path::new("/sys"), &station.name);
			radios.push(wiphy::parse(
				attributes,
				station.name.clone(),
				&adapter,
				survey,
			));
		}
		Ok(radios)
	}

	/// Put every radio under `domain`, `00` being the world domain.
	#[expect(
		dead_code,
		reason = "the applier sets this through iw until its System speaks nl80211"
	)]
	pub async fn set_regulatory_domain(&self, domain: &str) -> anyhow::Result<()> {
		let valid =
			domain == "00" || domain.len() == 2 && domain.bytes().all(|b| b.is_ascii_uppercase());
		if !valid {
			bail!("{domain:?} is neither an ISO 3166-1 alpha-2 code nor the world domain 00");
		}
		let mut alpha2 = domain.as_bytes().to_vec();
		alpha2.push(0);
		self.acknowledged(Nl80211Message {
			cmd: Nl80211Command::ReqSetReg,
			attributes: vec![Nl80211Attr::Other(DefaultNla::new(REG_ALPHA2, alpha2))],
		})
		.await
		.with_context(|| format!("cannot set the regulatory domain to {domain}"))
	}

	/// Create the access point interface `interface` on the radio whose station interface is
	/// `radio`, doing nothing where it exists as an access point already.
	#[expect(
		dead_code,
		reason = "the applier sets this through iw until its System speaks nl80211"
	)]
	pub async fn create_access_point(&self, radio: &str, interface: &str) -> anyhow::Result<()> {
		let interfaces = self.interfaces().await?;
		if let Some(existing) = interfaces
			.iter()
			.find(|candidate| candidate.name == interface)
		{
			if existing.kind == Nl80211InterfaceType::Ap {
				return Ok(());
			}
			bail!(
				"{interface} exists and is not an access point ({:?})",
				existing.kind
			);
		}
		let Some(station) = interfaces.iter().find(|candidate| candidate.name == radio) else {
			bail!("{radio} is not a wireless interface");
		};
		self.acknowledged(Nl80211Message {
			cmd: Nl80211Command::NewInterface,
			attributes: vec![
				Nl80211Attr::Wiphy(station.wiphy),
				Nl80211Attr::IfType(Nl80211InterfaceType::Ap),
				Nl80211Attr::IfName(interface.to_owned()),
			],
		})
		.await
		.with_context(|| format!("cannot create {interface} on {radio}'s radio"))
	}

	/// Delete the interface `interface`, doing nothing where it does not exist.
	#[expect(
		dead_code,
		reason = "the applier sets this through iw until its System speaks nl80211"
	)]
	pub async fn delete_access_point(&self, interface: &str) -> anyhow::Result<()> {
		let interfaces = self.interfaces().await?;
		let Some(existing) = interfaces
			.iter()
			.find(|candidate| candidate.name == interface)
		else {
			return Ok(());
		};
		self.acknowledged(Nl80211Message {
			cmd: Nl80211Command::DelInterface,
			attributes: vec![Nl80211Attr::IfIndex(existing.index)],
		})
		.await
		.with_context(|| format!("cannot delete {interface}"))
	}

	async fn interfaces(&self) -> anyhow::Result<Vec<Interface>> {
		let messages: Vec<_> = self
			.handle
			.interface()
			.get(Vec::new())
			.execute()
			.await
			.try_collect()
			.await
			.context("cannot list wireless interfaces")?;
		Ok(messages
			.into_iter()
			.filter_map(|message| {
				let (mut index, mut name, mut wiphy, mut kind) = (None, None, None, None);
				for attribute in message.payload.attributes {
					match attribute {
						Nl80211Attr::IfIndex(value) => index = Some(value),
						Nl80211Attr::IfName(value) => name = Some(value),
						Nl80211Attr::Wiphy(value) => wiphy = Some(value),
						Nl80211Attr::IfType(value) => kind = Some(value),
						_ => {}
					}
				}
				Some(Interface {
					index: index?,
					name: name?,
					wiphy: wiphy?,
					kind: kind?,
				})
			})
			.collect())
	}

	/// Every wiphy's attributes, the messages of the split dump each is sent over put together.
	async fn wiphys(&self) -> anyhow::Result<BTreeMap<u32, Vec<Nl80211Attr>>> {
		let messages: Vec<_> = self
			.handle
			.wireless_physic()
			.get()
			.execute()
			.await
			.try_collect()
			.await
			.context("cannot list radios")?;
		let mut wiphys: BTreeMap<u32, Vec<Nl80211Attr>> = BTreeMap::new();
		for message in messages {
			let attributes = message.payload.attributes;
			let Some(index) = attributes.iter().find_map(|attribute| match attribute {
				Nl80211Attr::Wiphy(index) => Some(*index),
				_ => None,
			}) else {
				continue;
			};
			wiphys.entry(index).or_default().extend(attributes);
		}
		Ok(wiphys)
	}

	/// Whether the driver behind `station` gives a survey of any channel.
	///
	/// Asked with `NL80211_CMD_GET_SURVEY`, a dump that reads what the driver has gathered and changes
	/// nothing on mac80211 drivers. A driver without the operation answers `EOPNOTSUPP`; one that
	/// has it but gathers nothing ends the dump empty, and gives no survey either.
	async fn surveys(&self, station: &Interface) -> bool {
		let answer: Result<Vec<_>, Nl80211Error> = self
			.handle
			.survey()
			.dump(Nl80211Survey::new(station.index).build())
			.execute()
			.await
			.try_collect()
			.await;
		match answer {
			Ok(messages) => messages.iter().any(|message| {
				message.payload.attributes.iter().any(|attribute| {
					matches!(attribute, Nl80211Attr::SurveyInfo(info)
						if info.iter().any(|item| matches!(item, Nl80211SurveyInfo::Frequency(_))))
				})
			}),
			Err(Nl80211Error::NetlinkError(error)) if error.raw_code() == -EOPNOTSUPP => false,
			Err(error) => {
				tracing::warn!(interface = station.name, %error, "the driver did not answer a survey");
				false
			}
		}
	}

	/// Send `message` and wait for the kernel to acknowledge it.
	#[expect(
		dead_code,
		reason = "the applier sets this through iw until its System speaks nl80211"
	)]
	async fn acknowledged(&self, message: Nl80211Message) -> anyhow::Result<()> {
		let mut request = NetlinkMessage::from(GenlMessage::from_payload(message));
		request.header.flags = NLM_F_REQUEST | NLM_F_ACK;
		let mut handle = self.handle.clone();
		let mut replies = handle.request(request).await?;
		while let Some(reply) = replies.next().await {
			if let NetlinkPayload::Error(error) = reply?.payload
				&& error.code.is_some()
			{
				return Err(error.to_io().into());
			}
		}
		Ok(())
	}
}
