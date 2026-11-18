//! Climate cycles and natural events (PRD FR-W7, FR-E3; TRD TR-C5).
//!
//! When a world turns them on, the climate changes on its own:
//!
//! * a **glacial cycle**: a slow cooling into a glacial maximum and a fast warming back
//!   to an interglacial, which lowers and raises the sea, so coastlines move and land
//!   bridges open and close;
//! * **droughts** (a region gets less rain for several years), **wildfires** (a region
//!   loses its vegetation, which then regrows), **harsh winters** (a region's winter is
//!   colder for a year) and **volcanic eruptions** (a region cools for two years, and
//!   life near the vent is killed).
//!
//! Everything is drawn from the world seed and the year, so a world replays exactly.
//! The settings and the running state live in [`Nature`], outside the bincode state:
//! they are saved after the "Why?" records and count towards `state_hash()` only when
//! something is on, so worlds without them behave, save and hash as before.

use crate::animal::Cause;
use crate::record::EventKind;
use crate::rng::{stream, Rng};
use crate::sim::{Mode, Simulation};
use crate::world::{Biome, World};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// A glacial/interglacial cycle (PRD FR-W7).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GlacialCycle {
    /// Length of one cycle, years (Earth's late Pleistocene: ~100,000).
    pub period_years: f64,
    /// Global cooling at the glacial maximum, °C.
    pub cooling_c: f32,
    /// Sea-level fall at the glacial maximum, metres.
    pub sea_drop_m: f32,
}

impl Default for GlacialCycle {
    fn default() -> Self {
        GlacialCycle {
            period_years: 100_000.0,
            cooling_c: 6.0,
            sea_drop_m: 120.0,
        }
    }
}

/// Share of sea tiles that are continental shelf (20–130 m deep) in worlds with a glacial
/// cycle; about 8% of Earth's ocean is.
pub const SHELF_SHARE: f64 = 0.1;

/// Share of a cycle spent cooling into the glacial maximum; the rest is the warming.
pub const COOLING_SHARE: f64 = 0.875;

impl GlacialCycle {
    /// How glacial the world is at `year`: 0 in the interglacial (year 0), 1 at the
    /// glacial maximum. A slow smooth cooling, then a fast smooth warming (a sawtooth,
    /// as in the ice-core record).
    pub fn intensity(&self, year: f64) -> f64 {
        let phase = (year / self.period_years).rem_euclid(1.0);
        let s = |x: f64| x * x * (3.0 - 2.0 * x);
        if phase < COOLING_SHARE {
            s(phase / COOLING_SHARE)
        } else {
            s((1.0 - phase) / (1.0 - COOLING_SHARE))
        }
    }

    /// Years between updates of the cycle: 1,000 updates per cycle, 1–100 years apart.
    pub fn step_years(&self) -> i64 {
        ((self.period_years / 1000.0).floor() as i64).clamp(1, 100)
    }
}

/// Which climate cycles and natural events a world has. Chosen when the world is made.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NatureSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glacial: Option<GlacialCycle>,
    #[serde(default)]
    pub droughts: bool,
    #[serde(default)]
    pub fires: bool,
    #[serde(default)]
    pub harsh_winters: bool,
    #[serde(default)]
    pub eruptions: bool,
}

impl NatureSettings {
    /// Everything on, with the default cycle.
    pub fn all() -> NatureSettings {
        NatureSettings {
            glacial: Some(GlacialCycle::default()),
            droughts: true,
            fires: true,
            harsh_winters: true,
            eruptions: true,
        }
    }

    pub fn any(&self) -> bool {
        self.glacial.is_some()
            || self.droughts
            || self.fires
            || self.harsh_winters
            || self.eruptions
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(c) = &self.glacial {
            if !(1000.0..=2_000_000.0).contains(&c.period_years) {
                return Err("the glacial cycle must last 1,000–2,000,000 years".into());
            }
            if !(0.0..=15.0).contains(&c.cooling_c) {
                return Err("glacial cooling must be 0–15 °C".into());
            }
            if !(0.0..=300.0).contains(&c.sea_drop_m) {
                return Err("the glacial sea-level fall must be 0–300 m".into());
            }
        }
        Ok(())
    }
}

/// Kinds of natural event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NatureKind {
    Drought,
    Wildfire,
    HarshWinter,
    Eruption,
}

