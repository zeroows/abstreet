use crate::{Color, GeomBatch};
use abstutil::VecMap;
use geom::{Bounds, Polygon, Pt2D};
use lyon::math::Point;
use lyon::path::PathEvent;
use lyon::tessellation;
use lyon::tessellation::geometry_builder::{simple_builder, VertexBuffers};

const TOLERANCE: f32 = 0.01;

// Code here adapted from
// https://github.com/nical/lyon/blob/b5c87c9a22dccfab24daa1947419a70915d60914/examples/wgpu_svg/src/main.rs.

// No offset. I'm not exactly sure how the simplification in usvg works, but this doesn't support
// transforms or strokes or text, just fills. Luckily, all of the files exported from Figma so far
// work just fine.
pub fn add_svg(batch: &mut GeomBatch, filename: &str) -> Bounds {
    let mut fill_tess = tessellation::FillTessellator::new();
    let mut stroke_tess = tessellation::StrokeTessellator::new();
    let mut mesh_per_color: VecMap<Color, VertexBuffers<_, u16>> = VecMap::new();

    let svg_tree = usvg::Tree::from_file(&filename, &usvg::Options::default()).unwrap();
    for node in svg_tree.root().descendants() {
        if let usvg::NodeKind::Path(ref p) = *node.borrow() {
            // TODO Handle transforms

            if let Some(ref fill) = p.fill {
                let color = convert_color(&fill.paint, fill.opacity.value());
                let geom = mesh_per_color.mut_or_insert(color, VertexBuffers::new);
                fill_tess
                    .tessellate(
                        convert_path(p),
                        &tessellation::FillOptions::tolerance(TOLERANCE),
                        &mut simple_builder(geom),
                    )
                    .expect(&format!("Couldn't tesellate something from {}", filename));
            }

            if let Some(ref stroke) = p.stroke {
                let (color, stroke_opts) = convert_stroke(stroke);
                let geom = mesh_per_color.mut_or_insert(color, VertexBuffers::new);
                stroke_tess
                    .tessellate(convert_path(p), &stroke_opts, &mut simple_builder(geom))
                    .unwrap();
            }
        }
    }

    for (color, mesh) in mesh_per_color.consume() {
        batch.push(
            color,
            Polygon::precomputed(
                mesh.vertices
                    .into_iter()
                    .map(|v| Pt2D::new(f64::from(v.x), f64::from(v.y)))
                    .collect(),
                mesh.indices.into_iter().map(|idx| idx as usize).collect(),
                None,
            ),
        );
    }
    let size = svg_tree.svg_node().size;
    Bounds::from(&vec![
        Pt2D::new(0.0, 0.0),
        Pt2D::new(size.width(), size.height()),
    ])
}

fn point(x: &f64, y: &f64) -> Point {
    Point::new((*x) as f32, (*y) as f32)
}

pub struct PathConvIter<'a> {
    iter: std::slice::Iter<'a, usvg::PathSegment>,
    prev: Point,
    first: Point,
    needs_end: bool,
    deferred: Option<PathEvent>,
}

impl<'l> Iterator for PathConvIter<'l> {
    type Item = PathEvent;
    fn next(&mut self) -> Option<PathEvent> {
        if self.deferred.is_some() {
            return self.deferred.take();
        }

        match self.iter.next() {
            Some(usvg::PathSegment::MoveTo { x, y }) => {
                if self.needs_end {
                    let last = self.prev;
                    let first = self.first;
                    self.needs_end = false;
                    self.deferred = Some(PathEvent::Begin { at: self.prev });
                    self.prev = point(x, y);
                    self.first = self.prev;
                    Some(PathEvent::End {
                        last,
                        first,
                        close: false,
                    })
                } else {
                    Some(PathEvent::Begin { at: self.prev })
                }
            }
            Some(usvg::PathSegment::LineTo { x, y }) => {
                self.needs_end = true;
                let from = self.prev;
                self.prev = point(x, y);
                Some(PathEvent::Line {
                    from,
                    to: self.prev,
                })
            }
            Some(usvg::PathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            }) => {
                self.needs_end = true;
                let from = self.prev;
                self.prev = point(x, y);
                Some(PathEvent::Cubic {
                    from,
                    ctrl1: point(x1, y1),
                    ctrl2: point(x2, y2),
                    to: self.prev,
                })
            }
            Some(usvg::PathSegment::ClosePath) => {
                self.needs_end = false;
                self.prev = self.first;
                Some(PathEvent::End {
                    last: self.prev,
                    first: self.first,
                    close: true,
                })
            }
            None => None,
        }
    }
}

pub fn convert_path<'a>(p: &'a usvg::Path) -> PathConvIter<'a> {
    PathConvIter {
        iter: p.data.0.iter(),
        first: Point::new(0.0, 0.0),
        prev: Point::new(0.0, 0.0),
        deferred: None,
        needs_end: false,
    }
}

fn convert_stroke(s: &usvg::Stroke) -> (Color, tessellation::StrokeOptions) {
    let color = convert_color(&s.paint, s.opacity.value());
    let linecap = match s.linecap {
        usvg::LineCap::Butt => tessellation::LineCap::Butt,
        usvg::LineCap::Square => tessellation::LineCap::Square,
        usvg::LineCap::Round => tessellation::LineCap::Round,
    };
    let linejoin = match s.linejoin {
        usvg::LineJoin::Miter => tessellation::LineJoin::Miter,
        usvg::LineJoin::Bevel => tessellation::LineJoin::Bevel,
        usvg::LineJoin::Round => tessellation::LineJoin::Round,
    };

    let opt = tessellation::StrokeOptions::tolerance(TOLERANCE)
        .with_line_width(s.width.value() as f32)
        .with_line_cap(linecap)
        .with_line_join(linejoin);

    (color, opt)
}

fn convert_color(paint: &usvg::Paint, opacity: f64) -> Color {
    if let usvg::Paint::Color(c) = paint {
        Color::rgba(
            c.red as usize,
            c.green as usize,
            c.blue as usize,
            opacity as f32,
        )
    } else {
        panic!("Unsupported paint {:?}", paint);
    }
}
