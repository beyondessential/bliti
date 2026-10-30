//! The PDF sheet of labels a provisioning client prints on sticker paper (PSHT, "The sheet").
//!
//! Pure: labels in, bytes out, so the grid and the wording of how full a sheet is are tested here
//! and the page only downloads what this makes.

use bliti_core::qr::QrPayload;
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

/// Labels across a page.
const COLUMNS: usize = 3;

/// Labels down a page.
const ROWS: usize = 9;

/// The positions on a page, which a fully used page holds a label in each of.
pub const POSITIONS: usize = COLUMNS * ROWS;

/// A label's width and height, in millimetres.
const WIDTH: f32 = 50.0;
const HEIGHT: f32 = 30.0;

/// A4, in millimetres.
const PAGE_WIDTH: f32 = 210.0;
const PAGE_HEIGHT: f32 = 297.0;

/// The quiet zone of a QR code, in modules, on each side.
const QUIET: usize = 4;

/// Between the code and the text, and the text's own gap, in millimetres.
const TEXT_AT: f32 = 31.0;

const POINTS_PER_MM: f32 = 72.0 / 25.4;

/// What a label is drawn from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
	/// The text the QR code encodes.
	pub payload: String,
	/// The board's version, as `major.minor`.
	pub version: String,
}

/// The labels of a JSON array of `{ payload, version }`.
pub fn parse(json: &str) -> Result<Vec<Label>, String> {
	let serde_json::Value::Array(items) =
		serde_json::from_str(json).map_err(|err| err.to_string())?
	else {
		return Err("labels are an array".to_owned());
	};
	items
		.iter()
		.map(|item| {
			let member = |name: &str| {
				item.get(name)
					.and_then(serde_json::Value::as_str)
					.map(str::to_owned)
					.ok_or_else(|| format!("a label has no {name}"))
			};
			Ok(Label {
				payload: member("payload")?,
				version: member("version")?,
			})
		})
		.collect()
}

/// How full the sheet of `count` labels is, as the action that downloads it says (PSHT).
pub fn fill(count: usize) -> String {
	let pages = count.div_ceil(POSITIONS).max(1);
	let last = count - POSITIONS * (pages - 1);
	let percent = (last * 100 + POSITIONS / 2) / POSITIONS;
	match (pages, last == POSITIONS) {
		(1, true) => "1 sheet".to_owned(),
		(pages, true) => format!("{pages} sheets"),
		(1, false) => format!("{percent}% filled"),
		(pages, false) => format!("{pages} sheets, last {percent}% filled"),
	}
}

/// The sheet for `labels`, in order, as the bytes of a PDF.
// spec: PSHT
pub fn pdf(labels: &[Label]) -> Result<Vec<u8>, String> {
	let drawn = labels
		.iter()
		.map(Drawn::of)
		.collect::<Result<Vec<_>, _>>()?;

	let pages = drawn.chunks(POSITIONS).collect::<Vec<_>>();
	let catalog = Ref::new(1);
	let tree = Ref::new(2);
	let sans = Ref::new(3);
	let mono = Ref::new(4);
	let first_page = 5;
	let page_ref = |index: usize| Ref::new(first_page + 2 * index as i32);
	let content_ref = |index: usize| Ref::new(first_page + 2 * index as i32 + 1);

	let mut pdf = Pdf::new();
	pdf.catalog(catalog).pages(tree);
	pdf.pages(tree)
		.kids((0..pages.len()).map(page_ref))
		.count(pages.len() as i32);
	pdf.type1_font(sans).base_font(Name(b"Helvetica"));
	pdf.type1_font(mono).base_font(Name(b"Courier-Bold"));

	for (index, page) in pages.iter().enumerate() {
		let mut sheet = pdf.page(page_ref(index));
		sheet
			.parent(tree)
			.media_box(Rect::new(0.0, 0.0, mm(PAGE_WIDTH), mm(PAGE_HEIGHT)))
			.contents(content_ref(index));
		sheet
			.resources()
			.fonts()
			.pair(Name(b"F1"), sans)
			.pair(Name(b"F2"), mono);
		sheet.finish();
		pdf.stream(content_ref(index), &draw(page).finish());
	}
	// An empty sheet is a blank page, not a document with no pages.
	if pages.is_empty() {
		pdf.pages(tree).kids([page_ref(0)]).count(1);
		pdf.page(page_ref(0)).parent(tree).media_box(Rect::new(
			0.0,
			0.0,
			mm(PAGE_WIDTH),
			mm(PAGE_HEIGHT),
		));
	}
	Ok(pdf.finish())
}

/// A label as it is drawn.
struct Drawn {
	suffix: String,
	product: String,
	/// The modules of the code, row by row, true where dark.
	modules: Vec<bool>,
	width: usize,
}

