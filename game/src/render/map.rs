use crate::helpers::{rotating_color, ColorScheme, ID};
use crate::render::area::DrawArea;
use crate::render::building::DrawBuilding;
use crate::render::bus_stop::DrawBusStop;
use crate::render::extra_shape::{DrawExtraShape, ExtraShapeID};
use crate::render::intersection::DrawIntersection;
use crate::render::lane::DrawLane;
use crate::render::road::DrawRoad;
use crate::render::Renderable;
use crate::ui::Flags;
use aabb_quadtree::QuadTree;
use abstutil::{Cloneable, Timer};
use ezgui::{Color, Drawable, EventCtx, GeomBatch, GfxCtx};
use geom::{Bounds, Circle, Distance, Duration, FindClosest, Time};
use map_model::{
    AreaID, BuildingID, BusStopID, DirectedRoadID, Intersection, IntersectionID, LaneID, Map, Road,
    RoadID, Traversable, LANE_THICKNESS,
};
use sim::{GetDrawAgents, UnzoomedAgent, VehicleType};
use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;

pub struct DrawMap {
    pub roads: Vec<DrawRoad>,
    pub lanes: Vec<DrawLane>,
    pub intersections: Vec<DrawIntersection>,
    pub buildings: Vec<DrawBuilding>,
    pub extra_shapes: Vec<DrawExtraShape>,
    pub bus_stops: HashMap<BusStopID, DrawBusStop>,
    pub areas: Vec<DrawArea>,

    // TODO Move?
    pub agents: RefCell<AgentCache>,

    pub boundary_polygon: Drawable,
    pub draw_all_thick_roads: Drawable,
    pub draw_all_unzoomed_intersections: Drawable,
    pub draw_all_buildings: Drawable,
    pub draw_all_areas: Drawable,

    quadtree: QuadTree<ID>,
}