impl NatureKind {
    /// Chance per year that one starts somewhere in the world.
    fn chance(self) -> f64 {
        match self {
            NatureKind::Drought => 0.08,
            NatureKind::Wildfire => 0.15,
            NatureKind::HarshWinter => 0.08,
            NatureKind::Eruption => 0.02,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NatureKind::Drought => "Drought",
            NatureKind::Wildfire => "Wildfire",
            NatureKind::HarshWinter => "Harsh winter",
            NatureKind::Eruption => "Volcanic eruption",
        }
    }
}

/// A natural event still changing the climate, with the offsets it added (so they can
/// be taken away again when it ends).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveEvent {
    pub kind: NatureKind,
    pub start: f64,
    /// Year it ends.
    pub until: f64,
    /// Tile at its centre.
    pub centre: u32,
    pub tiles: Vec<u32>,
    /// Added to `temp_base`, `temp_amp` (by magnitude) and `precip` of `tiles`; each is
    /// empty when the event does not change that field.
    pub d_temp: Vec<f32>,
    pub d_amp: Vec<f32>,
    pub d_precip: Vec<f32>,
}

/// One point of the climate chart.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClimateSample {
    pub year: f64,
    /// Global temperature offset (cycle, interventions and impact winters), °C.
    pub temp: f32,
    /// Sea level relative to the world's start, metres.
    pub sea: f32,
    /// Natural events under way.
    pub events: u16,
}

/// A natural event that happened, for chart markers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PastEvent {
    pub year: f64,
    pub kind: NatureKind,
    pub centre: u32,
    pub until: f64,
    /// Radius of the region it hit, tiles.
    pub radius: f32,
}

const SERIES_CAP: usize = 1500;
const HISTORY_CAP: usize = 2000;

/// The world's climate cycles and natural events: settings and running state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Nature {
    /// None when the world has none of them (the default).
    pub settings: Option<NatureSettings>,
    /// Last whole year processed.
    pub year: i64,
    /// Cycle cooling currently included in the world's global offset, °C (≤ 0).
    pub temp_applied: f32,
    /// Sea-level change currently applied, whole metres (≤ 0 in a glacial).
    pub sea_applied: f32,
    /// Large landmasses after the last sea-level change (or at creation).
    pub landmasses: u32,
    /// Smallest landmass counted, tiles.
    pub landmass_min: u32,
    /// Whether the glacial maximum of the current cycle has been reported.
    pub max_reported: bool,
    /// Bumped whenever the coastline moves, so views know to redraw the terrain.
    pub coast_rev: u32,
    pub active: Vec<ActiveEvent>,
    pub series: Vec<ClimateSample>,
    pub history: Vec<PastEvent>,
}

impl Nature {
    pub fn new(settings: Option<NatureSettings>, world: &World) -> Nature {
        let settings = settings.filter(|s| s.any());
        let mut n = Nature {
            settings,
            ..Default::default()
        };
        if n.settings.is_some() {
            n.landmass_min = (world.land_tiles() as u32 / 200).max(8);
            n.landmasses = landmasses(world, n.landmass_min);
            n.push_sample(0.0, world);
        }
        n
    }

    pub fn on(&self) -> bool {
        self.settings.is_some()
    }

    fn push_sample(&mut self, year: f64, world: &World) {
        self.series.push(ClimateSample {
            year,
            temp: world.global_offset,
            sea: self.sea_applied,
            events: self.active.len() as u16,
        });
        if self.series.len() > SERIES_CAP {
            let kept: Vec<ClimateSample> = self
                .series
                .drain(..)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, s)| s)
                .collect();
            self.series = kept;
        }
    }
}

/// Number of land areas (4-connected, wrapping east–west) of at least `min` tiles.
pub fn landmasses(w: &World, min: u32) -> u32 {
    let n = w.w * w.h;
    let mut seen = vec![false; n];
    let mut count = 0;
    let mut q = VecDeque::new();
    for s in 0..n {
        if seen[s] || !w.is_land(s) {
            continue;
        }
        seen[s] = true;
        q.push_back(s);
        let mut size = 0u32;
        while let Some(i) = q.pop_front() {
            size += 1;
            for d in [0, 2, 4, 6] {
                if let Some(j) = w.neighbour(i, d) {
                    if !seen[j] && w.is_land(j) {
                        seen[j] = true;
                        q.push_back(j);
                    }
                }
            }
        }
        if size >= min {
            count += 1;
        }
    }
    count
}