impl Drawn {
	fn of(label: &Label) -> Result<Self, String> {
		let payload = QrPayload::read(&label.payload)
			.map_err(|err| format!("a label's payload is not one: {err}"))?;
		let numbered = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
		match label.version.split_once('.') {
			Some((major, minor)) if numbered(major) && numbered(minor) => {}
			_ => return Err(format!("{:?} is not a version", label.version)),
		}
		let code = payload.to_qr_code();
		let width = code.width();
		let modules = (0..width * width)
			.map(|at| code[(at % width, at / width)].select(true, false))
			.collect();
		Ok(Self {
			suffix: payload.suffix(),
			product: format!("Tamanu Iti v{}", label.version),
			modules,
			width,
		})
	}
}

/// Millimetres in points.
fn mm(length: f32) -> f32 {
	length * POINTS_PER_MM
}

/// One page: the labels in rows from the top left, each with its cut line.
fn draw(labels: &[Drawn]) -> Content {
	let mut content = Content::new();
	let left = (PAGE_WIDTH - COLUMNS as f32 * WIDTH) / 2.0;
	let top = (PAGE_HEIGHT - ROWS as f32 * HEIGHT) / 2.0;
	for (at, label) in labels.iter().enumerate() {
		let (x, y) = (
			left + (at % COLUMNS) as f32 * WIDTH,
			PAGE_HEIGHT - top - (at / COLUMNS + 1) as f32 * HEIGHT,
		);
		cut_line(&mut content, x, y);
		code(&mut content, label, x, y);
		text(&mut content, label, x, y);
	}
	content
}

/// The outline to cut along, around the label whose lower left corner is at `x` and `y`.
fn cut_line(content: &mut Content, x: f32, y: f32) {
	content
		.save_state()
		.set_line_width(0.5)
		.set_stroke_gray(0.65)
		.set_dash_pattern([3.0, 3.0], 0.0)
		.rect(mm(x), mm(y), mm(WIDTH), mm(HEIGHT))
		.stroke()
		.restore_state();
}

/// The code, as high as the label, with its quiet zone, in runs of dark modules.
fn code(content: &mut Content, label: &Drawn, x: f32, y: f32) {
	let module = HEIGHT / (label.width + 2 * QUIET) as f32;
	content.save_state().set_fill_gray(0.0);
	for row in 0..label.width {
		let mut column = 0;
		while column < label.width {
			if !label.modules[row * label.width + column] {
				column += 1;
				continue;
			}
			let start = column;
			while column < label.width && label.modules[row * label.width + column] {
				column += 1;
			}
			content
				.rect(
					mm(x + (QUIET + start) as f32 * module),
					mm(y + HEIGHT - (QUIET + row + 1) as f32 * module),
					mm((column - start) as f32 * module),
					mm(module),
				)
				.fill_nonzero();
		}
	}
	content.restore_state();
}

/// The suffix large, and the product small beneath it, centred on the label's height.
fn text(content: &mut Content, label: &Drawn, x: f32, y: f32) {
	let at = x + TEXT_AT;
	let middle = y + HEIGHT / 2.0;
	content
		.save_state()
		.set_fill_gray(0.0)
		.begin_text()
		.set_font(Name(b"F2"), 15.0)
		.next_line(mm(at), mm(middle))
		.show(Str(label.suffix.as_bytes()))
		.end_text()
		.begin_text()
		.set_fill_gray(0.35)
		.set_font(Name(b"F1"), 6.5)
		.next_line(mm(at), mm(middle - 3.3))
		.show(Str(label.product.as_bytes()))
		.end_text()
		.restore_state();
}

#[cfg(test)]
mod tests {
	use bliti_core::key_schedule::Root;

	use super::*;

	fn label(seed: u8) -> Label {
		let keys = Root::from_bytes([seed; 32]).device_keys();
		let payload = QrPayload::new(keys.presence_token.clone(), keys.fingerprint());
		Label {
			payload: payload.to_text(),
			version: "4.0".to_owned(),
		}
	}

	fn labels(count: usize) -> Vec<Label> {
		(0..count).map(|seed| label(seed as u8 + 1)).collect()
	}

	fn text_of(bytes: &[u8]) -> String {
		String::from_utf8_lossy(bytes).into_owned()
	}

	fn pages(bytes: &[u8]) -> usize {
		text_of(bytes).matches("/Type /Page\n").count()
	}

	/// The rectangles stroked, as their four numbers.
	fn cut_lines(bytes: &[u8]) -> Vec<[f32; 4]> {
		let text = text_of(bytes);
		let lines = text.lines().collect::<Vec<_>>();
		lines
			.windows(2)
			.filter(|pair| pair[0].ends_with(" re") && pair[1] == "S")
			.map(|pair| {
				let numbers = pair[0].split(' ').collect::<Vec<_>>();
				std::array::from_fn(|at| numbers[at].parse().unwrap())
			})
			.collect()
	}