impl DrawMap {
    pub fn new(
        map: &Map,
        flags: &Flags,
        cs: &ColorScheme,
        ctx: &EventCtx,
        timer: &mut Timer,
    ) -> DrawMap {
        let mut roads: Vec<DrawRoad> = Vec::new();
        timer.start_iter("make DrawRoads", map.all_roads().len());
        for r in map.all_roads() {
            timer.next();
            roads.push(DrawRoad::new(r, map, cs, ctx.prerender));
        }

        timer.start("generate thick roads");
        let mut road_refs: Vec<&Road> = map.all_roads().iter().collect();
        road_refs.sort_by_key(|r| r.get_zorder());
        let mut all_roads = GeomBatch::new();
        for r in road_refs {
            all_roads.push(
                osm_rank_to_color(cs, r.get_rank()),
                r.get_thick_polygon().get(timer),
            );
            if false {
                all_roads.push(
                    cs.get_def("unzoomed outline", Color::BLACK),
                    roads[r.id.0].get_outline(map),
                );
            }
        }
        let draw_all_thick_roads = all_roads.upload(ctx);
        timer.stop("generate thick roads");

        let almost_lanes =
            timer.parallelize("prepare DrawLanes", map.all_lanes().iter().collect(), |l| {
                DrawLane::new(
                    l,
                    map,
                    flags.draw_lane_markings,
                    cs,
                    // TODO Really parallelize should give us something thread-safe that can at
                    // least take notes.
                    &mut Timer::throwaway(),
                )
            });
        timer.start_iter("finalize DrawLanes", almost_lanes.len());
        let mut lanes: Vec<DrawLane> = Vec::new();
        for almost in almost_lanes {
            timer.next();
            lanes.push(almost.finish(ctx.prerender));
        }

        let mut intersections: Vec<DrawIntersection> = Vec::new();
        timer.start_iter("make DrawIntersections", map.all_intersections().len());
        for i in map.all_intersections() {
            timer.next();
            intersections.push(DrawIntersection::new(i, map, cs, ctx.prerender, timer));
        }

        timer.start("generate unzoomed intersections");
        let mut intersection_refs: Vec<&Intersection> = map.all_intersections().iter().collect();
        intersection_refs.sort_by_key(|i| i.get_zorder(map));
        let mut all_intersections = GeomBatch::new();
        for i in intersection_refs {
            // TODO Would be neat to show closed intersections here, but then edits need to
            // regenerate this
            if i.is_stop_sign() {
                all_intersections.push(osm_rank_to_color(cs, i.get_rank(map)), i.polygon.clone());
                if false {
                    all_intersections.push(
                        cs.get("unzoomed outline"),
                        intersections[i.id.0].get_outline(map),
                    );
                }
            } else {
                all_intersections.push(
                    cs.get_def("unzoomed interesting intersection", Color::BLACK),
                    i.polygon.clone(),
                );
            }
        }
        let draw_all_unzoomed_intersections = all_intersections.upload(ctx);
        timer.stop("generate unzoomed intersections");

        let mut buildings: Vec<DrawBuilding> = Vec::new();
        let mut all_buildings = GeomBatch::new();
        timer.start_iter("make DrawBuildings", map.all_buildings().len());
        for b in map.all_buildings() {
            timer.next();
            buildings.push(DrawBuilding::new(b, cs, &mut all_buildings));
        }
        timer.start("upload all buildings");
        let draw_all_buildings = all_buildings.upload(ctx);
        timer.stop("upload all buildings");

        let mut extra_shapes: Vec<DrawExtraShape> = Vec::new();
        if let Some(ref path) = flags.kml {
            let raw_shapes = if path.ends_with(".kml") {
                kml::load(&path, &map.get_gps_bounds(), timer)
                    .expect("Couldn't load extra KML shapes")
                    .shapes
            } else {
                let shapes: kml::ExtraShapes = abstutil::read_binary(path.clone(), timer);
                shapes.shapes
            };

            let mut closest: FindClosest<DirectedRoadID> = FindClosest::new(&map.get_bounds());
            for r in map.all_roads().iter() {
                closest.add(
                    r.id.forwards(),
                    r.center_pts.shift_right(LANE_THICKNESS).get(timer).points(),
                );
                closest.add(
                    r.id.backwards(),
                    r.center_pts.shift_left(LANE_THICKNESS).get(timer).points(),
                );
            }

            let gps_bounds = map.get_gps_bounds();
            for s in raw_shapes.into_iter() {
                if let Some(es) = DrawExtraShape::new(
                    ExtraShapeID(extra_shapes.len()),
                    s,
                    gps_bounds,
                    &closest,
                    ctx.prerender,
                    cs,
                ) {
                    extra_shapes.push(es);
                }
            }
        }

        timer.start_iter("make DrawBusStop", map.all_bus_stops().len());
        let mut bus_stops: HashMap<BusStopID, DrawBusStop> = HashMap::new();
        for s in map.all_bus_stops().values() {
            timer.next();
            bus_stops.insert(s.id, DrawBusStop::new(s, map, cs, ctx.prerender));
        }

        let mut areas: Vec<DrawArea> = Vec::new();
        let mut all_areas = GeomBatch::new();
        timer.start_iter("make DrawAreas", map.all_areas().len());
        for a in map.all_areas() {
            timer.next();
            areas.push(DrawArea::new(a, cs, &mut all_areas));
        }
        timer.start("upload all areas");
        let draw_all_areas = all_areas.upload(ctx);
        timer.stop("upload all areas");

        let boundary_polygon = ctx.prerender.upload_borrowed(vec![(
            cs.get_def("map background", Color::grey(0.87)),
            map.get_boundary_polygon(),
        )]);

        timer.start("create quadtree");
        let mut quadtree = QuadTree::default(map.get_bounds().as_bbox());
        // TODO use iter chain if everything was boxed as a renderable...
        for obj in &roads {
            quadtree.insert_with_box(obj.get_id(), obj.get_outline(map).get_bounds().as_bbox());
        }
        for obj in &lanes {
            quadtree.insert_with_box(obj.get_id(), obj.get_outline(map).get_bounds().as_bbox());
        }
        for obj in &intersections {
            quadtree.insert_with_box(obj.get_id(), obj.get_outline(map).get_bounds().as_bbox());
        }
        for obj in &buildings {
            quadtree.insert_with_box(obj.get_id(), obj.get_outline(map).get_bounds().as_bbox());
        }
        for obj in &extra_shapes {
            quadtree.insert_with_box(obj.get_id(), obj.get_outline(map).get_bounds().as_bbox());
        }
        // Don't put BusStops in the quadtree
        for obj in &areas {
            quadtree.insert_with_box(obj.get_id(), obj.get_outline(map).get_bounds().as_bbox());
        }
        timer.stop("create quadtree");

        timer.note(format!(
            "static DrawMap consumes {} MB on the GPU",
            abstutil::prettyprint_usize(ctx.prerender.get_total_bytes_uploaded() / 1024 / 1024)
        ));

        DrawMap {
            roads,
            lanes,
            intersections,
            buildings,
            extra_shapes,
            bus_stops,
            areas,
            boundary_polygon,
            draw_all_thick_roads,
            draw_all_unzoomed_intersections,
            draw_all_buildings,
            draw_all_areas,

            agents: RefCell::new(AgentCache {
                time: None,
                agents_per_on: HashMap::new(),
                unzoomed: None,
            }),

            quadtree,
        }
    }

