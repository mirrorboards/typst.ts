//! Source locations from the same layout snapshot as the exported PDF.
use reflexo_typst::vfs::{FsProvider, WorkspaceResolver};
use typst::{
    layout::{Abs, Frame, FrameItem, Point, Size, Transform},
    syntax::Span,
    World, WorldExt,
};

pub fn collect(
    world: &(impl World + FsProvider),
    document: &typst::layout::PagedDocument,
) -> serde_json::Value {
    let mut entries = Vec::new();
    for (index, page) in document.pages.iter().enumerate() {
        visit(
            world,
            &page.frame,
            Transform::identity(),
            page.frame.size(),
            index + 1,
            &mut entries,
        );
    }
    serde_json::Value::Array(entries)
}

fn visit(
    world: &(impl World + FsProvider),
    frame: &Frame,
    transform: Transform,
    page_size: Size,
    page: usize,
    entries: &mut Vec<serde_json::Value>,
) {
    for (position, item) in frame.items() {
        let local = transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => visit(
                world,
                &group.frame,
                local.pre_concat(group.transform),
                page_size,
                page,
                entries,
            ),
            FrameItem::Text(text) => {
                let mut cursor = Point::zero();
                for glyph in &text.glyphs {
                    let advance = glyph.x_advance.at(text.size);
                    let origin = Point::new(
                        cursor.x + glyph.x_offset.at(text.size),
                        -cursor.y - glyph.y_offset.at(text.size) - text.size,
                    );
                    if let Some(location) = source_location(
                        world,
                        glyph.span.0,
                        glyph.span.1 as usize,
                        glyph.range().len(),
                    ) {
                        entries.push(entry(
                            location,
                            page,
                            origin,
                            Size::new(advance, text.size),
                            local,
                            page_size,
                        ));
                    }
                    cursor.x += advance;
                    cursor.y += glyph.y_advance.at(text.size);
                }
            }
            FrameItem::Image(_, size, span) => {
                if let Some(location) = source_location(world, *span, 0, 0) {
                    entries.push(entry(
                        location,
                        page,
                        Point::zero(),
                        *size,
                        local,
                        page_size,
                    ));
                }
            }
            _ => {}
        }
    }
}

fn source_location(
    world: &(impl World + FsProvider),
    span: Span,
    offset: usize,
    length: usize,
) -> Option<(String, usize, usize)> {
    let id = span.id()?;
    // Package/generated content has no editable room file.
    if WorkspaceResolver::is_package_file(id) {
        return None;
    }
    let source = world.source(id).ok()?;
    let range = world.range(span)?;
    let start = (range.start + offset).min(range.end);
    let end = (start + length).min(range.end);
    let start = source.lines().byte_to_utf16(start)?;
    let end = source.lines().byte_to_utf16(end)?;
    Some((
        world
            .file_path(id)
            .ok()?
            .to_err()
            .ok()?
            .to_string_lossy()
            .into_owned(),
        start,
        end,
    ))
}

fn entry(
    (path, start, end): (String, usize, usize),
    page: usize,
    origin: Point,
    size: Size,
    transform: Transform,
    page_size: Size,
) -> serde_json::Value {
    let corners = [
        origin,
        origin + Point::new(size.x, Abs::zero()),
        origin + Point::new(Abs::zero(), size.y),
        origin + Point::new(size.x, size.y),
    ]
    .map(|point| point.transform(transform));
    let x = corners
        .iter()
        .map(|p| p.x.to_pt())
        .fold(f64::INFINITY, f64::min);
    let y = corners
        .iter()
        .map(|p| p.y.to_pt())
        .fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|p| p.x.to_pt())
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners
        .iter()
        .map(|p| p.y.to_pt())
        .fold(f64::NEG_INFINITY, f64::max);
    serde_json::json!({ "path": path, "start": start, "end": end, "page": page,
        "x": x / page_size.x.to_pt(), "y": y / page_size.y.to_pt(),
        "width": (right - x) / page_size.x.to_pt(), "height": (bottom - y) / page_size.y.to_pt() })
}
