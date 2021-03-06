use crate::layout::Widget;
use crate::svg;
use crate::{
    text, Color, DrawBoth, EventCtx, GeomBatch, GfxCtx, Line, MultiKey, RewriteColor, ScreenDims,
    ScreenPt, Text,
};
use geom::{Distance, Polygon};

pub struct Button {
    pub action: String,

    // Both of these must have the same dimensions and are oriented with their top-left corner at
    // 0, 0. Transformation happens later.
    draw_normal: DrawBoth,
    draw_hovered: DrawBoth,

    hotkey: Option<MultiKey>,
    tooltip: Text,
    // Screenspace, top-left always at the origin. Also, probably not a box. :P
    hitbox: Polygon,

    hovering: bool,
    clicked: bool,

    top_left: ScreenPt,
    dims: ScreenDims,
}

impl Button {
    pub fn new(
        draw_normal: DrawBoth,
        draw_hovered: DrawBoth,
        hotkey: Option<MultiKey>,
        tooltip: &str,
        hitbox: Polygon,
    ) -> Button {
        let dims = draw_normal.get_dims();
        assert_eq!(dims, draw_hovered.get_dims());
        assert!(!tooltip.is_empty());
        Button {
            action: tooltip.to_string(),

            draw_normal,
            draw_hovered,
            hotkey,
            tooltip: if let Some(key) = hotkey {
                let mut txt = Text::from(Line(key.describe()).fg(text::HOTKEY_COLOR)).with_bg();
                txt.append(Line(format!(" - {}", tooltip)));
                txt
            } else {
                Text::from(Line(tooltip)).with_bg()
            },
            hitbox,

            hovering: false,
            clicked: false,

            top_left: ScreenPt::new(0.0, 0.0),
            dims,
        }
    }

    fn get_hitbox(&self) -> Polygon {
        self.hitbox.translate(self.top_left.x, self.top_left.y)
    }

    pub fn event(&mut self, ctx: &mut EventCtx) {
        if self.clicked {
            panic!("Caller didn't consume button click");
        }

        if ctx.redo_mouseover() {
            self.hovering = self
                .get_hitbox()
                .contains_pt(ctx.canvas.get_cursor_in_screen_space().to_pt());
        }
        if self.hovering && ctx.normal_left_click() {
            self.clicked = true;
        }

        if let Some(hotkey) = self.hotkey {
            if ctx.input.new_was_pressed(hotkey) {
                self.clicked = true;
            }
        }

        if self.hovering {
            // Once we asserted this was None, but because of just_replaced, sometimes not true.
            ctx.canvas.button_tooltip = Some(self.tooltip.clone());
        }
    }

    pub fn just_replaced(&mut self, ctx: &EventCtx) {
        self.hovering = self
            .get_hitbox()
            .contains_pt(ctx.canvas.get_cursor_in_screen_space().to_pt());
    }

    pub fn clicked(&mut self) -> bool {
        if self.clicked {
            self.clicked = false;
            true
        } else {
            false
        }
    }

    pub fn draw(&self, g: &mut GfxCtx) {
        if self.hovering {
            self.draw_hovered.redraw(self.top_left, g);
        } else {
            self.draw_normal.redraw(self.top_left, g);
        }

        g.canvas
            .covered_polygons
            .borrow_mut()
            .push(self.get_hitbox());
    }
}

impl Widget for Button {
    fn get_dims(&self) -> ScreenDims {
        self.dims
    }

    fn set_pos(&mut self, top_left: ScreenPt) {
        self.top_left = top_left;
    }
}

// Stuff to construct different types of buttons

const HORIZ_PADDING: f64 = 30.0;
const VERT_PADDING: f64 = 10.0;

// TODO Simplify all of these APIs!
impl Button {
    pub fn rectangle_img(
        filename: &str,
        key: Option<MultiKey>,
        ctx: &EventCtx,
        label: &str,
    ) -> Button {
        let img_color = ctx.canvas.texture(filename);
        let dims = img_color.texture_dims();
        let img_rect =
            Polygon::rectangle(dims.width, dims.height).translate(HORIZ_PADDING, VERT_PADDING);
        let bg = Polygon::rounded_rectangle(
            Distance::meters(dims.width + 2.0 * HORIZ_PADDING),
            Distance::meters(dims.height + 2.0 * VERT_PADDING),
            Distance::meters(VERT_PADDING),
        );

        let normal = DrawBoth::new(
            ctx,
            GeomBatch::from(vec![
                (Color::WHITE, bg.clone()),
                (img_color, img_rect.clone()),
            ]),
            vec![],
        );
        let hovered = DrawBoth::new(
            ctx,
            GeomBatch::from(vec![
                (Color::ORANGE, bg.clone()),
                (img_color, img_rect.clone()),
            ]),
            vec![],
        );
        Button::new(normal, hovered, key, label, bg)
    }