	fn close(a: f32, b: f32) -> bool {
		(a - b).abs() < 0.05
	}

	#[test]
	fn a_page_holds_three_across_and_nine_down() {
		assert_eq!(POSITIONS, 27);
	}

	#[test]
	fn pages_are_added_as_one_is_filled() {
		for (count, expected) in [(1, 1), (27, 1), (28, 2), (54, 2), (55, 3)] {
			let sheet = pdf(&labels(count)).unwrap();
			assert_eq!(pages(&sheet), expected, "{count} labels");
			assert!(sheet.starts_with(b"%PDF-"));
		}
	}

	#[test]
	fn the_page_is_a4_in_portrait() {
		let text = text_of(&pdf(&labels(1)).unwrap());
		let (width, height) = (mm(210.0), mm(297.0));
		assert!(
			text.contains(&format!("/MediaBox [0 0 {} {}]", width, height)),
			"{text}"
		);
	}

	#[test]
	fn labels_fill_each_row_from_the_left_and_the_page_from_the_top() {
		let lines = cut_lines(&pdf(&labels(28)).unwrap());
		assert_eq!(lines.len(), 28);
		let at = |index: usize| (lines[index][0], lines[index][1]);
		let (width, height) = (mm(50.0), mm(30.0));
		let (left, bottom) = (mm(30.0), mm(297.0 - 13.5 - 30.0));
		// The first, the second along, the first of the second row, and the first of the second page.
		assert!(close(at(0).0, left) && close(at(0).1, bottom));
		assert!(close(at(1).0, left + width) && close(at(1).1, bottom));
		assert!(close(at(3).0, left) && close(at(3).1, bottom - height));
		assert!(close(at(27).0, left) && close(at(27).1, bottom));
		assert!(
			lines
				.iter()
				.all(|l| close(l[2], width) && close(l[3], height))
		);
	}

	#[test]
	fn only_a_position_holding_a_label_is_cut_around() {
		assert_eq!(cut_lines(&pdf(&labels(4)).unwrap()).len(), 4);
	}

	#[test]
	fn a_label_carries_the_suffix_and_the_product_and_nothing_of_the_record() {
		let one = label(9);
		let suffix = QrPayload::read(&one.payload).unwrap().suffix();
		let text = text_of(&pdf(&[one]).unwrap());
		assert!(text.contains(&format!("({suffix}) Tj")));
		assert!(text.contains("(Tamanu Iti v4.0) Tj"));
		assert_eq!(text.matches(" Tj").count(), 2);
	}

	#[test]
	fn the_code_fills_the_height_of_the_label() {
		// Version 5 is 37 modules, and 45 with its quiet zone, which is the label's 30 mm.
		let one = label(3);
		let width = QrPayload::read(&one.payload).unwrap().to_qr_code().width();
		assert_eq!(width, 37);
		let module = mm(30.0) / (width + 2 * QUIET) as f32;
		let text = text_of(&pdf(&[one]).unwrap());
		let heights = text
			.lines()
			.collect::<Vec<_>>()
			.windows(2)
			.filter(|pair| pair[0].ends_with(" re") && pair[1] == "f")
			.map(|pair| pair[0].split(' ').nth(3).unwrap().parse::<f32>().unwrap())
			.collect::<Vec<_>>();
		assert!(!heights.is_empty());
		assert!(heights.iter().all(|height| close(*height, module)));
	}

	#[test]
	fn labels_are_read_from_json() {
		let json = format!(
			r#"[{{"payload":"{}","version":"4.0","at":"ignored"}}]"#,
			label(1).payload
		);
		assert_eq!(parse(&json).unwrap(), vec![label(1)]);
		assert!(parse("{}").is_err());
		assert!(parse(r#"[{"payload":"x"}]"#).is_err());
	}

	#[test]
	fn a_payload_that_is_not_one_is_refused() {
		let bad = Label {
			payload: "BLITI:NOPE".to_owned(),
			version: "4.0".to_owned(),
		};
		assert!(pdf(&[bad]).is_err());
	}

	#[test]
	fn a_version_that_is_not_one_is_refused() {
		let mut bad = label(1);
		bad.version = "4.0) Tj (".to_owned();
		assert!(pdf(&[bad]).is_err());
	}

	#[test]
	fn how_full_a_sheet_is_reads_as_the_spec_has_it() {
		assert_eq!(fill(1), "4% filled");
		assert_eq!(fill(4), "15% filled");
		assert_eq!(fill(26), "96% filled");
		assert_eq!(fill(27), "1 sheet");
		assert_eq!(fill(28), "2 sheets, last 4% filled");
		assert_eq!(fill(54), "2 sheets");
		assert_eq!(fill(27 * 2 + 19), "3 sheets, last 70% filled");
	}
}
