mod bus_explorer;
mod dashboards;
mod gameplay;
mod overlays;
mod speed;

pub use self::overlays::Overlays;
use crate::common::{tool_panel, AgentTools, CommonState, Minimap};
use crate::debug::DebugMode;
use crate::edit::{apply_map_edits, save_edits, EditMode, TrafficSignalEditor};
use crate::game::{State, Transition, WizardState};
use crate::helpers::ID;
use crate::managed::Outcome;
use crate::pregame::main_menu;
use crate::ui::{ShowEverything, UI};
use abstutil::Timer;
use ezgui::{
    hotkey, lctrl, Choice, Color, Composite, EventCtx, EventLoopMode, GfxCtx, HorizontalAlignment,
    Key, Line, ManagedWidget, Text, VerticalAlignment,
};
pub use gameplay::spawner::spawn_agents_around;
pub use gameplay::GameplayMode;
use geom::Time;
use map_model::MapEdits;
use sim::TripMode;
pub use speed::{SpeedControls, TimePanel};

pub struct SandboxMode {
    speed: SpeedControls,
    time_panel: TimePanel,
    agent_meter: AgentMeter,
    agent_tools: AgentTools,
    overlay: Overlays,
    gameplay: gameplay::GameplayRunner,
    common: CommonState,
    tool_panel: crate::managed::Composite,
    minimap: Option<Minimap>,
}

impl SandboxMode {
    pub fn new(ctx: &mut EventCtx, ui: &mut UI, mode: GameplayMode) -> SandboxMode {
        let tool_panel = tool_panel(
            ctx,
            vec![crate::managed::Composite::svg_button(
                ctx,
                "assets/tools/info.svg",
                "info",
                hotkey(Key::Q),
            )],
        );

        SandboxMode {
            speed: SpeedControls::new(ctx),
            time_panel: TimePanel::new(ctx, ui),
            agent_meter: AgentMeter::new(ctx, ui),
            agent_tools: AgentTools::new(),
            overlay: Overlays::Inactive,
            common: CommonState::new(),
            tool_panel,
            minimap: if mode.has_minimap() {
                Some(Minimap::new(ctx, ui))
            } else {
                None
            },
            gameplay: gameplay::GameplayRunner::initialize(mode, ui, ctx),
        }
    }
}

impl State for SandboxMode {
    fn event(&mut self, ctx: &mut EventCtx, ui: &mut UI) -> Transition {
        self.agent_meter.event(ctx, ui);
        if let Some(t) = self.gameplay.event(ctx, ui, &mut self.overlay) {
            return t;
        }

        ctx.canvas_movement();
        if ctx.redo_mouseover() {
            ui.recalculate_current_selection(ctx);
        }
        if let Some(ref mut m) = self.minimap {
            if let Some(t) = m.event(ui, ctx) {
                return t;
            }
        }

        if let Some(t) = self.agent_tools.event(ctx, ui, &mut self.gameplay.menu) {
            return t;
        }
        if let Some(explorer) = bus_explorer::BusRouteExplorer::new(ctx, ui) {
            return Transition::PushWithMode(explorer, EventLoopMode::Animation);
        }

        if ui.opts.dev && ctx.input.new_was_pressed(lctrl(Key::D).unwrap()) {
            return Transition::Push(Box::new(DebugMode::new(ctx)));
        }

        if let Some(ID::Building(b)) = ui.primary.current_selection {
            let cars = ui
                .primary
                .sim
                .get_offstreet_parked_cars(b)
                .into_iter()
                .map(|p| p.vehicle.id)
                .collect::<Vec<_>>();
            if !cars.is_empty()
                && ui.per_obj.action(
                    ctx,
                    Key::P,
                    format!("examine {} cars parked here", cars.len()),
                )
            {
                return Transition::Push(WizardState::new(Box::new(move |wiz, ctx, _| {
                    let _id = wiz.wrap(ctx).choose("Examine which car?", || {
                        cars.iter()
                            .map(|c| Choice::new(c.to_string(), *c))
                            .collect()
                    })?;
                    Some(Transition::Pop)
                })));
            }
        }
        if let Some(ID::Intersection(i)) = ui.primary.current_selection {
            if ui.primary.map.get_i(i).is_traffic_signal()
                && ui.per_obj.action(ctx, Key::C, "show current demand")
            {
                self.overlay = Overlays::intersection_demand(i, ctx, ui);
            }
            if ui.primary.map.get_i(i).is_traffic_signal()
                && ui.per_obj.action(ctx, Key::E, "edit traffic signal")
            {
                let edit = EditMode::new(ctx, ui, self.gameplay.mode.clone());
                let sim_copy = edit.suspended_sim.clone();
                return Transition::PushTwice(
                    Box::new(edit),
                    Box::new(TrafficSignalEditor::new(i, ctx, ui, sim_copy)),
                );
            }
        }

        self.time_panel.event(ctx, ui);

        match self.speed.event(ctx, ui) {
            Some(Outcome::Transition(t)) => {
                return t;
            }
            Some(Outcome::Clicked(x)) => match x {
                x if x == "reset to midnight" => {
                    ui.primary.clear_sim();
                    return Transition::Replace(Box::new(SandboxMode::new(
                        ctx,
                        ui,
                        self.gameplay.mode.clone(),
                    )));
                }
                _ => unreachable!(),
            },
            None => {}
        }

        if let Some(t) = self.common.event(ctx, ui) {
            return t;
        }
        if let Some(t) = self.overlay.event(ctx, ui) {
            return t;
        }
        match self.tool_panel.event(ctx, ui) {
            Some(Outcome::Transition(t)) => {
                return t;
            }
            Some(Outcome::Clicked(x)) => match x.as_ref() {
                "back" => {
                    return Transition::Push(WizardState::new(Box::new(move |wiz, ctx, ui| {
                        let mut wizard = wiz.wrap(ctx);
                        let dirty = ui.primary.map.get_edits().dirty;
                        let (resp, _) = wizard.choose(
                            "Sure you want to abandon the current challenge?",
                            || {
                                let mut choices = Vec::new();
                                choices.push(Choice::new("keep playing", ()));
                                if dirty {
                                    choices.push(Choice::new("save edits and quit", ()));
                                }
                                choices.push(Choice::new("quit challenge", ()).key(Key::Q));
                                choices
                            },
                        )?;
                        let map_name = ui.primary.map.get_name().to_string();
                        match resp.as_str() {
                            "save edits and quit" => {
                                save_edits(&mut wizard, ui)?;

                                // Always reset edits if we just saved edits.
                                apply_map_edits(
                                    &mut ui.primary,
                                    &ui.cs,
                                    ctx,
                                    MapEdits::new(map_name),
                                );
                                ui.primary.map.mark_edits_fresh();
                                ui.primary.map.recalculate_pathfinding_after_edits(
                                    &mut Timer::new("reset edits"),
                                );
                                ui.primary.clear_sim();
                                ui.set_prebaked(None);
                                ctx.canvas.save_camera_state(ui.primary.map.get_name());
                                Some(Transition::Clear(main_menu(ctx, ui)))
                            }
                            "quit challenge" => {
                                if !ui.primary.map.get_edits().is_empty() {
                                    apply_map_edits(
                                        &mut ui.primary,
                                        &ui.cs,
                                        ctx,
                                        MapEdits::new(map_name),
                                    );
                                    ui.primary.map.mark_edits_fresh();
                                    ui.primary.map.recalculate_pathfinding_after_edits(
                                        &mut Timer::new("reset edits"),
                                    );
                                }
                                ui.primary.clear_sim();
                                ui.set_prebaked(None);
                                ctx.canvas.save_camera_state(ui.primary.map.get_name());
                                Some(Transition::Clear(main_menu(ctx, ui)))
                            }
                            "keep playing" => Some(Transition::Pop),
                            _ => unreachable!(),
                        }
                    })));
                }
                "info" => {
                    return Transition::Push(dashboards::make(
                        ctx,
                        ui,
                        dashboards::Tab::FinishedTripsSummary,
                    ));
                }
                _ => unreachable!(),
            },
            None => {}
        }

        if self.speed.is_paused() {
            Transition::Keep
        } else {
            Transition::KeepWithMode(EventLoopMode::Animation)
        }
    }