/// "34°N 75°W" for a tile.
pub fn place(w: &World, i: usize) -> String {
    let (x, y) = (i % w.w, i / w.w);
    let lat = w.lat[y];
    let lon = (x as f64 + 0.5) / w.w as f64 * 360.0 - 180.0;
    format!(
        "{:.0}°{} {:.0}°{}",
        lat.abs(),
        if lat >= 0.0 { "N" } else { "S" },
        lon.abs(),
        if lon >= 0.0 { "E" } else { "W" }
    )
}

/// Tiles within `r` tiles of `centre` (wrapping east–west), with a weight falling
/// smoothly from 1 at the centre to 0 at the edge.
fn disc(w: &World, centre: usize, r: f32) -> Vec<(usize, f32)> {
    let (cx, cy) = ((centre % w.w) as f32, (centre / w.w) as f32);
    let ri = r.ceil() as i64;
    let mut out = vec![];
    for dy in -ri..=ri {
        let y = cy as i64 + dy;
        if y < 0 || y >= w.h as i64 {
            continue;
        }
        for dx in -ri..=ri {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > r {
                continue;
            }
            let x = (cx as i64 + dx).rem_euclid(w.w as i64) as usize;
            let wt = 0.5 + 0.5 * (std::f32::consts::PI * d / r.max(1e-3)).cos();
            out.push((y as usize * w.w + x, wt));
        }
    }
    out.sort_by_key(|&(i, _)| i);
    out.dedup_by_key(|&mut (i, _)| i);
    out
}

/// A random land tile passing `ok`, or None after 40 tries.
fn pick_tile(w: &World, rng: &mut Rng, ok: impl Fn(usize) -> bool) -> Option<usize> {
    let n = w.w * w.h;
    (0..40)
        .map(|_| rng.below(n))
        .find(|&i| w.is_land(i) && ok(i))
}

fn fire_prone(b: Biome) -> bool {
    matches!(
        b,
        Biome::Boreal
            | Biome::TemperateGrassland
            | Biome::TemperateForest
            | Biome::Savanna
            | Biome::TropicalSeasonalForest
            | Biome::ColdDesert
    )
}

/// Radius in tiles for a distance in km: at least one tile, at most a quarter of the map.
fn tiles_for_km(w: &World, km: f32) -> f32 {
    (km / w.tile_km as f32).clamp(1.0, w.w as f32 / 4.0)
}

impl Simulation {
    /// Advances the climate cycle and natural events to the current year. Called after
    /// every step; does nothing until a new whole year starts, or when nature is off.
    pub(crate) fn advance_nature(&mut self) {
        if !self.nature.on() {
            return;
        }
        let y = self.year().floor() as i64;
        let from = self.nature.year;
        if y <= from {
            return;
        }
        self.nature.year = y;
        let settings = self.nature.settings.clone().unwrap_or_default();
        let mut dirty = false;

        // Events that are over give their offsets back.
        let (ended, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.nature.active)
            .into_iter()
            .partition(|e| e.until <= y as f64);
        self.nature.active = still;
        for e in ended {
            self.remove_offsets(&e);
            dirty = true;
            let text = match e.kind {
                NatureKind::Drought => {
                    format!(
                        "The drought around {} ends.",
                        place(&self.world, e.centre as usize)
                    )
                }
                NatureKind::Eruption => format!(
                    "The volcanic haze around {} clears.",
                    place(&self.world, e.centre as usize)
                ),
                _ => String::new(),
            };
            if !text.is_empty() {
                self.event(EventKind::Nature, None, text);
            }
        }

        if let Some(cycle) = &settings.glacial {
            let step = cycle.step_years();
            if y.div_euclid(step) != from.div_euclid(step) {
                dirty |= self.advance_cycle(cycle, (y.div_euclid(step) * step) as f64);
            }
        }

        // Short events only in Individual and Population mode: Macro mode's 1,000-year
        // steps average them out.
        if self.mode != Mode::Macro {
            for kind in [
                NatureKind::Drought,
                NatureKind::Wildfire,
                NatureKind::HarshWinter,
                NatureKind::Eruption,
            ] {
                let on = match kind {
                    NatureKind::Drought => settings.droughts,
                    NatureKind::Wildfire => settings.fires,
                    NatureKind::HarshWinter => settings.harsh_winters,
                    NatureKind::Eruption => settings.eruptions,
                };
                if on {
                    dirty |= self.maybe_start(kind, y);
                }
            }
        }
        if dirty {
            self.world.refresh_biomes();
        }
        self.nature.push_sample(y as f64, &self.world);
    }

