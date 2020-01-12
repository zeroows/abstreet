use crate::common::Colorer;
use crate::game::{Transition, WizardState};
use crate::render::{AgentColorScheme, MIN_ZOOM_FOR_DETAIL};
use crate::ui::UI;
use abstutil::clamp;
use ezgui::{
    hotkey, Button, Choice, Color, Composite, DrawBoth, EventCtx, Filler, GeomBatch, GfxCtx,
    HorizontalAlignment, Key, Line, ManagedWidget, Outcome, RewriteColor, ScreenDims, ScreenPt,
    Text, VerticalAlignment,
};
use geom::{Circle, Distance, Polygon, Pt2D, Ring};

// TODO Some of the math in here might assume map bound minimums start at (0, 0).
pub struct Minimap {
    dragging: bool,
    composite: Composite,
    acs: AgentColorScheme,
    zoomed: bool,

    // [0, 3], with 0 meaning the most unzoomed
    zoom_lvl: usize,
    base_zoom: f64,
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
}

impl Minimap {
    pub fn new(ctx: &mut EventCtx, ui: &UI) -> Minimap {
        // Initially pick a zoom to fit the entire map's width in the minimap. Arbitrary and
        // probably pretty weird.
        let bounds = ui.primary.map.get_bounds();
        let base_zoom = 0.15 * ctx.canvas.window_width / (bounds.max_x - bounds.min_x);
        Minimap {
            dragging: false,
            composite: make_minimap_panel(ctx, &ui.agent_cs, 0),
            acs: ui.agent_cs.clone(),
            zoomed: ctx.canvas.cam_zoom >= MIN_ZOOM_FOR_DETAIL,

            zoom_lvl: 0,
            base_zoom,
            zoom: base_zoom,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    fn set_zoom(&mut self, ctx: &mut EventCtx, zoom_lvl: usize) {
        let zoom_speed: f64 = 2.0;
        self.zoom_lvl = zoom_lvl;
        self.zoom = self.base_zoom * zoom_speed.powi(self.zoom_lvl as i32);
        self.composite = make_minimap_panel(ctx, &self.acs, self.zoom_lvl);
    }

    pub fn event(&mut self, ui: &mut UI, ctx: &mut EventCtx) -> Option<Transition> {
        // Happens when we changed the colorscheme in WizardState
        if ui.agent_cs != self.acs {
            self.acs = ui.agent_cs.clone();
            self.composite = make_minimap_panel(ctx, &self.acs, self.zoom_lvl);
        }
        let zoomed = ctx.canvas.cam_zoom >= MIN_ZOOM_FOR_DETAIL;
        if zoomed != self.zoomed {
            self.zoomed = zoomed;
            self.composite = make_minimap_panel(ctx, &self.acs, self.zoom_lvl);
        }

        let pan_speed = 100.0;
        match self.composite.event(ctx) {
            Some(Outcome::Clicked(x)) => match x {
                x if x == "pan up" => self.offset_y -= pan_speed * self.zoom,
                x if x == "pan down" => self.offset_y += pan_speed * self.zoom,
                x if x == "pan left" => self.offset_x -= pan_speed * self.zoom,
                x if x == "pan right" => self.offset_x += pan_speed * self.zoom,
                // TODO Make the center of the cursor still point to the same thing. Same math as
                // Canvas.
                x if x == "zoom in" => {
                    if self.zoom_lvl != 3 {
                        self.set_zoom(ctx, self.zoom_lvl + 1);
                    }
                }
                x if x == "zoom out" => {
                    if self.zoom_lvl != 0 {
                        self.set_zoom(ctx, self.zoom_lvl - 1);
                    }
                }
                x if x == "zoom to level 1" => {
                    self.set_zoom(ctx, 0);
                }
                x if x == "zoom to level 2" => {
                    self.set_zoom(ctx, 1);
                }
                x if x == "zoom to level 3" => {
                    self.set_zoom(ctx, 2);
                }
                x if x == "zoom to level 4" => {
                    self.set_zoom(ctx, 3);
                }
                x if x == "change agent colorscheme" => {
                    return Some(Transition::Push(WizardState::new(Box::new(
                        |wiz, ctx, ui| {
                            let (_, acs) =
                                wiz.wrap(ctx).choose("Which colorscheme for agents?", || {
                                    let mut choices = Vec::new();
                                    for (acs, name) in AgentColorScheme::all(&ui.cs) {
                                        if ui.agent_cs.acs != acs.acs {
                                            choices.push(Choice::new(name, acs));
                                        }
                                    }
                                    choices
                                })?;
                            ui.agent_cs = acs;
                            // TODO It'd be great to replace self here, but the lifetimes don't
                            // work out.
                            Some(Transition::Pop)
                        },
                    ))));
                }
                x => {
                    let key = x["show/hide ".len()..].to_string();
                    ui.agent_cs.toggle(key);
                    self.acs = ui.agent_cs.clone();
                    self.composite = make_minimap_panel(ctx, &self.acs, self.zoom_lvl);
                }
            },
            None => {}
        }

        if self.zoomed {
            let inner_rect = self.composite.filler_rect("minimap");

            let mut pt = ctx.canvas.get_cursor_in_screen_space();
            if self.dragging {
                if ctx.input.left_mouse_button_released() {
                    self.dragging = false;
                }
                // Don't drag out of inner_rect
                pt.x = clamp(pt.x, inner_rect.x1, inner_rect.x2);
                pt.y = clamp(pt.y, inner_rect.y1, inner_rect.y2);
            } else if inner_rect.contains(pt) && ctx.input.left_mouse_button_pressed() {
                self.dragging = true;
            } else {
                return None;
            }

            let percent_x = (pt.x - inner_rect.x1) / inner_rect.width();
            let percent_y = (pt.y - inner_rect.y1) / inner_rect.height();

            let map_pt = Pt2D::new(
                (self.offset_x + percent_x * inner_rect.width()) / self.zoom,
                (self.offset_y + percent_y * inner_rect.height()) / self.zoom,
            );
            ctx.canvas.center_on_map_pt(map_pt);
        }

        None
    }

    pub fn draw(&self, g: &mut GfxCtx, ui: &UI, colorer: Option<&Colorer>) {
        self.composite.draw(g);
        if !self.zoomed {
            return;
        }

        let inner_rect = self.composite.filler_rect("minimap");

        let mut map_bounds = ui.primary.map.get_bounds().clone();
        // Adjust bounds to account for the current pan and zoom
        map_bounds.min_x = (map_bounds.min_x + self.offset_x) / self.zoom;
        map_bounds.min_y = (map_bounds.min_y + self.offset_y) / self.zoom;
        map_bounds.max_x = map_bounds.min_x + inner_rect.width() / self.zoom;
        map_bounds.max_y = map_bounds.min_y + inner_rect.height() / self.zoom;

        g.fork(
            Pt2D::new(map_bounds.min_x, map_bounds.min_y),
            ScreenPt::new(inner_rect.x1, inner_rect.y1),
            self.zoom,
        );
        g.enable_clipping(inner_rect);
        g.redraw(&ui.primary.draw_map.boundary_polygon);
        g.redraw(&ui.primary.draw_map.draw_all_areas);
        g.redraw(&ui.primary.draw_map.draw_all_thick_roads);
        g.redraw(&ui.primary.draw_map.draw_all_unzoomed_intersections);
        g.redraw(&ui.primary.draw_map.draw_all_buildings);
        if let Some(ref c) = colorer {
            g.redraw(&c.unzoomed);
        }

        let mut cache = ui.primary.draw_map.agents.borrow_mut();
        cache.draw_unzoomed_agents(
            &ui.primary.sim,
            &ui.primary.map,
            &ui.agent_cs,
            g,
            self.zoom,
            Distance::meters(5.0),
        );

        // The cursor
        let (x1, y1) = {
            let pt = g.canvas.screen_to_map(ScreenPt::new(0.0, 0.0));
            (
                clamp(pt.x(), map_bounds.min_x, map_bounds.max_x),
                clamp(pt.y(), map_bounds.min_y, map_bounds.max_y),
            )
        };
        let (x2, y2) = {
            let pt = g
                .canvas
                .screen_to_map(ScreenPt::new(g.canvas.window_width, g.canvas.window_height));
            (
                clamp(pt.x(), map_bounds.min_x, map_bounds.max_x),
                clamp(pt.y(), map_bounds.min_y, map_bounds.max_y),
            )
        };
        if x1 != x2 && y1 != y2 {
            g.draw_polygon(
                Color::BLACK,
                &Ring::new(vec![
                    Pt2D::new(x1, y1),
                    Pt2D::new(x2, y1),
                    Pt2D::new(x2, y2),
                    Pt2D::new(x1, y2),
                    Pt2D::new(x1, y1),
                ])
                .make_polygons(Distance::meters(20.0)),
            );
        } else {
            // TODO Happens when we're quite out-of-bounds. Maybe stop allowing this at all?
            println!("Warning: Minimap cursor is just a point right now");
        }
        g.disable_clipping();
        g.unfork();
    }
}

fn make_minimap_panel(ctx: &mut EventCtx, acs: &AgentColorScheme, zoom_lvl: usize) -> Composite {
    if ctx.canvas.cam_zoom < MIN_ZOOM_FOR_DETAIL {
        return Composite::new(make_viz_panel(ctx, acs))
            .aligned(
                HorizontalAlignment::Right,
                VerticalAlignment::BottomAboveOSD,
            )
            .build(ctx);
    }

    let square_len = 0.15 * ctx.canvas.window_width;
    let mut zoom_col = vec![ManagedWidget::btn(Button::rectangle_svg(
        "assets/speed/speed_up.svg",
        "zoom in",
        None,
        RewriteColor::ChangeAll(Color::ORANGE),
        ctx,
    ))];
    for i in (0..=3).rev() {
        let color = if zoom_lvl < i {
            Color::grey(0.2)
        } else {
            Color::WHITE
        };
        let rect = Polygon::rectangle(20.0, 8.0);
        zoom_col.push(ManagedWidget::btn(Button::new(
            DrawBoth::new(
                ctx,
                GeomBatch::from(vec![(color, rect.clone())]),
                Vec::new(),
            ),
            DrawBoth::new(
                ctx,
                GeomBatch::from(vec![(Color::ORANGE, rect.clone())]),
                Vec::new(),
            ),
            None,
            &format!("zoom to level {}", i + 1),
            rect,
        )));
    }
    zoom_col.push(ManagedWidget::btn(Button::rectangle_svg(
        "assets/speed/slow_down.svg",
        "zoom out",
        None,
        RewriteColor::ChangeAll(Color::ORANGE),
        ctx,
    )));

    Composite::new(
        ManagedWidget::row(vec![
            make_viz_panel(ctx, acs),
            ManagedWidget::col(zoom_col).margin(5).centered(),
            ManagedWidget::col(vec![
                ManagedWidget::row(vec![crate::managed::Composite::svg_button(
                    ctx,
                    "assets/minimap/up.svg",
                    "pan up",
                    None,
                )])
                .margin(5)
                .centered(),
                ManagedWidget::row(vec![
                    ManagedWidget::col(vec![crate::managed::Composite::svg_button(
                        ctx,
                        "assets/minimap/left.svg",
                        "pan left",
                        None,
                    )])
                    .margin(5)
                    .centered(),
                    ManagedWidget::filler("minimap"),
                    ManagedWidget::col(vec![crate::managed::Composite::svg_button(
                        ctx,
                        "assets/minimap/right.svg",
                        "pan right",
                        None,
                    )])
                    .margin(5)
                    .centered(),
                ]),
                ManagedWidget::row(vec![crate::managed::Composite::svg_button(
                    ctx,
                    "assets/minimap/down.svg",
                    "pan down",
                    None,
                )])
                .margin(5)
                .centered(),
            ]),
        ])
        .bg(Color::grey(0.5)),
    )
    .aligned(
        HorizontalAlignment::Right,
        VerticalAlignment::BottomAboveOSD,
    )
    .filler(
        "minimap",
        Filler::new(ScreenDims::new(square_len, square_len)),
    )
    .build(ctx)
}

fn make_viz_panel(ctx: &mut EventCtx, acs: &AgentColorScheme) -> ManagedWidget {
    let radius = 15.0;
    let mut col = vec![
        // TODO Too wide most of the time...
        ManagedWidget::draw_text(ctx, Text::prompt(&acs.title)),
        ManagedWidget::btn(Button::text(
            Text::from(Line("change")),
            Color::grey(0.6),
            Color::ORANGE,
            hotkey(Key::Semicolon),
            "change agent colorscheme",
            ctx,
        )),
    ];
    for (label, color, enabled) in &acs.rows {
        col.push(
            ManagedWidget::row(vec![
                ManagedWidget::btn(Button::rectangle_svg(
                    "assets/tools/visibility.svg",
                    &format!("show/hide {}", label),
                    None,
                    RewriteColor::Change(Color::WHITE, Color::ORANGE),
                    ctx,
                )),
                ManagedWidget::draw_batch(
                    ctx,
                    GeomBatch::from(vec![(
                        if *enabled {
                            color.alpha(1.0)
                        } else {
                            color.alpha(0.5)
                        },
                        Circle::new(Pt2D::new(radius, radius), Distance::meters(radius))
                            .to_polygon(),
                    )]),
                ),
                ManagedWidget::draw_text(
                    ctx,
                    Text::from(if *enabled {
                        Line(label)
                    } else {
                        Line(label).fg(Color::WHITE.alpha(0.5))
                    }),
                ),
            ])
            .centered_cross(),
        );
    }
    ManagedWidget::col(col).bg(Color::grey(0.4))
}