    fn draw_default_ui(&self) -> bool {
        false
    }

    fn draw(&self, g: &mut GfxCtx, ui: &UI) {
        ui.draw(
            g,
            self.common.draw_options(ui),
            &ui.primary.sim,
            &ShowEverything::new(),
        );
        self.overlay.draw(g);
        self.agent_tools.draw(g);
        self.common.draw(g, ui);
        self.tool_panel.draw(g);
        self.speed.draw(g);
        self.time_panel.draw(g);
        self.gameplay.draw(g, ui);
        self.agent_meter.draw(g);
        if let Some(ref m) = self.minimap {
            m.draw(g, ui, self.overlay.maybe_colorer());
        }
    }

    fn on_suspend(&mut self, ctx: &mut EventCtx, _: &mut UI) {
        self.speed.pause(ctx);
    }
}

struct AgentMeter {
    time: Time,
    composite: Composite,
}

impl AgentMeter {
    pub fn new(ctx: &mut EventCtx, ui: &UI) -> AgentMeter {
        let (active, unfinished, by_mode) = ui.primary.sim.num_trips();

        let composite = Composite::new(
            ManagedWidget::col(vec![
                {
                    let mut txt = Text::new();
                    txt.add(Line(format!("Active trips: {}", active)));
                    txt.add(Line(format!("Unfinished trips: {}", unfinished)));
                    ManagedWidget::draw_text(ctx, txt)
                },
                ManagedWidget::row(vec![
                    ManagedWidget::draw_svg(ctx, "assets/meters/pedestrian.svg"),
                    ManagedWidget::draw_text(ctx, Text::from(Line(&by_mode[&TripMode::Walk]))),
                    ManagedWidget::draw_svg(ctx, "assets/meters/bike.svg"),
                    ManagedWidget::draw_text(ctx, Text::from(Line(&by_mode[&TripMode::Bike]))),
                    ManagedWidget::draw_svg(ctx, "assets/meters/car.svg"),
                    ManagedWidget::draw_text(ctx, Text::from(Line(&by_mode[&TripMode::Drive]))),
                    ManagedWidget::draw_svg(ctx, "assets/meters/bus.svg"),
                    ManagedWidget::draw_text(ctx, Text::from(Line(&by_mode[&TripMode::Transit]))),
                ])
                .centered(),
            ])
            .bg(Color::grey(0.4))
            .padding(20),
        )
        .aligned(HorizontalAlignment::Right, VerticalAlignment::Top)
        .build(ctx);

        AgentMeter {
            time: ui.primary.sim.time(),
            composite,
        }
    }

    pub fn event(&mut self, ctx: &mut EventCtx, ui: &mut UI) {
        if self.time != ui.primary.sim.time() {
            *self = AgentMeter::new(ctx, ui);
        }
        self.composite.event(ctx);
    }

    pub fn draw(&self, g: &mut GfxCtx) {
        self.composite.draw(g);
    }
}