    /// Moves the cycle's temperature and sea level to their values at `year`. Returns
    /// whether the climate fields changed without a refresh.
    fn advance_cycle(&mut self, cycle: &GlacialCycle, year: f64) -> bool {
        let g = cycle.intensity(year);
        let temp = -(cycle.cooling_c as f64 * g) as f32;
        let sea = -(cycle.sea_drop_m as f64 * g).round() as f32;
        let mut dirty = false;
        let dt = temp - self.nature.temp_applied;
        if dt != 0.0 {
            self.world.global_offset += dt;
            self.nature.temp_applied = temp;
            dirty = true;
        }
        let rise = sea - self.nature.sea_applied;
        if rise != 0.0 {
            // Counted afresh: world edits may have changed the land since the last change.
            let was = landmasses(&self.world, self.nature.landmass_min);
            let flooded = self.world.change_sea_level(rise);
            self.nature.sea_applied = sea;
            self.nature.coast_rev = self.nature.coast_rev.wrapping_add(1);
            dirty = false;
            let drowned = self.drown(&flooded, 2);
            let now = landmasses(&self.world, self.nature.landmass_min);
            self.nature.landmasses = now;
            if now != was {
                let what = match (rise < 0.0, now < was) {
                    (true, true) => "opened a land bridge",
                    (true, false) => "exposed new land",
                    (false, true) => "drowned a landmass",
                    (false, false) => "flooded a land bridge",
                };
                self.event(
                    EventKind::Nature,
                    None,
                    format!(
                        "{} seas (sea level {:+.0} m) {what}: now {}, was {was}{}.",
                        if rise < 0.0 { "Falling" } else { "Rising" },
                        sea,
                        if now == 1 {
                            "1 large landmass".to_string()
                        } else {
                            format!("{now} large landmasses")
                        },
                        if drowned > 0 {
                            format!("; {drowned} organisms drowned")
                        } else {
                            String::new()
                        }
                    ),
                );
            }
        }
        let phase = (year / cycle.period_years).rem_euclid(1.0);
        if phase >= COOLING_SHARE && !self.nature.max_reported && g > 0.0 {
            self.nature.max_reported = true;
            self.event(
                EventKind::Nature,
                None,
                format!(
                    "Glacial maximum: the world is {:.1} °C colder and the sea {:.0} m lower than at the start of the cycle. The ice now retreats.",
                    -temp, -sea
                ),
            );
        } else if phase < COOLING_SHARE && self.nature.max_reported {
            self.nature.max_reported = false;
            self.event(
                EventKind::Nature,
                None,
                "Interglacial: the ice has retreated and the sea is back near its starting level. A new glacial cycle begins."
                    .into(),
            );
        }
        dirty
    }