    pub fn rectangle_svg(
        filename: &str,
        tooltip: &str,
        key: Option<MultiKey>,
        hover: RewriteColor,
        ctx: &EventCtx,
    ) -> Button {
        let mut normal = GeomBatch::new();
        let bounds = svg::add_svg(&mut normal, filename);

        let mut hovered = normal.clone();
        hovered.rewrite_color(hover);

        Button::new(
            DrawBoth::new(ctx, normal, Vec::new()),
            DrawBoth::new(ctx, hovered, Vec::new()),
            key,
            tooltip,
            bounds.get_rectangle(),
        )
    }

    pub fn rectangle_svg_rewrite(
        filename: &str,
        tooltip: &str,
        key: Option<MultiKey>,
        normal_rewrite: RewriteColor,
        hover: RewriteColor,
        ctx: &EventCtx,
    ) -> Button {
        let mut normal = GeomBatch::new();
        let bounds = svg::add_svg(&mut normal, filename);
        normal.rewrite_color(normal_rewrite);

        let mut hovered = normal.clone();
        hovered.rewrite_color(hover);

        Button::new(
            DrawBoth::new(ctx, normal, Vec::new()),
            DrawBoth::new(ctx, hovered, Vec::new()),
            key,
            tooltip,
            bounds.get_rectangle(),
        )
    }

    pub fn rectangle_svg_bg(
        filename: &str,
        tooltip: &str,
        key: Option<MultiKey>,
        bg_color: Color,
        hover: Color,
        ctx: &EventCtx,
    ) -> Button {
        let mut throwaway = GeomBatch::new();
        let bounds = svg::add_svg(&mut throwaway, filename);

        let mut normal = GeomBatch::new();
        normal.push(bg_color, bounds.get_rectangle());
        svg::add_svg(&mut normal, filename);

        let mut hovered = GeomBatch::new();
        hovered.push(hover, bounds.get_rectangle());
        svg::add_svg(&mut hovered, filename);

        Button::new(
            DrawBoth::new(ctx, normal, Vec::new()),
            DrawBoth::new(ctx, hovered, Vec::new()),
            key,
            tooltip,
            bounds.get_rectangle(),
        )
    }

    pub fn text(
        text: Text,
        unselected_bg_color: Color,
        selected_bg_color: Color,
        hotkey: Option<MultiKey>,
        tooltip: &str,
        ctx: &EventCtx,
    ) -> Button {
        let dims = ctx.text_dims(&text);
        let geom = Polygon::rounded_rectangle(
            Distance::meters(dims.width + 2.0 * HORIZ_PADDING),
            Distance::meters(dims.height + 2.0 * VERT_PADDING),
            Distance::meters(VERT_PADDING),
        );
        let draw_text = vec![(text, ScreenPt::new(HORIZ_PADDING, VERT_PADDING))];

        let normal = DrawBoth::new(
            ctx,
            GeomBatch::from(vec![(unselected_bg_color, geom.clone())]),
            draw_text.clone(),
        );
        let hovered = DrawBoth::new(
            ctx,
            GeomBatch::from(vec![(selected_bg_color, geom.clone())]),
            draw_text,
        );

        Button::new(normal, hovered, hotkey, tooltip, geom)
    }

    pub fn text_no_bg(
        unselected_text: Text,
        selected_text: Text,
        hotkey: Option<MultiKey>,
        tooltip: &str,
        ctx: &EventCtx,
    ) -> Button {
        let dims = ctx.text_dims(&unselected_text);
        assert_eq!(dims, ctx.text_dims(&selected_text));
        let geom = Polygon::rectangle(dims.width, dims.height);

        let normal = DrawBoth::new(
            ctx,
            GeomBatch::new(),
            vec![(unselected_text, ScreenPt::new(0.0, 0.0))],
        );
        let hovered = DrawBoth::new(
            ctx,
            GeomBatch::new(),
            vec![(selected_text, ScreenPt::new(0.0, 0.0))],
        );

        Button::new(normal, hovered, hotkey, tooltip, geom)
    }

    pub fn at(mut self, pt: ScreenPt) -> Button {
        self.set_pos(pt);
        self
    }
}
