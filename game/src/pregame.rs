use crate::abtest::setup::PickABTest;
use crate::challenges::challenges_picker;
use crate::game::{State, Transition};
use crate::managed::{Composite, ManagedGUIState, Outcome};
use crate::mission::MissionEditMode;
use crate::sandbox::{GameplayMode, SandboxMode};
use crate::tutorial::TutorialMode;
use crate::ui::UI;
use ezgui::{
    hotkey, Button, Color, EventCtx, EventLoopMode, GfxCtx, JustDraw, Key, Line, ManagedWidget,
    Text,
};
use geom::{Duration, Line, Pt2D, Speed};
use map_model::Map;
use rand::Rng;
use rand_xorshift::XorShiftRng;
use std::time::Instant;

pub struct TitleScreen {
    composite: Composite,
    screensaver: Screensaver,
    rng: XorShiftRng,
}

impl TitleScreen {
    pub fn new(ctx: &mut EventCtx, ui: &UI) -> TitleScreen {
        let mut rng = ui.primary.current_flags.sim_flags.make_rng();
        TitleScreen {
            composite: Composite::new(
                ezgui::Composite::new(ManagedWidget::col(vec![
                    ManagedWidget::just_draw(JustDraw::image("assets/pregame/logo.png", ctx))
                        .bg(Color::GREEN.alpha(0.2)),
                    // TODO that nicer font
                    // TODO Any key
                    ManagedWidget::row(vec![ManagedWidget::btn(Button::text(
                        Text::from(Line("PLAY")),
                        Color::BLUE,
                        Color::ORANGE,
                        hotkey(Key::Space),
                        "start game",
                        ctx,
                    ))])
                    .centered(),
                ]))
                .build(ctx),
            )
            .cb(
                "start game",
                Box::new(|ctx, ui| Some(Transition::Replace(main_menu(ctx, ui)))),
            ),
            screensaver: Screensaver::start_bounce(&mut rng, ctx, &ui.primary.map),
            rng,
        }
    }
}

impl State for TitleScreen {
    fn event(&mut self, ctx: &mut EventCtx, ui: &mut UI) -> Transition {
        match self.composite.event(ctx, ui) {
            Some(Outcome::Transition(t)) => t,
            Some(Outcome::Clicked(_)) => unreachable!(),
            None => {
                self.screensaver.update(&mut self.rng, ctx, &ui.primary.map);
                Transition::KeepWithMode(EventLoopMode::Animation)
            }
        }
    }

    fn draw(&self, g: &mut GfxCtx, _: &UI) {
        self.composite.draw(g);
    }
}

pub fn main_menu(ctx: &mut EventCtx, ui: &UI) -> Box<dyn State> {
    let mut col = Vec::new();

    col.push(ManagedWidget::row(vec![
        Composite::svg_button(ctx, "assets/pregame/quit.svg", "quit", hotkey(Key::Escape)),
        ManagedWidget::draw_text(ctx, Text::from(Line("A/B STREET").size(50))),
    ]));

    col.push(ManagedWidget::draw_text(
        ctx,
        Text::from(Line("Created by Dustin Carlino")),
    ));

    col.push(
        ManagedWidget::row(vec![
            Composite::svg_button(
                ctx,
                "assets/pregame/tutorial.svg",
                "Tutorial",
                hotkey(Key::T),
            ),
            Composite::svg_button(
                ctx,
                "assets/pregame/sandbox.svg",
                "Sandbox mode",
                hotkey(Key::S),
            ),
            Composite::img_button(
                ctx,
                "assets/pregame/challenges.png",
                hotkey(Key::C),
                "Challenges",
            ),
        ])
        .centered(),
    );
    if ui.opts.dev {
        col.push(
            ManagedWidget::row(vec![
                Composite::text_button(ctx, "INTERNAL DEV TOOLS", hotkey(Key::M)),
                Composite::text_button(ctx, "INTERNAL A/B TEST MODE", hotkey(Key::A)),
            ])
            .centered(),
        );
    }
    col.push(Composite::text_button(ctx, "About A/B Street", None));

    let mut c = Composite::new(
        ezgui::Composite::new(ManagedWidget::col(col).centered())
            .fullscreen()
            .build(ctx),
    )
    .cb(
        "quit",
        Box::new(|_, _| {
            // TODO before_quit?
            std::process::exit(0);
        }),
    )
    .cb(
        "Tutorial",
        Box::new(|ctx, _| Some(Transition::Push(Box::new(TutorialMode::new(ctx))))),
    )
    .cb(
        "Sandbox mode",
        Box::new(|ctx, ui| {
            Some(Transition::PushWithMode(
                Box::new(SandboxMode::new(
                    ctx,
                    ui,
                    GameplayMode::PlayScenario("random scenario with some agents".to_string()),
                )),
                EventLoopMode::Animation,
            ))
        }),
    )
    .cb(
        "Challenges",
        Box::new(|ctx, _| Some(Transition::Push(challenges_picker(ctx)))),
    )
    .cb(
        "About A/B Street",
        Box::new(|ctx, _| Some(Transition::Push(about(ctx)))),
    );
    if ui.opts.dev {
        c = c
            .cb(
                "INTERNAL DEV TOOLS",
                Box::new(|ctx, _| Some(Transition::Push(Box::new(MissionEditMode::new(ctx))))),
            )
            .cb(
                "INTERNAL A/B TEST MODE",
                Box::new(|_, _| Some(Transition::Push(PickABTest::new()))),
            );
    }
    ManagedGUIState::fullscreen(c)
}