    /// Starts an event of `kind` in `year` if the year's draw says so. Returns whether
    /// it changed the climate fields.
    fn maybe_start(&mut self, kind: NatureKind, year: i64) -> bool {
        let mut rng = Rng::keyed(self.seed, year as u64, kind as u64, stream::NATURE);
        if !rng.chance(kind.chance()) {
            return false;
        }
        let w = &self.world;
        let centre = match kind {
            NatureKind::Wildfire => pick_tile(w, &mut rng, |i| {
                fire_prone(w.biome[i]) && w.forage[i] + w.roughage[i] > 0.0
            }),
            NatureKind::HarshWinter => pick_tile(w, &mut rng, |i| w.lat[i / w.w].abs() > 25.0),
            NatureKind::Eruption => pick_tile(w, &mut rng, |i| w.elevation[i] > 1000.0)
                .or_else(|| pick_tile(w, &mut rng, |_| true)),
            NatureKind::Drought => pick_tile(w, &mut rng, |i| w.precip[i] > 100.0),
        };
        let Some(centre) = centre else {
            return false;
        };
        let y = year as f64;
        let at = place(w, centre);
        match kind {
            NatureKind::Drought => {
                let km = rng.range_f32(300.0, 800.0);
                let years = 2 + rng.below(5) as u32;
                let cut = rng.range_f32(0.4, 0.7);
                let r = tiles_for_km(w, km);
                let tiles = disc(w, centre, r);
                let mut e = self.new_event(kind, y, y + years as f64, centre, r);
                for &(i, wt) in &tiles {
                    if self.world.is_land(i) {
                        e.tiles.push(i as u32);
                        e.d_precip.push(-cut * wt * self.world.precip[i]);
                    }
                }
                self.add_offsets(&e);
                self.event(
                    EventKind::Nature,
                    None,
                    format!(
                        "Drought around {at}: up to {:.0}% less rain over ~{km:.0} km for {years} years.",
                        cut * 100.0
                    ),
                );
                self.nature.active.push(e);
                true
            }
            NatureKind::Wildfire => {
                let km = rng.range_f32(50.0, 250.0);
                let mut burned = 0;
                let r = tiles_for_km(w, km);
                for (i, _) in disc(w, centre, r) {
                    if self.world.is_land(i) && fire_prone(self.world.biome[i]) {
                        self.world.forage[i] = 0.0;
                        self.world.roughage[i] = 0.0;
                        self.world.carcass[i] = 0.0;
                        burned += 1;
                    }
                }
                self.world.vegetation_changed();
                self.remember(kind, y, y + 1.0, centre, r);
                self.event(
                    EventKind::Nature,
                    None,
                    format!("Wildfire around {at} burned the vegetation of {burned} tiles; it will grow back."),
                );
                false
            }
            NatureKind::HarshWinter => {
                let km = rng.range_f32(800.0, 2000.0);
                let cold = rng.range_f32(2.0, 4.0);
                let r = tiles_for_km(w, km);
                let tiles = disc(w, centre, r);
                let mut e = self.new_event(kind, y, y + 1.0, centre, r);
                for (i, wt) in tiles {
                    e.tiles.push(i as u32);
                    e.d_temp.push(-cold * wt);
                    e.d_amp.push(cold * wt);
                }
                self.add_offsets(&e);
                self.event(
                    EventKind::Nature,
                    None,
                    format!(
                        "Harsh winter around {at}: winters up to {:.0} °C colder over ~{km:.0} km this year.",
                        2.0 * cold
                    ),
                );
                self.nature.active.push(e);
                true
            }
            NatureKind::Eruption => {
                let km = rng.range_f32(1500.0, 3000.0);
                let cool = rng.range_f32(1.5, 3.5);
                let r = tiles_for_km(w, km);
                let tiles = disc(w, centre, r);
                let vent = disc(w, centre, tiles_for_km(w, 60.0));
                let mut e = self.new_event(kind, y, y + 2.0, centre, r);
                for (i, wt) in tiles {
                    e.tiles.push(i as u32);
                    e.d_temp.push(-cool * wt);
                }
                self.add_offsets(&e);
                let killed = self.devastate(&vent);
                self.event(
                    EventKind::Nature,
                    None,
                    format!(
                        "Volcanic eruption at {at}: up to {cool:.1} °C cooler over ~{km:.0} km for 2 years{}.",
                        if killed > 0 {
                            format!("; {killed} organisms killed near the vent")
                        } else {
                            String::new()
                        }
                    ),
                );
                self.nature.active.push(e);
                true
            }
        }
    }

    fn new_event(
        &mut self,
        kind: NatureKind,
        start: f64,
        until: f64,
        centre: usize,
        radius: f32,
    ) -> ActiveEvent {
        self.remember(kind, start, until, centre, radius);
        ActiveEvent {
            kind,
            start,
            until,
            centre: centre as u32,
            tiles: vec![],
            d_temp: vec![],
            d_amp: vec![],
            d_precip: vec![],
        }
    }

    fn remember(&mut self, kind: NatureKind, year: f64, until: f64, centre: usize, radius: f32) {
        let h = &mut self.nature.history;
        h.push(PastEvent {
            year,
            kind,
            centre: centre as u32,
            until,
            radius,
        });
        if h.len() > HISTORY_CAP {
            h.drain(..h.len() - HISTORY_CAP);
        }
    }

    fn add_offsets(&mut self, e: &ActiveEvent) {
        self.offsets(e, 1.0);
    }

    fn remove_offsets(&mut self, e: &ActiveEvent) {
        self.offsets(e, -1.0);
    }