    // The alt to these is implementing std::ops::Index, but that's way more verbose!
    pub fn get_r(&self, id: RoadID) -> &DrawRoad {
        &self.roads[id.0]
    }

    pub fn get_l(&self, id: LaneID) -> &DrawLane {
        &self.lanes[id.0]
    }

    pub fn get_i(&self, id: IntersectionID) -> &DrawIntersection {
        &self.intersections[id.0]
    }

    pub fn get_b(&self, id: BuildingID) -> &DrawBuilding {
        &self.buildings[id.0]
    }

    pub fn get_es(&self, id: ExtraShapeID) -> &DrawExtraShape {
        &self.extra_shapes[id.0]
    }

    pub fn get_bs(&self, id: BusStopID) -> &DrawBusStop {
        &self.bus_stops[&id]
    }

    pub fn get_a(&self, id: AreaID) -> &DrawArea {
        &self.areas[id.0]
    }

    // Unsorted, unexpanded, raw result.
    pub fn get_matching_objects(&self, bounds: Bounds) -> Vec<ID> {
        let mut results: Vec<ID> = Vec::new();
        for &(id, _, _) in &self.quadtree.query(bounds.as_bbox()) {
            results.push(id.clone());
        }
        results
    }
}

pub struct AgentCache {
    // This time applies to agents_per_on. unzoomed has its own possibly separate Time!
    time: Option<Time>,
    agents_per_on: HashMap<Traversable, Vec<Box<dyn Renderable>>>,
    // cam_zoom and agent radius also matters
    unzoomed: Option<(Time, f64, Distance, AgentColorScheme, Drawable)>,
}

impl AgentCache {
    pub fn has(&self, now: Time, on: Traversable) -> bool {
        if Some(now) != self.time {
            return false;
        }
        self.agents_per_on.contains_key(&on)
    }

    // Must call has() first.
    pub fn get(&self, on: Traversable) -> Vec<&dyn Renderable> {
        self.agents_per_on[&on]
            .iter()
            .map(|obj| obj.borrow())
            .collect()
    }

    pub fn put(&mut self, now: Time, on: Traversable, agents: Vec<Box<dyn Renderable>>) {
        if Some(now) != self.time {
            self.agents_per_on.clear();
            self.time = Some(now);
        }

        assert!(!self.agents_per_on.contains_key(&on));
        self.agents_per_on.insert(on, agents);
    }

    // TODO GetDrawAgents indirection added for time traveling, but that's been removed. Maybe
    // simplify this.
    pub fn draw_unzoomed_agents(
        &mut self,
        source: &dyn GetDrawAgents,
        map: &Map,
        acs: &AgentColorScheme,
        g: &mut GfxCtx,
        cam_zoom: f64,
        radius: Distance,
    ) {
        let now = source.time();
        if let Some((time, z, r, ref orig_acs, ref draw)) = self.unzoomed {
            if cam_zoom == z && now == time && radius == r && acs == orig_acs {
                g.redraw(draw);
                return;
            }
        }

        // TODO The perf is a little slow compared to when we just returned a bunch of Pt2Ds
        // without the extra data. Try plumbing a callback that directly populates batch.
        let mut batch = GeomBatch::new();
        for agent in source.get_unzoomed_agents(map) {
            if let Some(color) = acs.color(&agent) {
                batch.push(
                    color,
                    Circle::new(agent.pos, radius / cam_zoom).to_polygon(),
                );
            }
        }

        let draw = g.upload(batch);
        g.redraw(&draw);
        self.unzoomed = Some((now, cam_zoom, radius, acs.clone(), draw));
    }
}

fn osm_rank_to_color(cs: &ColorScheme, rank: usize) -> Color {
    if rank >= 16 {
        cs.get_def("unzoomed highway road", Color::rgb(232, 146, 162))
    } else if rank >= 6 {
        cs.get_def("unzoomed arterial road", Color::rgb(255, 199, 62))
    } else {
        cs.get_def("unzoomed residential road", Color::WHITE)
    }
}

// TODO ETA till goal...
#[derive(Clone, Copy, PartialEq)]
pub enum InnerAgentColorScheme {
    VehicleTypes,
    Delay,
    TripTimeSoFar,
    DistanceCrossedSoFar,
}

