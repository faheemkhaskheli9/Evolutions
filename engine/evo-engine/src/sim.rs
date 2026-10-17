//! The simulation: owns the world, genomes, organisms, demes and records, and
//! exposes the engine API used by the CLI, the WASM bindings and tests
//! (TRD §10, System Design §1.2).

use crate::animal::{Animal, Cause, Sex};
use crate::genetics::{AlleleTable, Genome, GenomeArch, MutationRates};
use crate::individual::Grid;
use crate::params::{Realism, SimParams};
use crate::physiology::{derive, growth_mass};
use crate::population::Deme;
use crate::record::{Event, EventKind, Metrics, TraitStats};
use crate::rng::{stream, Rng};
use crate::species::SpeciesRegistry;
use crate::species_lib;
use crate::templates::{self, FounderGroup};
use crate::traits::{Phenotype, Trait, N_TRAITS};
use crate::world::{World, WorldGenParams};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SAVE_MAGIC: &[u8; 4] = b"EVO1";
/// Bumped whenever a model change alters outcomes (TRD TR-DA2).
pub const MODEL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Every organism simulated (1-hour ticks).
    Individual,
    /// Demes with allele frequencies (1-year steps).
    Population,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewWorld {
    /// Template key, or "random".
    pub template: String,
    pub seed: u64,
    #[serde(default)]
    pub gen: Option<WorldGenParams>,
    #[serde(default)]
    pub founders: Option<Vec<FounderGroup>>,
    #[serde(default)]
    pub realism: Option<Realism>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Intervention {
    /// Global warming (+) or cooling (−), °C. Conditions only; genomes untouched (PRD FR-E6).
    ClimateShift {
        delta_c: f32,
    },
    /// Impact at (fx, fy) in map fractions; radius as a fraction of map width.
    Meteor {
        fx: f32,
        fy: f32,
        radius: f32,
    },
    AddFounders {
        group: FounderGroup,
    },
    /// Epidemic killing a fraction of one species.
    Plague {
        species: u32,
        mortality: f32,
    },
}

#[derive(Serialize, Deserialize)]
pub struct Simulation {
    pub model_version: u32,
    pub seed: u64,
    pub params: SimParams,
    pub template_key: String,
    pub template_name: String,
    /// Simulated time, minutes since world start (TRD TR-T1).
    pub minutes: u64,
    pub world: World,
    pub archs: Vec<GenomeArch>,
    pub alleles: AlleleTable,
    pub animals: Vec<Animal>,
    pub next_id: u64,
    pub demes: Vec<Deme>,
    /// Demes outside the focus region, held while a region is simulated individually.
    #[serde(default)]
    pub frozen: Vec<Deme>,
    /// Focus region in tile coordinates (x0, y0, x1, y1), when Individual mode covers only part of the world.
    #[serde(default)]
    pub focus: Option<(u32, u32, u32, u32)>,
    pub mode: Mode,
    pub species: SpeciesRegistry,
    pub metrics: Metrics,
    pub events: Vec<Event>,
    pub births: u64,
    pub deaths: [u64; 9],
    pub last_day: u64,
    pub last_metric_year: f64,
    pub last_species_check: f64,
    /// Temporary climate offsets to undo: (year, delta).
    pub climate_effects: Vec<(f64, f32)>,
    /// Founder groups waiting for their release year.
    pub scheduled: Vec<(f64, FounderGroup)>,
    #[serde(skip)]
    pub(crate) grid: Grid,
    #[serde(skip)]
    pub(crate) index: crate::individual::IdMap,
    #[serde(skip)]
    pub(crate) pending: Vec<Animal>,
    /// Last Population-mode step's vital rates per deme: (species, patch, n, rates).
    #[serde(skip)]
    pub diagnostics: Vec<(u32, u32, u32, crate::population::Annual)>,
}

impl Simulation {
    pub fn new(cfg: NewWorld) -> Result<Simulation, String> {
        let tmpl = if cfg.template == "random" {
            templates::random(cfg.seed)
        } else {
            templates::find(&cfg.template)
                .ok_or_else(|| format!("unknown template '{}'", cfg.template))?
        };
        let gen = cfg.gen.unwrap_or_else(|| tmpl.gen.clone());
        if gen.width < 8 || gen.height < 8 || gen.width * gen.height > 1 << 20 {
            return Err("world size must be between 8×8 and ~1M tiles".into());
        }
        let world = World::generate(gen, cfg.seed);
        let mut params = SimParams::default();
        if let Some(r) = cfg.realism {
            params.realism = r;
        }
        let mut sim = Simulation {
            model_version: MODEL_VERSION,
            seed: cfg.seed,
            params,
            template_key: tmpl.key.clone(),
            template_name: tmpl.name.clone(),
            minutes: 0,
            world,
            archs: vec![],
            alleles: AlleleTable::default(),
            animals: vec![],
            next_id: 1,
            demes: vec![],
            frozen: vec![],
            focus: None,
            mode: Mode::Individual,
            species: SpeciesRegistry::default(),
            metrics: Metrics::default(),
            events: vec![],
            births: 0,
            deaths: [0; 9],
            last_day: u64::MAX,
            last_metric_year: -1.0,
            last_species_check: 0.0,
            climate_effects: vec![],
            scheduled: vec![],
            grid: Grid::default(),
            index: Default::default(),
            pending: vec![],
            diagnostics: vec![],
        };
        sim.event(
            EventKind::Info,
            None,
            format!(
                "World '{}' created (seed {}, {}×{} tiles, ~{:.0} km per tile, {:.0}% land).",
                tmpl.name,
                cfg.seed,
                sim.world.w,
                sim.world.h,
                sim.world.tile_km,
                100.0 * sim.world.land_tiles() as f64 / (sim.world.w * sim.world.h) as f64
            ),
        );
        for g in cfg.founders.unwrap_or(tmpl.founders) {
            if g.delay_years > 0.0 {
                sim.event(
                    EventKind::Info,
                    None,
                    format!(
                        "{} {} will be released in year {:.0}.",
                        g.count, g.species, g.delay_years
                    ),
                );
                sim.scheduled.push((g.delay_years as f64, g));
            } else {
                sim.add_founders(&g)?;
            }
        }
        sim.record_metrics();
        Ok(sim)
    }

    // ---- Calendar -------------------------------------------------------------

    pub fn minutes_per_year(&self) -> f64 {
        self.params.year_days * self.params.day_minutes as f64
    }

    pub fn year(&self) -> f64 {
        self.minutes as f64 / self.minutes_per_year()
    }

    pub fn day_of_year(&self) -> f64 {
        (self.minutes as f64 / self.params.day_minutes as f64) % self.params.year_days
    }

    pub fn mutation_rates(&self) -> MutationRates {
        let mu = self.params.effective_mutation_rate();
        MutationRates {
            functional: mu,
            neutral: mu * self.params.neutral_rate_factor,
            dmi: mu * self.params.dmi_rate_factor,
            large_effect_fraction: self.params.large_effect_fraction,
        }
    }

    pub fn patches_w(&self) -> usize {
        self.world.w.div_ceil(self.params.patch_size)
    }

    pub fn patches_h(&self) -> usize {
        self.world.h.div_ceil(self.params.patch_size)
    }

    pub fn patch_of_tile(&self, tile: usize) -> u32 {
        let ps = self.params.patch_size;
        let (x, y) = (tile % self.world.w, tile / self.world.w);
        ((y / ps) * self.patches_w() + x / ps) as u32
    }

    pub(crate) fn event(&mut self, kind: EventKind, species: Option<u32>, text: String) {
        let year = self.year();
        self.events.push(Event {
            year,
            kind,
            species,
            text,
        });
    }

    // ---- Organisms ---------------------------------------------------------------

    /// Builds an organism from a genome: genetic values + environmental deviation →
    /// phenotype → body at the given developmental age.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_animal(
        &mut self,
        species: u32,
        arch: u16,
        genome: Genome,
        sex: Sex,
        pos: (f32, f32),
        age_days: f32,
        energy_frac: f32,
        parents: (u64, u64),
        generation: u32,
        mutations: u32,
        rng: &mut Rng,
    ) -> Animal {
        let a = &self.archs[arch as usize];
        let mut latent = a.genetic_values(&self.alleles, &genome);
        for (i, v) in latent.iter_mut().enumerate() {
            *v += rng.normal_f32(0.0, a.env_sd[i]);
        }
        let pheno = Phenotype::from_latent(&latent);
        let adult = pheno.get(Trait::BodyMass);
        let d_adult = derive(&pheno, adult);
        let neonate = d_adult.neonate_kg.min(adult * 0.5);
        let mass = growth_mass(adult, neonate, age_days, d_adult.maturity_days);
        let d = derive(&pheno, mass);
        let id = self.next_id;
        self.next_id += 1;
        let birth_minute =
            self.minutes as i64 - (age_days as f64 * self.params.day_minutes as f64) as i64;
        Animal {
            id,
            species,
            arch,
            sex,
            x: pos.0,
            y: pos.1,
            target: None,
            birth_minute,
            mother: parents.0,
            father: parents.1,
            generation,
            genome,
            latent,
            pheno,
            d,
            mass,
            neonate_mass: neonate,
            energy: d.fat_cap_kj * energy_frac,
            water_deficit: 0.0,
            heat_damage: 0.0,
            cold_damage: 0.0,
            dev_days: age_days,
            action: crate::animal::Action::Rest,
            fear: 0.0,
            flee_from: (0.0, 0.0),
            hunt_cooldown: 0,
            disperse_hours: 0,
            disperse_dir: 0,
            pregnancy: None,
            last_birth_minute: 0,
            offspring: 0,
            matings: 0,
            mutations,
            alive: true,
            cause: None,
        }
    }

    /// Tiles suitable for releasing founders, around the best spot (or a given region).
    /// Release tiles with weights proportional to the food each can supply.
    fn founder_tiles(
        &self,
        region: Option<(f32, f32, f32)>,
        means: &[f32; N_TRAITS],
        count: u32,
    ) -> Vec<(usize, f64)> {
        let w = &self.world;
        let herbivore = means[Trait::Diet as usize] < 0.5;
        let pheno = Phenotype(*means);
        let d = derive(&pheno, means[Trait::BodyMass as usize]);
        // Thermal comfort in the warmest and coldest months (with behavioural buffering).
        let comfort = |i: usize| -> f32 {
            let mut penalty = 0.0;
            for doy in [15.0, 196.0] {
                let t = w.temp_day(i, doy);
                for ta in [t - 6.0, t + 6.0] {
                    let (heat, th) = crate::population::behavioural_thermal(&d, ta);
                    penalty += 3.0 * th.heat_stress + th.cold_stress + 0.5 * th.extra_w / heat;
                }
            }
            (-penalty).exp()
        };
        let score = |i: usize| -> f32 {
            if !w.is_land(i) || w.water[i] == crate::world::Water::Lake {
                return 0.0;
            }
            let comfort = if herbivore {
                comfort(i)
            } else {
                (comfort(i) * 4.0).min(1.0)
            };
            let t = w.temp_mean(i);
            if !(-6.0..=29.0).contains(&t) || w.water_dist[i] == u16::MAX {
                return 0.0;
            }
            let q = crate::physiology::roughage_quality(means[Trait::BodyMass as usize]);
            let food = if herbivore {
                w.forage_max[i] + q * w.roughage_max[i]
            } else {
                // Carnivores care about prey (weighted below), not plants.
                1.0
            };
            food / (1.0 + w.water_dist[i] as f32) * comfort
        };
        let n = w.w * w.h;
        let in_region = |i: usize| -> bool {
            match region {
                None => true,
                Some((fx, fy, r)) => {
                    let (x, y) = ((i % w.w) as f32 + 0.5, (i / w.w) as f32 + 0.5);
                    let mut dx = (x - fx * w.w as f32).abs();
                    dx = dx.min(w.w as f32 - dx);
                    let dy = y - fy * w.h as f32;
                    (dx * dx + dy * dy).sqrt() <= r * w.w as f32
                }
            }
        };
        // Predators are released where the most prey live.
        let mut prey = vec![0f32; if herbivore { 0 } else { n }];
        if !herbivore {
            // Prey biomass (kg) per tile.
            for a in &self.animals {
                if a.d.meat_eff < 0.3 {
                    prey[w.tile_of(a.x, a.y)] += a.mass;
                }
            }
            let ps = self.params.patch_size;
            let pw = self.patches_w();
            for dm in &self.demes {
                let arch = &self.archs[dm.arch as usize];
                let m = Phenotype::from_latent(&dm.mean_latent(arch, &self.alleles));
                if derive(&m, m.get(Trait::BodyMass)).meat_eff >= 0.3 {
                    continue;
                }
                let (px, py) = (dm.patch as usize % pw, dm.patch as usize / pw);
                let tiles: Vec<usize> = (py * ps..((py + 1) * ps).min(w.h))
                    .flat_map(|y| (px * ps..((px + 1) * ps).min(w.w)).map(move |x| y * w.w + x))
                    .filter(|&i| w.is_land(i))
                    .collect();
                let kg = dm.total() as f32 * m.get(Trait::BodyMass) / tiles.len().max(1) as f32;
                for t in tiles {
                    prey[t] += kg;
                }
            }
        }
        let any_prey = prey.iter().any(|&p| p > 0.0);
        let mut best = (0.0f32, usize::MAX);
        for i in 0..n {
            if !in_region(i) {
                continue;
            }
            let mut s = score(i);
            if !herbivore && any_prey {
                let (x, y) = ((i % w.w) as i64, (i / w.w) as i64);
                let mut around = 0.0;
                for dy in -2..=2i64 {
                    for dx in -2..=2i64 {
                        let yy = y + dy;
                        if (0..w.h as i64).contains(&yy) {
                            around +=
                                prey[yy as usize * w.w + (x + dx).rem_euclid(w.w as i64) as usize];
                        }
                    }
                }
                s *= around;
            }
            if s > best.0 {
                best = (s, i);
            }
        }
        if best.1 == usize::MAX && !herbivore {
            // No prey in comfortable habitat: fall back to habitat alone.
            for i in 0..n {
                let s = score(i);
                if in_region(i) && s > best.0 {
                    best = (s, i);
                }
            }
        }
        if best.1 == usize::MAX {
            return vec![];
        }
        // Compare neighbours against the raw habitat score, not the prey-weighted one.
        let best = (score(best.1), best.1);
        let (bx, by) = ((best.1 % w.w) as i64, (best.1 / w.w) as i64);
        // Grow the release area until it can feed the founders (about 2× their need).
        let need_kj = (d.bmr_w * 1.7 * 86.4 * 365.25).max(1.0);
        let kj_g = self.params.forage_kj_per_g as f32;
        let area = self.params.habitat_m2_per_tile as f32;
        let capacity = |i: usize| -> f32 {
            if herbivore {
                let q = crate::physiology::roughage_quality(means[Trait::BodyMass as usize]);
                let food =
                    0.5 * (w.forage_production(i) + q * w.roughage_production(i)) * area * kj_g;
                food * d.plant_eff / need_kj
            } else if prey.is_empty() {
                0.2
            } else {
                // Sustainable offtake ≈ half the prey biomass per year, 60% edible at 7 kJ/g.
                prey[i] * 0.5 * 600.0 * 7.0 * d.meat_eff / need_kj
            }
        };
        let max_rad = if region.is_some() { 4 } else { 16 };
        let mut tiles = vec![];
        for rad in 2..=max_rad {
            tiles.clear();
            let mut cap = 0.0;
            for dy in -rad..=rad {
                for dx in -rad..=rad {
                    let y = by + dy;
                    if y < 0 || y >= w.h as i64 {
                        continue;
                    }
                    let x = (bx + dx).rem_euclid(w.w as i64);
                    let i = y as usize * w.w + x as usize;
                    if score(i) > 0.15 * best.0 && in_region(i) {
                        tiles.push((i, capacity(i).max(1e-6) as f64));
                        cap += capacity(i);
                    }
                }
            }
            if cap >= if herbivore { 2.0 } else { 3.0 } * count as f32 {
                break;
            }
        }
        tiles
    }

    /// Releases a founder population (PRD FR-S1/S4). Returns the species id.
    pub fn add_founders(&mut self, g: &FounderGroup) -> Result<u32, String> {
        let tmpl = species_lib::find(&g.species)
            .ok_or_else(|| format!("unknown species '{}'", g.species))?;
        if g.count == 0 || g.count > 20_000 {
            return Err("founder count must be 1..20000".into());
        }
        let arch_id = self.archs.len() as u16;
        let mut rng = Rng::keyed(self.seed, self.minutes, arch_id as u64, stream::FOUNDERS);
        let spec = tmpl.founder_spec(g.diversity);
        let arch = GenomeArch::create(arch_id, &spec, &mut self.alleles, &mut rng);
        self.archs.push(arch);
        let year = self.year();
        let sp = self.species.add(
            arch_id,
            None,
            tmpl.name.to_string(),
            year,
            tmpl.color,
            self.seed,
        );
        let tiles = self.founder_tiles(g.region, &tmpl.phenotype_means(), g.count);
        let weights: Vec<f64> = tiles.iter().map(|t| t.1).collect();
        if tiles.is_empty() {
            return Err(format!("no suitable habitat found for {}", tmpl.name));
        }
        let mut new = Vec::with_capacity(g.count as usize);
        for k in 0..g.count {
            let genome = self.archs[arch_id as usize].founder_genome(&mut rng);
            let t = tiles[rng.weighted(&weights)].0;
            let pos = (
                (t % self.world.w) as f32 + rng.f32(),
                (t / self.world.w) as f32 + rng.f32(),
            );
            let sex = if k % 2 == 0 { Sex::Female } else { Sex::Male };
            let probe = derive(
                &Phenotype::from_latent(&self.archs[arch_id as usize].base),
                tmpl.phenotype_means()[0],
            );
            let age = probe.maturity_days * rng.range_f32(1.0, 2.5);
            let a = self.build_animal(
                sp,
                arch_id,
                genome,
                sex,
                pos,
                age,
                0.8,
                (0, 0),
                0,
                0,
                &mut rng,
            );
            new.push(a);
        }
        match self.mode {
            Mode::Individual => self.animals.extend(new),
            Mode::Population => self.absorb_into_demes(new),
        }
        self.event(
            EventKind::Founders,
            Some(sp),
            format!(
                "{} founders of {} ({}) released.",
                g.count,
                tmpl.name,
                self.species.get(sp).name
            ),
        );
        Ok(sp)
    }

    pub(crate) fn kill(&mut self, i: usize, cause: Cause) {
        let a = &mut self.animals[i];
        if a.alive {
            a.alive = false;
            a.cause = Some(cause);
            self.deaths[cause as usize] += 1;
        }
    }

    // ---- Stepping -----------------------------------------------------------------

    /// One engine step: an hour in Individual mode, a year in Population mode.
    pub fn step(&mut self) {
        match self.mode {
            Mode::Individual => self.tick_individual(),
            Mode::Population => self.step_population(),
        }
        self.after_step();
    }

    /// Simulated minutes advanced by one step in the current mode.
    pub fn step_minutes(&self) -> u64 {
        match self.mode {
            Mode::Individual => self.params.tick_minutes,
            Mode::Population => self.minutes_per_year() as u64,
        }
    }

    /// Advances by `steps` steps or until nothing is alive.
    pub fn run_steps(&mut self, steps: u64) {
        for _ in 0..steps {
            self.step();
        }
    }

    /// Advances by at least `years` simulated years.
    pub fn run_years(&mut self, years: f64) {
        let target = self.minutes + (years * self.minutes_per_year()) as u64;
        while self.minutes < target {
            self.step();
        }
    }

    fn after_step(&mut self) {
        let y = self.year();
        let mut expired = vec![];
        self.climate_effects.retain(|&(until, d)| {
            if y >= until {
                expired.push(d);
                false
            } else {
                true
            }
        });
        let due: Vec<FounderGroup> = {
            let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.scheduled)
                .into_iter()
                .partition(|(at, _)| y >= *at);
            self.scheduled = later;
            now.into_iter().map(|(_, g)| g).collect()
        };
        for g in due {
            if let Err(e) = self.add_founders(&g) {
                self.event(
                    EventKind::Info,
                    None,
                    format!("Could not release {}: {e}", g.species),
                );
            }
        }
        for d in expired {
            self.world.shift_climate(d);
            self.event(
                EventKind::Intervention,
                None,
                format!("Climate recovers ({:+.1} °C).", d),
            );
        }
        let interval = match self.mode {
            Mode::Individual => 1.0 / 24.0,
            Mode::Population => 1.0,
        };
        if y - self.last_metric_year >= interval - 1e-9 {
            self.record_metrics();
        }
        if y - self.last_species_check >= self.params.species_check_years && self.frozen.is_empty()
        {
            self.last_species_check = y;
            self.species_check();
        }
        if self.mode == Mode::Individual && self.animals.len() > self.params.individual_cap {
            let n = self.animals.len();
            if self.set_mode(Mode::Population).is_ok() {
                self.event(
                    EventKind::Mode,
                    None,
                    format!(
                        "Population reached {n} animals — switched to Population mode to keep up."
                    ),
                );
            }
        }
        if !self.archs.is_empty() && self.alleles.len() > 400_000 {
            self.compact_alleles();
        }
    }

    /// Switches fidelity mode (TRD §5.13). P → I fails if too many animals would result;
    /// use [`Simulation::focus_region`] to watch part of a large world instead.
    pub fn set_mode(&mut self, target: Mode) -> Result<(), String> {
        if target == self.mode {
            return Ok(());
        }
        match target {
            Mode::Population => {
                // Restore frozen demes first so animals that wandered out of the focus
                // region merge into them instead of forming duplicate (species, patch) demes.
                let frozen = std::mem::take(&mut self.frozen);
                self.demes.extend(frozen);
                let animals = std::mem::take(&mut self.animals);
                self.absorb_into_demes(animals);
                self.demes.sort_by_key(|d| (d.species, d.patch));
                self.focus = None;
                self.mode = Mode::Population;
            }
            Mode::Individual => {
                let total: u64 = self.demes.iter().map(|d| d.total() as u64).sum();
                if total as usize > self.params.individual_cap {
                    return Err(format!(
                        "{total} organisms is above the Individual-mode cap of {}. Pick a focus region to watch part of the world.",
                        self.params.individual_cap
                    ));
                }
                self.materialize_demes();
                self.mode = Mode::Individual;
            }
        }
        let label = match target {
            Mode::Individual => "Individual",
            Mode::Population => "Population",
        };
        self.event(EventKind::Mode, None, format!("Switched to {label} mode."));
        Ok(())
    }

    /// Simulates only the demes in a rectangle (tile coordinates) as individuals; the rest
    /// of the world is held as demes until returning to Population mode (TRD TR-T4).
    pub fn focus_region(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) -> Result<u64, String> {
        if self.mode == Mode::Individual {
            self.set_mode(Mode::Population)?;
        }
        let ps = self.params.patch_size as u32;
        let pw = self.patches_w() as u32;
        let (px0, py0, px1, py1) = (x0 / ps, y0 / ps, x1.max(x0) / ps, y1.max(y0) / ps);
        let inside = |p: u32| {
            let (px, py) = (p % pw, p / pw);
            (px0..=px1).contains(&px) && (py0..=py1).contains(&py)
        };
        let n: u64 = self
            .demes
            .iter()
            .filter(|d| inside(d.patch))
            .map(|d| d.total() as u64)
            .sum();
        if n as usize > self.params.individual_cap {
            return Err(format!(
                "{n} organisms live in that area — above the cap of {}. Choose a smaller area.",
                self.params.individual_cap
            ));
        }
        let (keep, hold): (Vec<Deme>, Vec<Deme>) = std::mem::take(&mut self.demes)
            .into_iter()
            .partition(|d| inside(d.patch));
        self.demes = keep;
        self.frozen = hold;
        self.materialize_demes();
        self.mode = Mode::Individual;
        self.focus = Some((
            px0 * ps,
            py0 * ps,
            ((px1 + 1) * ps).min(self.world.w as u32),
            ((py1 + 1) * ps).min(self.world.h as u32),
        ));
        self.event(
            EventKind::Mode,
            None,
            format!("Focused on a region: {n} organisms simulated individually; the rest of the world is held."),
        );
        Ok(n)
    }

    // ---- Species, metrics ---------------------------------------------------------------

    pub fn species_counts(&self) -> BTreeMap<u32, u32> {
        let mut c = BTreeMap::new();
        match self.mode {
            Mode::Individual => {
                for a in &self.animals {
                    *c.entry(a.species).or_insert(0) += 1;
                }
                for d in &self.frozen {
                    *c.entry(d.species).or_insert(0) += d.total();
                }
            }
            Mode::Population => {
                for d in &self.demes {
                    *c.entry(d.species).or_insert(0) += d.total();
                }
            }
        }
        c
    }

    pub fn population(&self) -> u64 {
        match self.mode {
            Mode::Individual => {
                self.animals.len() as u64
                    + self.frozen.iter().map(|d| d.total() as u64).sum::<u64>()
            }
            Mode::Population => self.demes.iter().map(|d| d.total() as u64).sum(),
        }
    }

    fn species_check(&mut self) {
        let (pops, groups) = match self.mode {
            Mode::Individual => self.pops_from_animals(),
            Mode::Population => self.pops_from_demes(),
        };
        let pw = self.patches_w();
        let seed = self.seed;
        let params = self.params.clone();
        let (world, ps) = (&self.world, self.params.patch_size);
        // Two patches are connected if they are the same or share a land border.
        let connected = |a: u32, b: u32| -> bool {
            if a == b {
                return true;
            }
            let (ax, ay) = ((a as usize % pw) as i64, (a as usize / pw) as i64);
            let (bx, by) = ((b as usize % pw) as i64, (b as usize / pw) as i64);
            let mut dx = bx - ax;
            if dx > pw as i64 / 2 {
                dx -= pw as i64;
            } else if dx < -(pw as i64) / 2 {
                dx += pw as i64;
            }
            let dy = by - ay;
            dx.abs() <= 1
                && dy.abs() <= 1
                && crate::population::border_connectivity(
                    world,
                    ps,
                    ax as usize,
                    ay as usize,
                    dx,
                    dy,
                ) > 0.0
        };
        let splits =
            crate::species::detect_splits(&pops, &mut self.species, seed, &params, &connected);
        for s in splits {
            let parent = self.species.get(s.parent).clone();
            let members: std::collections::BTreeSet<usize> = s.members.iter().copied().collect();
            // Where the daughter lives relative to the parent decides its common name.
            let (mut my, mut oy, mut mx, mut ox, mut mn, mut on) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for p in &pops {
                if p.species != s.parent {
                    continue;
                }
                let (px, py) = (
                    (p.patch as usize % pw) as f64,
                    (p.patch as usize / pw) as f64,
                );
                if members.contains(&p.key) {
                    my += py * p.n as f64;
                    mx += px * p.n as f64;
                    mn += p.n as f64;
                } else {
                    oy += py * p.n as f64;
                    ox += px * p.n as f64;
                    on += p.n as f64;
                }
            }
            let (dy, dx) = (
                my / mn.max(1.0) - oy / on.max(1.0),
                mx / mn.max(1.0) - ox / on.max(1.0),
            );
            let prefix = if dy.abs() >= dx.abs() {
                if dy < 0.0 {
                    "Northern"
                } else {
                    "Southern"
                }
            } else if dx > 0.0 {
                "Eastern"
            } else {
                "Western"
            };
            let base = parent
                .common_name
                .trim_start_matches("Northern ")
                .trim_start_matches("Southern ")
                .trim_start_matches("Eastern ")
                .trim_start_matches("Western ")
                .to_string();
            let mut common = format!("{prefix} {}", base.to_lowercase());
            let dupes = self
                .species
                .list
                .iter()
                .filter(|x| x.common_name.starts_with(&common))
                .count();
            if dupes > 0 {
                common = format!("{common} {}", dupes + 1);
            }
            let color = shift_hue(parent.color, self.species.list.len() as u32);
            let year = self.year();
            let id = self
                .species
                .add(parent.arch, Some(s.parent), common, year, color, self.seed);
            self.species.get_mut(id).split_cause = Some(s.cause.clone());
            match self.mode {
                Mode::Individual => {
                    let patches: Vec<u32> = self
                        .animals
                        .iter()
                        .map(|a| self.patch_of_tile_xy(a.x, a.y))
                        .collect();
                    for (a, p) in self.animals.iter_mut().zip(patches) {
                        if a.species == s.parent && members.contains(&groups[&(a.species, p)]) {
                            a.species = id;
                        }
                    }
                }
                Mode::Population => {
                    for (k, d) in self.demes.iter_mut().enumerate() {
                        if d.species == s.parent && members.contains(&k) {
                            d.species = id;
                        }
                    }
                }
            }
            let name = self.species.get(id).name.clone();
            self.event(
                EventKind::Speciation,
                Some(id),
                format!(
                    "New species: {} ({}) split from {} — {}.",
                    self.species.get(id).common_name,
                    name,
                    parent.name,
                    s.cause
                ),
            );
        }
    }

    pub(crate) fn patch_of_tile_xy(&self, x: f32, y: f32) -> u32 {
        self.patch_of_tile(self.world.tile_of(x, y))
    }

    fn pops_from_animals(&self) -> (Vec<crate::species::PopSummary>, BTreeMap<(u32, u32), usize>) {
        let mut groups: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
        for (i, a) in self.animals.iter().enumerate() {
            groups
                .entry((a.species, self.patch_of_tile_xy(a.x, a.y)))
                .or_default()
                .push(i);
        }
        let mut pops = vec![];
        let mut keys = BTreeMap::new();
        for (k, ((sp, patch), idx)) in groups.into_iter().enumerate() {
            let arch = self.animals[idx[0]].arch;
            let n = idx.len() as f32;
            let mean = |t: Trait| {
                idx.iter()
                    .map(|&i| self.animals[i].pheno.get(t))
                    .sum::<f32>()
                    / n
            };
            let dmi = crate::species::dmi_freqs(
                &self.alleles,
                &self.archs[arch as usize].dmi_loci,
                idx.iter().map(|&i| &self.animals[i].genome),
            );
            pops.push(crate::species::PopSummary {
                key: k,
                species: sp,
                arch,
                patch,
                n: idx.len() as u32,
                dmi,
                ornament: mean(Trait::Ornament),
                preference: mean(Trait::Preference),
                choosiness: mean(Trait::Choosiness),
                breeding_day: mean(Trait::BreedingDay),
            });
            keys.insert((sp, patch), k);
        }
        (pops, keys)
    }

    pub fn record_metrics(&mut self) {
        let year = self.year();
        self.last_metric_year = year;
        let mut stats: BTreeMap<u32, TraitStats> = BTreeMap::new();
        match self.mode {
            Mode::Individual => {
                for a in &self.animals {
                    let het = self.archs[a.arch as usize].neutral_heterozygosity(&a.genome);
                    stats.entry(a.species).or_default().add(&a.pheno.0, het, 1);
                }
                for d in &self.frozen {
                    let (mean, het) = d.mean_phenotype(&self.archs[d.arch as usize], &self.alleles);
                    stats
                        .entry(d.species)
                        .or_default()
                        .add(&mean, het, d.total());
                }
            }
            Mode::Population => {
                for d in &self.demes {
                    let (mean, het) = d.mean_phenotype(&self.archs[d.arch as usize], &self.alleles);
                    stats
                        .entry(d.species)
                        .or_default()
                        .add(&mean, het, d.total());
                }
            }
        }
        let ids: Vec<u32> = self
            .species
            .list
            .iter()
            .filter(|s| s.extinct_year.is_none())
            .map(|s| s.id)
            .collect();
        for id in ids {
            let st = stats.remove(&id).unwrap_or_default();
            let sample = st.sample(year);
            let n = sample.n;
            if n == 0 {
                let info = self.species.get_mut(id);
                info.extinct_year = Some(year);
                let (cn, nm) = (info.common_name.clone(), info.name.clone());
                self.event(
                    EventKind::Extinction,
                    Some(id),
                    format!("{cn} ({nm}) went extinct."),
                );
                self.metrics.push(id, sample);
                continue;
            }
            self.check_trait_shifts(id, &sample.mean, &sample.sd, year);
            let info = self.species.get_mut(id);
            if info.peak_n >= 100 && (n as f32) < 0.35 * info.peak_n as f32 {
                let (cn, peak) = (info.common_name.clone(), info.peak_n);
                info.peak_n = n;
                self.event(
                    EventKind::Crash,
                    Some(id),
                    format!("Population crash: {cn} fell from {peak} to {n}."),
                );
            }
            let info = self.species.get_mut(id);
            info.peak_n = info.peak_n.max(n);
            info.last_n = n;
            self.metrics.push(id, sample);
        }
    }

    fn check_trait_shifts(&mut self, id: u32, mean: &[f32], sd: &[f32], year: f64) {
        const WATCH: [Trait; 12] = [
            Trait::BodyMass,
            Trait::Insulation,
            Trait::Appendage,
            Trait::LegLength,
            Trait::Diet,
            Trait::CoatLuminance,
            Trait::Brain,
            Trait::LitterSize,
            Trait::Pace,
            Trait::Ornament,
            Trait::HeatTolerance,
            Trait::Kidney,
        ];
        let info = self.species.get_mut(id);
        let (Some(om), Some(osd)) = (info.origin_mean, info.origin_sd) else {
            let mut m = [0f32; N_TRAITS];
            let mut s = [0f32; N_TRAITS];
            m.copy_from_slice(mean);
            for i in 0..N_TRAITS {
                s[i] = sd[i].max(0.02 * mean[i].abs()).max(1e-3);
            }
            info.origin_mean = Some(m);
            info.origin_sd = Some(s);
            return;
        };
        let mut msgs = vec![];
        for t in WATCH {
            let i = t as usize;
            let z = (mean[i] - om[i]) / osd[i];
            let level = (z.abs() / 3.0).floor().min(250.0) as u8;
            if level > info.announced_shift[i] {
                info.announced_shift[i] = level;
                let pct = 100.0 * (mean[i] - om[i]) / om[i].abs().max(1e-6);
                msgs.push(format!(
                    "{}: {} {} by {:.0}% ({:.1} SD) since the species arose {:.0} years ago.",
                    info.common_name,
                    t.info().label.to_lowercase(),
                    if z > 0.0 { "increased" } else { "decreased" },
                    pct.abs(),
                    z.abs(),
                    year - info.origin_year
                ));
            }
        }
        for m in msgs {
            self.event(EventKind::Adaptation, Some(id), m);
        }
    }

    // ---- Interventions ----------------------------------------------------------------

    pub fn apply(&mut self, iv: Intervention) -> Result<(), String> {
        match iv {
            Intervention::ClimateShift { delta_c } => {
                if !(-15.0..=15.0).contains(&delta_c) {
                    return Err("climate shift must be within ±15 °C".into());
                }
                self.world.shift_climate(delta_c);
                self.event(
                    EventKind::Intervention,
                    None,
                    format!("Climate shift: {:+.1} °C global temperature.", delta_c),
                );
            }
            Intervention::Meteor { fx, fy, radius } => {
                let (cx, cy) = (fx * self.world.w as f32, fy * self.world.h as f32);
                let r = radius.clamp(0.01, 0.5) * self.world.w as f32;
                let w = self.world.w as f32;
                let dist = |x: f32, y: f32| {
                    let mut dx = (x - cx).abs();
                    dx = dx.min(w - dx);
                    (dx * dx + (y - cy).powi(2)).sqrt()
                };
                let mut killed = 0u64;
                for i in 0..self.animals.len() {
                    let (x, y) = (self.animals[i].x, self.animals[i].y);
                    if dist(x, y) < r {
                        self.kill(i, Cause::Catastrophe);
                        killed += 1;
                    }
                }
                self.animals.retain(|a| a.alive);
                let ps = self.params.patch_size as f32;
                let pw = self.patches_w();
                let mut rng = Rng::keyed(self.seed, self.minutes, 0, stream::EVENTS);
                for d in self.demes.iter_mut().chain(self.frozen.iter_mut()) {
                    let (px, py) = (
                        (d.patch as usize % pw) as f32,
                        (d.patch as usize / pw) as f32,
                    );
                    if dist((px + 0.5) * ps, (py + 0.5) * ps) < r {
                        killed += d.cull(0.95, &mut rng);
                    }
                }
                self.demes.retain(|d| d.total() > 0);
                self.frozen.retain(|d| d.total() > 0);
                for i in 0..self.world.w * self.world.h {
                    let (x, y) = (
                        (i % self.world.w) as f32 + 0.5,
                        (i / self.world.w) as f32 + 0.5,
                    );
                    if dist(x, y) < r {
                        self.world.forage[i] = 0.0;
                        self.world.roughage[i] = 0.0;
                    }
                }
                self.world.shift_climate(-4.0);
                self.climate_effects.push((self.year() + 8.0, 4.0));
                self.event(
                    EventKind::Intervention,
                    None,
                    format!("Meteor impact! {killed} organisms killed; impact winter (−4 °C for ~8 years)."),
                );
            }
            Intervention::AddFounders { group } => {
                self.add_founders(&group)?;
            }
            Intervention::Plague { species, mortality } => {
                let name = self
                    .species
                    .list
                    .get(species as usize)
                    .map(|s| s.common_name.clone())
                    .ok_or_else(|| format!("unknown species {species}"))?;
                let m = mortality.clamp(0.0, 1.0) as f64;
                let mut rng = Rng::keyed(self.seed, self.minutes, species as u64, stream::EVENTS);
                let mut killed = 0;
                for i in 0..self.animals.len() {
                    if self.animals[i].species == species && rng.chance(m) {
                        self.kill(i, Cause::Catastrophe);
                        killed += 1;
                    }
                }
                self.animals.retain(|a| a.alive);
                for d in self
                    .demes
                    .iter_mut()
                    .chain(self.frozen.iter_mut())
                    .filter(|d| d.species == species)
                {
                    killed += d.cull(m, &mut rng);
                }
                self.demes.retain(|d| d.total() > 0);
                self.frozen.retain(|d| d.total() > 0);
                self.event(
                    EventKind::Intervention,
                    Some(species),
                    format!("Plague struck {name}: {killed} died."),
                );
            }
        }
        Ok(())
    }

    // ---- Allele table maintenance ---------------------------------------------------------

    /// Drops alleles no living genome or deme references and renumbers the rest.
    pub fn compact_alleles(&mut self) {
        let n = self.alleles.len();
        let mut live = vec![false; n];
        for a in &self.archs {
            for pool in &a.founder_alleles {
                for &x in pool {
                    live[x as usize] = true;
                }
            }
        }
        let mark_genome = |g: &Genome, live: &mut Vec<bool>| {
            for h in &g.hap {
                for &x in h {
                    live[x as usize] = true;
                }
            }
        };
        for a in &self.animals {
            mark_genome(&a.genome, &mut live);
            if let Some(p) = &a.pregnancy {
                for (e, _) in &p.embryos {
                    mark_genome(e, &mut live);
                }
            }
        }
        for d in self.demes.iter().chain(&self.frozen) {
            d.mark_live(&mut live);
        }
        let mut remap = vec![u32::MAX; n];
        let mut kept = Vec::new();
        for (i, a) in self.alleles.alleles.iter().enumerate() {
            if live[i] {
                remap[i] = kept.len() as u32;
                kept.push(a.clone());
            }
        }
        for a in kept.iter_mut() {
            // Parent links point to the nearest surviving ancestor, if any.
            let mut p = a.parent;
            while p != crate::genetics::NO_PARENT && remap[p as usize] == u32::MAX {
                p = self.alleles.alleles[p as usize].parent;
            }
            a.parent = if p == crate::genetics::NO_PARENT {
                p
            } else {
                remap[p as usize]
            };
        }
        let rg = |g: &mut Genome| {
            for h in g.hap.iter_mut() {
                for x in h.iter_mut() {
                    *x = remap[*x as usize];
                }
            }
        };
        for a in self.archs.iter_mut() {
            for pool in a.founder_alleles.iter_mut() {
                for x in pool.iter_mut() {
                    *x = remap[*x as usize];
                }
            }
        }
        for a in self.animals.iter_mut() {
            rg(&mut a.genome);
            if let Some(p) = a.pregnancy.as_mut() {
                for (e, _) in p.embryos.iter_mut() {
                    rg(e);
                }
            }
        }
        for d in self.demes.iter_mut().chain(self.frozen.iter_mut()) {
            d.remap(&remap);
        }
        self.alleles.alleles = kept;
    }

    // ---- Persistence ----------------------------------------------------------------------

    pub fn save_bytes(&self) -> Result<Vec<u8>, String> {
        let mut out = SAVE_MAGIC.to_vec();
        out.extend(bincode::serialize(self).map_err(|e| e.to_string())?);
        Ok(out)
    }

    pub fn load_bytes(bytes: &[u8]) -> Result<Simulation, String> {
        if bytes.len() < 4 || &bytes[..4] != SAVE_MAGIC {
            return Err("not an Evolutions save file".into());
        }
        let sim: Simulation =
            bincode::deserialize(&bytes[4..]).map_err(|e| format!("corrupt save: {e}"))?;
        if sim.model_version > MODEL_VERSION {
            return Err("save was made by a newer engine".into());
        }
        Ok(sim)
    }

    /// Deterministic hash of the simulation state (TRD TR-D7).
    pub fn state_hash(&self) -> u64 {
        let bytes = bincode::serialize(self).unwrap_or_default();
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }
}

/// Rotates an RGB colour's hue so daughter species get related but distinct colours.
pub fn shift_hue(c: [u8; 3], k: u32) -> [u8; 3] {
    let (r, g, b) = (
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    let mut h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    let s = if d == 0.0 {
        0.5
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6)
    };
    let s = s.max(0.45);
    let l = l.clamp(0.35, 0.65);
    h = (h + 137.5 * k as f32) % 360.0;
    let c2 = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c2 * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c2 / 2.0;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c2, x, 0.0),
        1 => (x, c2, 0.0),
        2 => (0.0, c2, x),
        3 => (0.0, x, c2),
        4 => (x, 0.0, c2),
        _ => (c2, 0.0, x),
    };
    [
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    ]
}