    fn offsets(&mut self, e: &ActiveEvent, sign: f32) {
        let w = &mut self.world;
        for (k, &t) in e.tiles.iter().enumerate() {
            let i = t as usize;
            if let Some(d) = e.d_temp.get(k) {
                w.temp_base[i] += sign * d;
            }
            if let Some(d) = e.d_amp.get(k) {
                let s = if w.temp_amp[i] < 0.0 { -1.0 } else { 1.0 };
                w.temp_amp[i] += sign * s * d;
            }
            if let Some(d) = e.d_precip.get(k) {
                w.precip[i] = (w.precip[i] + sign * d).max(0.0);
            }
        }
    }

    /// Natural events under way at some time in [`from`, `to`] whose region reached
    /// where `species` lives now, newest first (for "Why?" explanations).
    pub(crate) fn natural_events_over(&self, species: u32, from: f64, to: f64) -> Vec<&PastEvent> {
        let w = &self.world;
        let ps = self.params.patch_size as f32;
        let pw = self.patches_w();
        let mut spots: Vec<(f32, f32)> = self
            .demes
            .iter()
            .filter(|d| d.species == species)
            .map(|d| {
                (
                    ((d.patch as usize % pw) as f32 + 0.5) * ps,
                    ((d.patch as usize / pw) as f32 + 0.5) * ps,
                )
            })
            .collect();
        let reach = if spots.is_empty() { 0.0 } else { ps * 0.71 };
        spots.extend(
            self.animals
                .iter()
                .filter(|a| a.species == species)
                .map(|a| (a.x, a.y)),
        );
        self.nature
            .history
            .iter()
            .rev()
            .filter(|e| e.year <= to && e.until >= from)
            .filter(|e| {
                let (cx, cy) = (
                    (e.centre as usize % w.w) as f32 + 0.5,
                    (e.centre as usize / w.w) as f32 + 0.5,
                );
                spots.iter().any(|&(x, y)| {
                    let mut dx = (x - cx).abs();
                    dx = dx.min(w.w as f32 - dx);
                    (dx * dx + (y - cy).powi(2)).sqrt() <= e.radius + reach
                })
            })
            .collect()
    }