impl InnerAgentColorScheme {
    fn data(self, cs: &ColorScheme) -> (&str, Vec<(&str, Color)>) {
        match self {
            InnerAgentColorScheme::VehicleTypes => (
                "vehicle types",
                vec![
                    ("car", cs.get_def("unzoomed car", Color::RED.alpha(0.5))),
                    ("bike", cs.get_def("unzoomed bike", Color::GREEN.alpha(0.5))),
                    ("bus", cs.get_def("unzoomed bus", Color::BLUE.alpha(0.5))),
                    (
                        "pedestrian",
                        cs.get_def("unzoomed pedestrian", Color::ORANGE.alpha(0.5)),
                    ),
                ],
            ),
            InnerAgentColorScheme::Delay => (
                "time spent delayed/blocked",
                vec![
                    ("<= 1 minute", Color::BLUE.alpha(0.3)),
                    ("<= 5 minutes", Color::ORANGE.alpha(0.5)),
                    ("> 5 minutes", Color::RED.alpha(0.8)),
                ],
            ),
            InnerAgentColorScheme::TripTimeSoFar => (
                "trip time so far",
                vec![
                    ("<= 1 minute", Color::BLUE.alpha(0.3)),
                    ("<= 5 minutes", Color::ORANGE.alpha(0.5)),
                    ("> 5 minutes", Color::RED.alpha(0.8)),
                ],
            ),
            InnerAgentColorScheme::DistanceCrossedSoFar => (
                "distance crossed to goal so far",
                vec![
                    ("<= 10%", rotating_color(0)),
                    ("<= 20%", rotating_color(1)),
                    ("<= 30%", rotating_color(2)),
                    ("<= 40%", rotating_color(3)),
                    ("<= 50%", rotating_color(4)),
                    ("<= 60%", rotating_color(5)),
                    ("<= 70%", rotating_color(6)),
                    ("<= 80%", rotating_color(7)),
                    ("<= 90%", rotating_color(8)),
                    ("> 90%", rotating_color(9)),
                ],
            ),
        }
    }

    fn classify(self, agent: &UnzoomedAgent) -> String {
        match self {
            InnerAgentColorScheme::VehicleTypes => match agent.vehicle_type {
                Some(VehicleType::Car) => "car".to_string(),
                Some(VehicleType::Bike) => "bike".to_string(),
                Some(VehicleType::Bus) => "bus".to_string(),
                None => "pedestrian".to_string(),
            },
            InnerAgentColorScheme::Delay => classify_delay(agent.metadata.time_spent_blocked),
            InnerAgentColorScheme::TripTimeSoFar => classify_delay(agent.metadata.trip_time_so_far),
            InnerAgentColorScheme::DistanceCrossedSoFar => {
                classify_percent(agent.metadata.percent_dist_crossed)
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct AgentColorScheme {
    pub acs: InnerAgentColorScheme,
    pub title: String,
    pub rows: Vec<(String, Color, bool)>,
}

impl Cloneable for AgentColorScheme {}

impl AgentColorScheme {
    pub fn new(acs: InnerAgentColorScheme, cs: &ColorScheme) -> AgentColorScheme {
        let (title, rows) = acs.data(cs);
        AgentColorScheme {
            acs,
            title: title.to_string(),
            rows: rows
                .into_iter()
                .map(|(name, color)| (name.to_string(), color, true))
                .collect(),
        }
    }

    pub fn default(cs: &ColorScheme) -> AgentColorScheme {
        AgentColorScheme::new(InnerAgentColorScheme::VehicleTypes, cs)
    }

    pub fn all(cs: &ColorScheme) -> Vec<(AgentColorScheme, String)> {
        vec![
            InnerAgentColorScheme::VehicleTypes,
            InnerAgentColorScheme::Delay,
            InnerAgentColorScheme::TripTimeSoFar,
            InnerAgentColorScheme::DistanceCrossedSoFar,
        ]
        .into_iter()
        .map(|acs| {
            let x = AgentColorScheme::new(acs, cs);
            let title = x.title.clone();
            (x, title)
        })
        .collect()
    }

    pub fn toggle(&mut self, name: String) {
        for (n, _, enabled) in &mut self.rows {
            if &name == n {
                *enabled = !*enabled;
                return;
            }
        }
        panic!("Can't toggle category {}", name);
    }

    fn color(&self, agent: &UnzoomedAgent) -> Option<Color> {
        let category = self.acs.classify(agent);
        for (name, color, enabled) in &self.rows {
            if name == &category {
                if *enabled {
                    return Some(*color);
                }
                return None;
            }
        }
        panic!("Unknown AgentColorScheme category {}", category);
    }
}

fn classify_delay(delay: Duration) -> String {
    if delay <= Duration::minutes(1) {
        return "<= 1 minute".to_string();
    }
    if delay <= Duration::minutes(5) {
        return "<= 5 minutes".to_string();
    }
    "> 5 minutes".to_string()
}

fn classify_percent(percent: f64) -> String {
    if percent > 0.9 {
        return "> 90%".to_string();
    }
    if percent <= 0.1 {
        return "<= 10%".to_string();
    }
    format!("<= {}%", ((percent * 10.0).round() as usize) * 10)
}
