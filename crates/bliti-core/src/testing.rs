//! Helpers shared by this crate's tests.

/// Scan an SVG image as a camera would: rasterise the dark path and decode whatever code is found.
pub(crate) fn scan_svg(svg: &str) -> String {
	let attr = |name: &str| -> usize {
		let start = svg.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
		let len = svg[start..].find('"').unwrap();
		svg[start..start + len].parse().unwrap()
	};
	let (width, height) = (attr("width"), attr("height"));
	let mut dark = vec![false; width * height];
	// The dark path is a run of rectangles, each `M{left} {top}h{width}v{height}H{left}V{top}`.
	let path = svg.rsplit(" d=\"").next().unwrap();
	let path = &path[..path.find('"').unwrap()];
	for rect in path.split('M').filter(|rect| !rect.is_empty()) {
		let numbers: Vec<usize> = rect
			.split(|c: char| !c.is_ascii_digit())
			.filter(|n| !n.is_empty())
			.map(|n| n.parse().unwrap())
			.collect();
		let [left, top, w, h, ..] = numbers[..] else {
			panic!("not a rectangle: {rect}");
		};
		for y in top..top + h {
			dark[y * width + left..y * width + left + w].fill(true);
		}
	}
	let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
		if dark[y * width + x] { 0 } else { 255 }
	});
	let grids = image.detect_grids();
	assert_eq!(grids.len(), 1, "exactly one code in the image");
	grids[0].decode().unwrap().1
}