fn about(ctx: &mut EventCtx) -> Box<dyn State> {
    let mut col = Vec::new();

    col.push(Composite::svg_button(
        ctx,
        "assets/pregame/back.svg",
        "back",
        hotkey(Key::Escape),
    ));

    let mut txt = Text::new();
    txt.add(Line("A/B STREET").size(50));
    txt.add(Line("Created by Dustin Carlino, UX by Yuwen Li"));
    txt.add(Line(""));
    txt.add(Line("Contact: dabreegster@gmail.com"));
    txt.add(Line(
        "Project: http://github.com/dabreegster/abstreet (aliased by abstreet.org)",
    ));
    txt.add(Line("Map data from OpenStreetMap and King County GIS"));
    // TODO Add more here
    txt.add(Line(
        "See full credits at https://github.com/dabreegster/abstreet#credits",
    ));
    txt.add(Line(""));
    // TODO Word wrapping please?
    txt.add(Line(
        "Disclaimer: This game is based on imperfect data, heuristics ",
    ));
    txt.add(Line(
        "concocted under the influence of cold brew, a simplified traffic ",
    ));
    txt.add(Line(
        "simulation model, and a deeply flawed understanding of how much ",
    ));
    txt.add(Line(
        "articulated buses can bend around tight corners. Use this as a ",
    ));
    txt.add(Line(
        "conversation starter with your city government, not a final ",
    ));
    txt.add(Line(
        "decision maker. Any resemblance of in-game characters to real ",
    ));
    txt.add(Line(
        "people is probably coincidental, except for PedestrianID(42). ",
    ));
    txt.add(Line("Have the appropriate amount of fun."));
    col.push(ManagedWidget::draw_text(ctx, txt));

    ManagedGUIState::fullscreen(
        Composite::new(ezgui::Composite::new(ManagedWidget::col(col)).build(ctx))
            .cb("back", Box::new(|_, _| Some(Transition::Pop))),
    )
}

const SPEED: Speed = Speed::const_meters_per_second(20.0);

struct Screensaver {
    line: Line,
    started: Instant,
}

impl Screensaver {
    fn start_bounce(rng: &mut XorShiftRng, ctx: &mut EventCtx, map: &Map) -> Screensaver {
        let at = ctx.canvas.center_to_map_pt();
        let bounds = map.get_bounds();
        // TODO Ideally bounce off the edge of the map
        let goto = Pt2D::new(
            rng.gen_range(0.0, bounds.max_x),
            rng.gen_range(0.0, bounds.max_y),
        );

        ctx.canvas.cam_zoom = 10.0;
        ctx.canvas.center_on_map_pt(at);

        Screensaver {
            line: Line::new(at, goto),
            started: Instant::now(),
        }
    }

    fn update(&mut self, rng: &mut XorShiftRng, ctx: &mut EventCtx, map: &Map) {
        if ctx.input.nonblocking_is_update_event() {
            ctx.input.use_update_event();
            let dist_along = Duration::realtime_elapsed(self.started) * SPEED;
            if dist_along < self.line.length() {
                ctx.canvas
                    .center_on_map_pt(self.line.dist_along(dist_along));
            } else {
                *self = Screensaver::start_bounce(rng, ctx, map)
            }
        }
    }
}