    /// Lava and ash near a vent: vegetation gone, animals there killed, and each deme in
    /// a patch loses the share of its land that was hit.
    fn devastate(&mut self, tiles: &[(usize, f32)]) -> u64 {
        let n = self.world.w * self.world.h;
        let mut hit = vec![false; n];
        let mut patch_hit = vec![0u32; self.patches_w() * self.patches_h()];
        for &(i, _) in tiles {
            if self.world.is_land(i) {
                hit[i] = true;
                patch_hit[self.patch_of_tile(i) as usize] += 1;
                self.world.forage[i] = 0.0;
                self.world.roughage[i] = 0.0;
            }
        }
        self.world.vegetation_changed();
        let mut killed = 0u64;
        for i in 0..self.animals.len() {
            let a = &self.animals[i];
            if a.alive && hit[self.world.tile_of(a.x, a.y)] {
                self.kill(i, Cause::Catastrophe);
                killed += 1;
            }
        }
        self.animals.retain(|a| a.alive);
        let mut land = vec![0u32; patch_hit.len()];
        for i in (0..n).filter(|&i| self.world.is_land(i)) {
            land[self.patch_of_tile(i) as usize] += 1;
        }
        let mut rng = Rng::keyed(self.seed, self.minutes, 3, stream::NATURE);
        for d in self.demes.iter_mut().chain(self.frozen.iter_mut()) {
            let p = d.patch as usize;
            if patch_hit[p] > 0 {
                killed += d.cull(patch_hit[p] as f64 / land[p].max(1) as f64, &mut rng);
            }
        }
        self.demes.retain(|d| d.total() > 0);
        self.frozen.retain(|d| d.total() > 0);
        killed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::population::border_connectivity;
    use crate::sim::NewWorld;
    use crate::world::Water;

    fn world_with(nature: NatureSettings, seed: u64) -> Simulation {
        Simulation::new(NewWorld {
            template: "blank".into(),
            seed,
            gen: None,
            founders: Some(vec![]),
            realism: None,
            label: None,
            edits: vec![],
            designs: vec![],
            nature: Some(nature),
        })
        .unwrap()
    }

    /// Two islands: deep sea in patch column 0, a 30 m deep strait in patch column 3.
    fn two_islands(sim: &mut Simulation) {
        let ps = sim.params.patch_size;
        let w = &mut sim.world;
        for i in 0..w.w * w.h {
            let px = (i % w.w) / ps;
            w.elevation[i] = match px {
                0 => -2000.0,
                3 => -30.0,
                _ => 100.0,
            };
        }
        let zero = vec![0.0; w.w * w.h];
        w.recompute_after_edit(&zero, &zero);
        sim.nature.landmasses = landmasses(&sim.world, sim.nature.landmass_min);
    }

    fn strait_open(sim: &Simulation) -> bool {
        let ps = sim.params.patch_size;
        (0..sim.patches_h()).all(|py| border_connectivity(&sim.world, ps, 2, py, 1, 0) == 0.0)
    }

    fn bridged(sim: &Simulation) -> bool {
        let ps = sim.params.patch_size;
        (0..sim.patches_h()).all(|py| {
            border_connectivity(&sim.world, ps, 2, py, 1, 0) > 0.0
                && border_connectivity(&sim.world, ps, 4, py, -1, 0) > 0.0
        })
    }

    #[test]
    fn falling_seas_join_two_islands_and_rising_seas_split_them() {
        let mut sim = world_with(
            NatureSettings {
                glacial: Some(GlacialCycle {
                    period_years: 1000.0,
                    cooling_c: 6.0,
                    sea_drop_m: 120.0,
                }),
                ..Default::default()
            },
            5,
        );
        two_islands(&mut sim);
        assert_eq!(sim.nature.landmasses, 2);
        assert!(strait_open(&sim));
        sim.set_mode(crate::sim::Mode::Population).unwrap();
        sim.run_years(500.0);
        // Glacial: sea about 73 m lower and 3.6 °C colder.
        assert!(sim.nature.sea_applied < -60.0, "{}", sim.nature.sea_applied);
        assert!(sim.world.global_offset < -3.0);
        assert!(bridged(&sim), "the strait should be dry land");
        assert_eq!(landmasses(&sim.world, sim.nature.landmass_min), 1);
        assert!(sim
            .events
            .iter()
            .any(|e| e.text.contains("opened a land bridge")));
        sim.run_years(400.0);
        assert!(sim
            .events
            .iter()
            .any(|e| e.text.starts_with("Glacial maximum")));
        sim.run_years(95.0);
        // Back near the interglacial: the strait is sea again.
        assert!(sim.nature.sea_applied > -25.0, "{}", sim.nature.sea_applied);
        assert!(strait_open(&sim));
        assert!(sim
            .events
            .iter()
            .any(|e| e.text.contains("flooded a land bridge")));
        assert_eq!(sim.world.water[3 * sim.params.patch_size], Water::Ocean);
        sim.run_years(10.0);
        assert!(sim
            .events
            .iter()
            .any(|e| e.text.starts_with("Interglacial")));
        assert!(sim.world.global_offset.abs() < 0.01);
        assert_eq!(sim.nature.sea_applied, 0.0);
        // The chart series follows the cycle.
        let low = sim.nature.series.iter().map(|s| s.sea).fold(0.0, f32::min);
        assert_eq!(low, -120.0);
    }

    #[test]
    fn glacial_worlds_get_continental_shelves_and_keep_their_land() {
        let make = |nature: Option<NatureSettings>| {
            Simulation::new(NewWorld {
                template: "archipelago".into(),
                seed: 4,
                gen: None,
                founders: Some(vec![]),
                realism: None,
                label: None,
                edits: vec![],
                designs: vec![],
                nature,
            })
            .unwrap()
        };
        let plain = make(None);
        let ice = make(Some(NatureSettings {
            glacial: Some(GlacialCycle::default()),
            ..Default::default()
        }));
        let sea: Vec<usize> = (0..plain.world.w * plain.world.h)
            .filter(|&i| !plain.world.is_land(i))
            .collect();
        let shallow = |w: &World| sea.iter().filter(|&&i| w.elevation[i] > -130.5).count();
        let n = shallow(&ice.world);
        assert_eq!(n, (sea.len() as f64 * SHELF_SHARE) as usize);
        assert!(
            shallow(&plain.world) * 2 < n,
            "{} {n}",
            shallow(&plain.world)
        );
        // Land, coast and climate are as generated.
        for i in 0..plain.world.w * plain.world.h {
            assert_eq!(plain.world.water[i], ice.world.water[i]);
            assert_eq!(plain.world.temp_base[i], ice.world.temp_base[i]);
            if plain.world.is_land(i) {
                assert_eq!(plain.world.elevation[i], ice.world.elevation[i]);
            }
        }
        // Droughts alone leave the sea floor as generated.
        let dry = make(Some(NatureSettings {
            droughts: true,
            ..Default::default()
        }));
        assert_eq!(dry.world.elevation, plain.world.elevation);
    }

    #[test]
    fn a_drought_cuts_rain_in_its_region_and_gives_it_back() {
        let mut sim = world_with(
            NatureSettings {
                droughts: true,
                ..Default::default()
            },
            8,
        );
        sim.set_mode(crate::sim::Mode::Population).unwrap();
        let before = sim.world.precip.clone();
        let mut years = 0;
        while sim.nature.active.is_empty() && years < 200 {
            sim.run_years(1.0);
            years += 1;
        }
        let e = sim.nature.active[0].clone();
        assert_eq!(e.kind, NatureKind::Drought);
        let c = e.centre as usize;
        assert!(sim.world.precip[c] < before[c] * 0.65);
        assert!(sim.world.npp[c] < crate::world::World::generate(sim.world.gen.clone(), 8).npp[c]);
        assert!(sim
            .events
            .iter()
            .any(|ev| ev.text.starts_with("Drought around")));
        // No new droughts: once this one (and any other) ends, the rain is back.
        sim.nature.settings = Some(NatureSettings::default());
        while !sim.nature.active.is_empty() {
            sim.run_years(1.0);
        }
        for (a, b) in sim.world.precip.iter().zip(&before) {
            assert!((a - b).abs() < 1e-3);
        }
        assert!(sim
            .events
            .iter()
            .any(|ev| ev.text.contains("drought around")));
    }

    #[test]
    fn every_kind_of_event_happens_and_climate_returns() {
        let mut sim = world_with(
            NatureSettings {
                glacial: None,
                ..NatureSettings::all()
            },
            3,
        );
        sim.set_mode(crate::sim::Mode::Population).unwrap();
        let temp = sim.world.temp_base.clone();
        let amp = sim.world.temp_amp.clone();
        sim.run_years(400.0);
        for kind in [
            NatureKind::Drought,
            NatureKind::Wildfire,
            NatureKind::HarshWinter,
            NatureKind::Eruption,
        ] {
            assert!(
                sim.nature.history.iter().any(|h| h.kind == kind),
                "no {kind:?} in 400 years"
            );
        }
        // With no new events, every offset is given back once the last one ends.
        sim.nature.settings = Some(NatureSettings::default());
        sim.run_years(7.0);
        assert!(sim.nature.active.is_empty());
        for i in 0..temp.len() {
            assert!((sim.world.temp_base[i] - temp[i]).abs() < 1e-3);
            assert!((sim.world.temp_amp[i] - amp[i]).abs() < 1e-3);
        }
    }

    #[test]
    fn the_cycle_cools_slowly_and_warms_fast() {
        let c = GlacialCycle::default();
        assert_eq!(c.intensity(0.0), 0.0);
        assert!((c.intensity(87_500.0) - 1.0).abs() < 1e-12);
        assert!((c.intensity(100_000.0)).abs() < 1e-12);
        assert!(c.intensity(43_750.0) > 0.49 && c.intensity(43_750.0) < 0.51);
        assert!(c.intensity(93_750.0) > 0.49 && c.intensity(93_750.0) < 0.51);
        // Monotonic within each half.
        let mut last = -1.0;
        for k in 0..=875 {
            let g = c.intensity(k as f64 * 100.0);
            assert!(g >= last);
            last = g;
        }
        assert_eq!(c.step_years(), 100);
        assert_eq!(
            GlacialCycle {
                period_years: 1000.0,
                ..c
            }
            .step_years(),
            1
        );
    }

    #[test]
    fn bad_settings_are_refused() {
        let mut s = NatureSettings::all();
        assert!(s.validate().is_ok());
        s.glacial.as_mut().unwrap().period_years = 10.0;
        assert!(s.validate().is_err());
        let mut s = NatureSettings::all();
        s.glacial.as_mut().unwrap().cooling_c = f32::NAN;
        assert!(s.validate().is_err());
        let mut s = NatureSettings::all();
        s.glacial.as_mut().unwrap().sea_drop_m = 500.0;
        assert!(s.validate().is_err());
        assert!(!NatureSettings::default().any());
    }
}
