//! Individual mode (Mode I): the hourly tick pipeline (System Design §4.1).
//!
//! Each animal perceives its surroundings, chooses an action by utility
//! (weights come from heritable traits), moves, interacts (eats, drinks, hunts,
//! mates) and pays the physiological costs of being alive. Animals are processed
//! in a fixed order with per-animal counter-keyed random streams, so ticks are
//! deterministic.

use crate::animal::{Action, Animal, Cause, Pregnancy, Sex};
use crate::genetics::{dmi_viability, Genome};
use crate::physiology::{
    camouflage, derive, gompertz_hazard, growth_mass, locomotion_kj, mate_acceptance,
    roughage_quality, thermal, year_distance, EVAP_J_PER_G, TISSUE_KJ_PER_KG,
};
use crate::rng::{stream, Rng};
use crate::sim::Simulation;
use crate::traits::Trait;
use crate::world::{World, DIRS, HERE, NOWHERE};

/// Uniform grid of animal indices, one cell per tile.
#[derive(Default, Clone)]
pub struct Grid {
    pub start: Vec<u32>,
    pub items: Vec<u32>,
    /// Number of potential predators per cell.
    pub predators: Vec<u16>,
    /// Predator indices per cell (CSR, like `start`/`items`).
    pub pred_start: Vec<u32>,
    pub pred_items: Vec<u32>,
    /// Scratch buffers reused between builds: each animal's tile, and CSR fill cursors.
    tiles: Vec<u32>,
    fill: Vec<u32>,
}

impl Grid {
    fn build(&mut self, world: &World, animals: &[Animal]) {
        let n = world.w * world.h;
        self.start.clear();
        self.start.resize(n + 1, 0);
        self.predators.clear();
        self.predators.resize(n, 0);
        self.tiles.clear();
        self.tiles
            .extend(animals.iter().map(|a| world.tile_of(a.x, a.y) as u32));
        for (a, &t) in animals.iter().zip(&self.tiles) {
            let t = t as usize;
            self.start[t + 1] += 1;
            if a.d.meat_eff > 0.3 {
                self.predators[t] = self.predators[t].saturating_add(1);
            }
        }
        for i in 0..n {
            self.start[i + 1] += self.start[i];
        }
        self.fill.clear();
        self.fill.extend_from_slice(&self.start[..n]);
        self.items.clear();
        self.items.resize(animals.len(), 0);
        for (i, &t) in self.tiles.iter().enumerate() {
            let f = &mut self.fill[t as usize];
            self.items[*f as usize] = i as u32;
            *f += 1;
        }
        self.pred_start.clear();
        self.pred_start.resize(n + 1, 0);
        for i in 0..n {
            self.pred_start[i + 1] = self.pred_start[i] + self.predators[i] as u32;
        }
        self.fill.clear();
        self.fill.extend_from_slice(&self.pred_start[..n]);
        self.pred_items.clear();
        self.pred_items.resize(self.pred_start[n] as usize, 0);
        for (i, (a, &t)) in animals.iter().zip(&self.tiles).enumerate() {
            let t = t as usize;
            if a.d.meat_eff > 0.3 && self.predators[t] < u16::MAX {
                self.pred_items[self.fill[t] as usize] = i as u32;
                self.fill[t] += 1;
            }
        }
    }

    #[inline]
    fn preds(&self, t: usize) -> &[u32] {
        &self.pred_items[self.pred_start[t] as usize..self.pred_start[t + 1] as usize]
    }

    #[inline]
    fn cell(&self, t: usize) -> &[u32] {
        &self.items[self.start[t] as usize..self.start[t + 1] as usize]
    }
}

/// Hasher for u64 entity ids (ids are unique; a multiplicative mix is enough).
#[derive(Default, Clone, Copy)]
pub struct IdHasher(u64);
impl std::hash::Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = i.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}
pub type IdMap = std::collections::HashMap<u64, usize, std::hash::BuildHasherDefault<IdHasher>>;

/// Signed x-distance on the cylinder.
#[inline]
fn wrap_dx(dx: f32, w: f32) -> f32 {
    if dx > w / 2.0 {
        dx - w
    } else if dx < -w / 2.0 {
        dx + w
    } else {
        dx
    }
}

/// Largest search radius in tiles; perception reach is clamped to this.
const MAX_REACH: f32 = 2.0;
const MAX_NEAR: usize = (2 * MAX_REACH as usize + 1) * (2 * MAX_REACH as usize + 1);

/// Tile indices around a point, stored inline so hot loops do not allocate.
struct Near {
    tiles: [usize; MAX_NEAR],
    len: usize,
}

impl std::ops::Deref for Near {
    type Target = [usize];
    fn deref(&self) -> &[usize] {
        &self.tiles[..self.len]
    }
}

/// Tiles within `r` tiles of (x, y) (`r` is capped at `MAX_REACH`).
fn tiles_around(world: &World, x: f32, y: f32, r: f32) -> Near {
    let ri = r.min(MAX_REACH).ceil() as i64;
    let (tx, ty) = (x.floor() as i64, y.floor() as i64);
    let mut out = Near {
        tiles: [0; MAX_NEAR],
        len: 0,
    };
    let mut cols = [0usize; 2 * MAX_REACH as usize + 1];
    let nc = (2 * ri + 1) as usize;
    for (k, c) in cols[..nc].iter_mut().enumerate() {
        *c = (tx - ri + k as i64).rem_euclid(world.w as i64) as usize;
    }
    let (y0, y1) = ((ty - ri).max(0), (ty + ri).min(world.h as i64 - 1));
    for yy in y0..=y1 {
        let row = yy as usize * world.w;
        for &c in &cols[..nc] {
            out.tiles[out.len] = row + c;
            out.len += 1;
        }
    }
    out
}

struct Ctx {
    season: f32,
    diurnal: f32,
    now: u64,
    tick: u64,
    night: bool,
    day_minutes: u64,
    year_frac: f32,
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Simulation {
    pub(crate) fn tick_individual(&mut self) {
        let dm = self.params.day_minutes;
        let now = self.minutes;
        let day = now / dm;
        let doy = (day as f64) % self.params.year_days;
        let hour = ((now % dm) / 60) as f64;
        let ctx = Ctx {
            season: (std::f64::consts::TAU * (doy - 200.0) / 365.25).cos() as f32,
            diurnal: (std::f64::consts::TAU * (hour - 15.0) / 24.0).cos() as f32,
            now,
            tick: now / self.params.tick_minutes,
            night: !(6.0..20.0).contains(&hour),
            day_minutes: dm,
            year_frac: (doy / self.params.year_days) as f32,
        };
        if day != self.last_day {
            self.last_day = day;
            let decay = self.params.carcass_decay_per_day as f32;
            self.world.grow_vegetation_day(doy, decay);
            self.daily_update(&ctx);
        }
        self.grid.build(&self.world, &self.animals);
        self.index.clear();
        for (i, a) in self.animals.iter().enumerate() {
            self.index.insert(a.id, i);
        }
        for i in 0..self.animals.len() {
            if self.animals[i].alive {
                self.act(i, &ctx);
            }
        }
        self.commit();
        self.minutes += self.params.tick_minutes;
    }

    fn commit(&mut self) {
        self.animals.retain(|a| a.alive);
        let born = std::mem::take(&mut self.pending);
        self.births += born.len() as u64;
        self.animals.extend(born);
    }

    /// Once per simulated day: ageing, growth, births, dispersal onset.
    fn daily_update(&mut self, ctx: &Ctx) {
        let bg = self.params.background_mortality as f32;
        for i in 0..self.animals.len() {
            if !self.animals[i].alive {
                continue;
            }
            let mut rng = Rng::keyed(self.seed, ctx.tick, self.animals[i].id, stream::MORTALITY);
            let a = &mut self.animals[i];
            let age_y = a.age_days(ctx.now, ctx.day_minutes) / 365.25;
            let intrinsic = gompertz_hazard(&a.d, age_y, 0.0);
            let hazard = intrinsic + bg;
            if rng.chance(1.0 - (-(hazard as f64) / 365.25).exp()) {
                let cause = if rng.f32() * hazard < intrinsic {
                    Cause::OldAge
                } else {
                    Cause::Accident
                };
                self.kill(i, cause);
                continue;
            }
            // Growth (West et al. 2001), slowed when undernourished (plasticity).
            let a = &mut self.animals[i];
            let fed = a.energy / a.d.fat_cap_kj.max(1.0);
            let was_mature = a.is_mature();
            a.dev_days += if fed > 0.3 { 1.0 } else { 0.4 };
            let adult = a.pheno.get(Trait::BodyMass);
            if a.dev_days < a.d.maturity_days * 2.0 {
                let target = growth_mass(adult, a.neonate_mass, a.dev_days, a.d.maturity_days);
                if target > a.mass {
                    let cost = (target - a.mass) * TISSUE_KJ_PER_KG;
                    if a.energy > cost + 0.1 * a.d.fat_cap_kj {
                        a.energy -= cost;
                        a.mass = target;
                    } else {
                        // Not enough energy: grow what can be afforded.
                        let afford =
                            ((a.energy - 0.1 * a.d.fat_cap_kj) / TISSUE_KJ_PER_KG).max(0.0);
                        a.mass += afford;
                        a.energy -= afford * TISSUE_KJ_PER_KG;
                    }
                }
                a.d = derive(&a.pheno, a.mass);
            }
            if !was_mature && a.is_mature() && rng.chance(a.pheno.get(Trait::Dispersal) as f64) {
                a.disperse_hours = (24.0 * (2.0 + 20.0 * a.pheno.get(Trait::Dispersal))) as u16;
                a.disperse_dir = rng.below(8) as u8;
            }
            // Birth.
            let due = a
                .pregnancy
                .as_ref()
                .is_some_and(|p| p.due_minute <= ctx.now);
            if due {
                let preg = a.pregnancy.take().unwrap();
                let (mid, gen, sp, arch, x, y) = (a.id, a.generation, a.species, a.arch, a.x, a.y);
                a.offspring += preg.embryos.len() as u32;
                a.last_birth_minute = ctx.now;
                let mut brng = Rng::keyed(self.seed, ctx.tick, mid, stream::BIRTH);
                for (g, muts) in preg.embryos {
                    let sex = if brng.chance(0.5) {
                        Sex::Female
                    } else {
                        Sex::Male
                    };
                    let pos = (
                        x + brng.range_f32(-0.05, 0.05),
                        y + brng.range_f32(-0.05, 0.05),
                    );
                    let pos = (
                        self.world.wrap_x(pos.0),
                        pos.1.clamp(0.0, self.world.h as f32 - 0.01),
                    );
                    let child = self.build_animal(
                        sp,
                        arch,
                        g,
                        sex,
                        pos,
                        0.0,
                        0.6,
                        (mid, preg.father),
                        gen + 1,
                        muts,
                        &mut brng,
                    );
                    self.pending.push(child);
                }
            }
        }
    }

    fn receptive(&self, a: &Animal, ctx: &Ctx) -> bool {
        if !a.is_mature() {
            return false;
        }
        let fed = a.energy / a.d.fat_cap_kj.max(1.0);
        let window = year_distance(ctx.year_frac, a.pheno.get(Trait::BreedingDay));
        match a.sex {
            Sex::Female => {
                a.pregnancy.is_none()
                    && fed > 0.35
                    && window < 0.12
                    && (a.last_birth_minute == 0
                        || (ctx.now - a.last_birth_minute) as f32
                            > a.d.weaning_days * ctx.day_minutes as f32)
            }
            Sex::Male => fed > 0.25 && window < 0.2,
        }
    }

    fn act(&mut self, i: usize, ctx: &Ctx) {
        let mut rng = Rng::keyed(self.seed, ctx.tick, self.animals[i].id, stream::DECISION);
        let tile = self.world.tile_of(self.animals[i].x, self.animals[i].y);
        let ta = self.world.temp_fast(tile, ctx.season, ctx.diurnal);
        let tile_km = self.world.tile_km as f32;
        let reach = (self.animals[i].d.perception_km / tile_km).clamp(0.25, 1.5);

        // --- Perception: predators nearby (only where the grid says there are any).
        self.animals[i].fear *= 0.5;
        self.animals[i].hunt_cooldown = self.animals[i].hunt_cooldown.saturating_sub(1);
        let prey_like = self.animals[i].d.plant_eff > 0.15 || self.animals[i].mass < 20.0;
        if prey_like {
            let near = tiles_around(&self.world, self.animals[i].x, self.animals[i].y, reach);
            if near.iter().any(|&t| self.grid.predators[t] > 0) {
                let me = &self.animals[i];
                let p_detect = 0.25 + 0.6 * me.pheno.get(Trait::Senses);
                let mut best: Option<(f32, f32, f32)> = None;
                for &t in near.iter() {
                    for &j in self.grid.preds(t) {
                        let o = &self.animals[j as usize];
                        if !o.alive
                            || o.arch == me.arch
                            || o.d.meat_eff < 0.3
                            || o.mass < 0.3 * me.mass
                        {
                            continue;
                        }
                        let dx = wrap_dx(o.x - me.x, self.world.w as f32);
                        let dy = o.y - me.y;
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist <= reach && best.is_none_or(|b| dist < b.0) {
                            best = Some((dist, o.x, o.y));
                        }
                    }
                }
                if let Some((dist, px, py)) = best {
                    if rng.chance(p_detect as f64) {
                        let a = &mut self.animals[i];
                        a.fear =
                            (1.0 - dist / reach).max(0.3) * (1.2 - a.pheno.get(Trait::Boldness));
                        a.flee_from = (px, py);
                    }
                }
            }
        }

        // --- Decision.
        let receptive = {
            let a = &self.animals[i];
            self.receptive(a, ctx)
        };
        let a = &self.animals[i];
        let hunger = a.hunger();
        let thirst = a.thirst();
        let mother_idx = if a.dev_days < a.d.weaning_days {
            self.index
                .get(&a.mother)
                .copied()
                .filter(|&m| self.animals[m].alive)
        } else {
            None
        };
        let carnivore = a.d.meat_eff >= a.d.plant_eff;
        let action = if mother_idx.is_some() {
            if a.fear > 0.4 {
                Action::Flee
            } else {
                Action::FollowMother
            }
        } else if a.fear > 0.4 {
            Action::Flee
        } else if a.disperse_hours > 0 && thirst < 0.6 && hunger < 0.7 {
            Action::Disperse
        } else {
            let eat = if carnivore {
                if self.world.carcass[tile] > 50.0 {
                    Action::Scavenge
                } else if a.hunt_cooldown == 0 {
                    Action::Hunt
                } else {
                    Action::Wander
                }
            } else {
                Action::Graze
            };
            let mut opts = [
                (
                    Action::Drink,
                    if thirst > 0.12 { thirst * 1.4 } else { 0.0 },
                ),
                (
                    eat,
                    if hunger > 0.05 {
                        hunger * 1.1 + 0.08
                    } else {
                        0.0
                    },
                ),
                (
                    Action::SeekMate,
                    if receptive {
                        0.55 * (1.0 - hunger)
                    } else {
                        0.0
                    },
                ),
                (Action::Rest, 0.18 + if ctx.night { 0.35 } else { 0.0 }),
                (Action::Wander, 0.1 + 0.1 * a.pheno.get(Trait::Sociality)),
            ];
            for o in opts.iter_mut() {
                o.1 += rng.f32() * 0.05;
            }
            opts.iter()
                .copied()
                .fold((Action::Rest, -1.0), |b, o| if o.1 > b.1 { o } else { b })
                .0
        };
        // Behavioural thermoregulation: rest in shade instead of overheating.
        let action = {
            let a = &self.animals[i];
            let urgent = matches!(action, Action::Flee | Action::Drink | Action::FollowMother);
            if !urgent && thermal(&a.d, a.d.bmr_w * action.activity(), ta).heat_stress > 0.0 {
                Action::Rest
            } else {
                action
            }
        };
        self.animals[i].action = action;

        // --- Execution.
        let mut moved_km = 0.0f32;
        match action {
            Action::Graze => moved_km = self.graze(i, tile, &mut rng),
            Action::Hunt => moved_km = self.hunt(i, tile, reach, &mut rng),
            Action::Scavenge => self.scavenge(i, tile),
            Action::Drink => moved_km = self.drink(i, tile, &mut rng),
            Action::Flee => {
                let a = &self.animals[i];
                let (fx, fy) = a.flee_from;
                let dx = wrap_dx(a.x - fx, self.world.w as f32);
                let dy = a.y - fy;
                let l = (dx * dx + dy * dy).sqrt().max(1e-3);
                let km = a.d.vmax_kmh * 0.15;
                let t = (a.x + dx / l * 2.0, a.y + dy / l * 2.0);
                moved_km = self.move_toward(i, t, km);
                self.animals[i].fear *= 0.6;
            }
            Action::SeekMate => moved_km = self.seek_mate(i, tile, reach, ctx, &mut rng),
            Action::Disperse => {
                let a = &mut self.animals[i];
                a.disperse_hours -= 1;
                let (dx, dy) = DIRS[a.disperse_dir as usize];
                let km = a.d.cruise_kmh;
                let t = (a.x + dx as f32 * 3.0, a.y + dy as f32 * 3.0);
                moved_km = self.move_toward(i, t, km);
                if moved_km == 0.0 {
                    let a = &mut self.animals[i];
                    a.disperse_dir = rng.below(8) as u8;
                }
            }
            Action::FollowMother => {
                if let Some(m) = mother_idx {
                    let (mx, my) = (self.animals[m].x, self.animals[m].y);
                    let km = self.animals[i].d.cruise_kmh;
                    moved_km = self.move_toward(i, (mx, my), km);
                    self.nurse(m, i);
                    if self.animals[i].dev_days > 0.5 * self.animals[i].d.weaning_days
                        && !carnivore
                        && self.animals[i].hunger() > 0.3
                    {
                        self.graze(i, tile, &mut rng);
                    }
                }
            }
            Action::Wander => moved_km = self.wander(i, tile, &mut rng),
            Action::Rest => {}
        }

        // --- Physiology (energy, water, temperature).
        let a = &mut self.animals[i];
        let heat_w = a.d.bmr_w * action.activity();
        let ta_eff = if action == Action::Rest {
            if ta > 25.0 {
                ta - crate::physiology::SHADE_COOLING
            } else if ta < 10.0 {
                ta + crate::physiology::SHELTER_WARMING
            } else {
                ta
            }
        } else {
            ta
        };
        let th = thermal(&a.d, heat_w, ta_eff);
        let mut cost_kj = (heat_w + th.extra_w) * 3.6 + locomotion_kj(a.mass, moved_km * 1000.0);
        if let Some(p) = &a.pregnancy {
            cost_kj += p.litter_kg * TISSUE_KJ_PER_KG / 0.5 / (a.d.gestation_days * 24.0).max(1.0);
        }
        a.energy = (a.energy - cost_kj).min(a.d.fat_cap_kj);
        a.water_deficit += a.d.water_loss_g_day / 24.0 + th.evap_w * 3600.0 / EVAP_J_PER_G;
        a.heat_damage = (a.heat_damage * 0.85 + th.heat_stress * 0.2).max(0.0);
        a.cold_damage = (a.cold_damage * 0.95 + th.cold_stress * 0.15).max(0.0);
        let cause = if a.energy < 0.0 {
            Some(if th.extra_w > 0.4 * heat_w {
                Cause::Cold
            } else {
                Cause::Starvation
            })
        } else if a.water_deficit > a.d.water_cap_g {
            Some(Cause::Dehydration)
        } else if a.heat_damage > 1.0 {
            Some(Cause::Heat)
        } else if a.cold_damage > 1.0 {
            Some(Cause::Cold)
        } else {
            None
        };
        if let Some(c) = cause {
            self.kill(i, c);
        }
    }

    /// Moves animal `i` up to `km` toward tile coordinates `t`; refuses to enter the ocean.
    fn move_toward(&mut self, i: usize, t: (f32, f32), km: f32) -> f32 {
        let tile_km = self.world.tile_km as f32;
        let w = self.world.w as f32;
        let a = &self.animals[i];
        let dx = wrap_dx(t.0 - a.x, w);
        let dy = t.1 - a.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1e-4 {
            return 0.0;
        }
        let step = (km / tile_km).min(dist);
        let nx = self.world.wrap_x(a.x + dx / dist * step);
        let ny = (a.y + dy / dist * step).clamp(0.0, self.world.h as f32 - 0.001);
        let nt = self.world.tile_of(nx, ny);
        if !self.world.is_land(nt) {
            self.animals[i].target = None;
            return 0.0;
        }
        let a = &mut self.animals[i];
        a.x = nx;
        a.y = ny;
        step * tile_km
    }

    fn tile_centre(&self, t: usize) -> (f32, f32) {
        (
            (t % self.world.w) as f32 + 0.5,
            (t / self.world.w) as f32 + 0.5,
        )
    }

    fn graze(&mut self, i: usize, tile: usize, rng: &mut Rng) -> f32 {
        let area = self.params.habitat_m2_per_tile as f32;
        let kj_g = self.params.forage_kj_per_g as f32;
        let a = &self.animals[i];
        let eff = a.d.plant_eff.max(0.01);
        let q = roughage_quality(a.mass);
        let wanted = (a.d.max_intake_kj_day / 12.0)
            .min(a.d.fat_cap_kj - a.energy)
            .max(0.0);
        let mut want = wanted;
        // High-quality forage first, then coarse forage if the gut can use it.
        let g1 = (want / (kj_g * eff)).min(self.world.forage[tile] * area * 0.25);
        self.world.forage[tile] -= g1 / area;
        want -= g1 * kj_g * eff;
        if want > 0.0 && q > 0.1 {
            let e2 = eff * q;
            let g2 = (want / (kj_g * e2)).min(self.world.roughage[tile] * area * 0.25);
            self.world.roughage[tile] -= g2 / area;
            want -= g2 * kj_g * e2;
        }
        self.animals[i].energy += wanted - want;
        let value = |w: &World, t: usize| w.forage[t] + q * w.roughage[t];
        // Poor pickings: head for the best neighbouring tile.
        if want > 0.5 * wanted && wanted > 0.0 {
            let mut best = (value(&self.world, tile) * 1.3, NOWHERE as usize);
            for d in 0..8 {
                if let Some(n) = self.world.neighbour(tile, d) {
                    let v = value(&self.world, n);
                    if self.world.is_land(n) && v > best.0 {
                        best = (v, n);
                    }
                }
            }
            if best.1 != NOWHERE as usize {
                let c = self.tile_centre(best.1);
                let t = (
                    c.0 + rng.range_f32(-0.4, 0.4),
                    c.1 + rng.range_f32(-0.4, 0.4),
                );
                let km = self.animals[i].d.cruise_kmh;
                return self.move_toward(i, t, km);
            }
        }
        self.jitter(i, rng, 0.03)
    }

    fn jitter(&mut self, i: usize, rng: &mut Rng, amount: f32) -> f32 {
        let a = &self.animals[i];
        let tile = self.world.tile_of(a.x, a.y);
        let (cx, cy) = self.tile_centre(tile);
        // Random walk that stays inside the current tile.
        let t = (
            (a.x + rng.range_f32(-amount, amount)).clamp(cx - 0.49, cx + 0.49),
            (a.y + rng.range_f32(-amount, amount)).clamp(cy - 0.49, cy + 0.49),
        );
        let km = self.world.tile_km as f32 * amount;
        self.move_toward(i, t, km)
    }

    fn wander(&mut self, i: usize, tile: usize, rng: &mut Rng) -> f32 {
        let a = &self.animals[i];
        if a.pheno.get(Trait::Sociality) > 0.5 {
            // Herd toward conspecifics in the same tile.
            let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
            for &j in self.grid.cell(tile) {
                let o = &self.animals[j as usize];
                if o.species == a.species && o.alive {
                    sx += wrap_dx(o.x - a.x, self.world.w as f32);
                    sy += o.y - a.y;
                    n += 1.0;
                }
            }
            if n > 1.0 {
                let t = (a.x + sx / n, a.y + sy / n);
                let km = a.d.cruise_kmh * 0.3;
                return self.move_toward(i, t, km);
            }
        }
        self.jitter(i, rng, 0.08)
    }

    fn drink(&mut self, i: usize, tile: usize, rng: &mut Rng) -> f32 {
        if rng.chance(self.world.water_access[tile] as f64) {
            self.animals[i].water_deficit = 0.0;
            return 0.0;
        }
        let dir = self.world.water_dir[tile];
        if dir == NOWHERE || dir == HERE {
            return self.jitter(i, rng, 0.1);
        }
        let n = self.world.neighbour(tile, dir as usize).unwrap_or(tile);
        let t = self.tile_centre(n);
        let km = self.animals[i].d.cruise_kmh * 1.5;
        self.move_toward(i, t, km)
    }

    fn scavenge(&mut self, i: usize, tile: usize) {
        let a = &self.animals[i];
        let want = (a.d.max_intake_kj_day / 4.0)
            .min(a.d.fat_cap_kj - a.energy)
            .max(0.0);
        let eff = a.d.meat_eff.max(0.05);
        let take = (want / eff).min(self.world.carcass[tile]);
        self.world.carcass[tile] -= take;
        self.animals[i].energy += take * eff;
    }

    fn hunt(&mut self, i: usize, tile: usize, reach: f32, rng: &mut Rng) -> f32 {
        let near = tiles_around(&self.world, self.animals[i].x, self.animals[i].y, reach);
        let me = &self.animals[i];
        let (mx, my, mmass) = (me.x, me.y, me.mass);
        let packmates = self
            .grid
            .cell(tile)
            .iter()
            .filter(|&&j| {
                let o = &self.animals[j as usize];
                o.species == me.species && o.alive && o.id != me.id && o.is_mature()
            })
            .count()
            .min(6) as f32
            * me.pheno.get(Trait::Sociality);
        let max_prey = mmass * 2.0 * (1.0 + 0.4 * packmates);
        let p_base = 0.25 + 0.6 * me.pheno.get(Trait::Senses);
        let doy = self.day_of_year();
        let mut target: Option<usize> = None;
        let mut nearest_prey: Option<(f32, f32, f32)> = None;
        let offset = rng.below(near.len().max(1));
        'scan: for k in 0..near.len() {
            let t = near[(k + offset) % near.len()];
            let bg = self
                .world
                .background_luminance(t, self.world.temp_day(t, doy));
            let cover = self.world.biome[t].props().cover;
            for &j in self.grid.cell(t) {
                let j = j as usize;
                let o = &self.animals[j];
                if !o.alive || o.arch == me.arch || o.mass > max_prey || o.mass < 0.005 * mmass {
                    continue;
                }
                let dx = wrap_dx(o.x - mx, self.world.w as f32);
                let dy = o.y - my;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > reach {
                    continue;
                }
                if nearest_prey.is_none_or(|b| dist < b.0) {
                    nearest_prey = Some((dist, o.x, o.y));
                }
                let camo = camouflage(o.pheno.get(Trait::CoatLuminance), bg);
                let p = p_base
                    * (1.0 - 0.6 * camo)
                    * (1.0 - 0.4 * cover)
                    * (1.0 + 0.5 * o.pheno.get(Trait::Ornament));
                if rng.chance(p as f64) {
                    target = Some(j);
                    break 'scan;
                }
            }
        }
        let Some(j) = target else {
            // Nothing spotted: move toward the nearest prey sign or roam.
            if let Some((_, px, py)) = nearest_prey {
                let km = self.animals[i].d.cruise_kmh;
                return self.move_toward(i, (px, py), km);
            }
            if rng.chance(0.3) {
                let d = rng.below(8);
                if let Some(n) = self.world.neighbour(tile, d) {
                    let c = self.tile_centre(n);
                    let km = self.animals[i].d.cruise_kmh;
                    return self.move_toward(i, c, km);
                }
            }
            return self.jitter(i, rng, 0.1);
        };

        // Chase, resolved analytically (TRD TR-BH5).
        let (pred, prey) = (&self.animals[i], &self.animals[j]);
        let prey_tile = self.world.tile_of(prey.x, prey.y);
        let herd = self
            .grid
            .cell(prey_tile)
            .iter()
            .filter(|&&k| self.animals[k as usize].species == prey.species)
            .count() as f32;
        let vigilance = 1.0
            - (1.0 - (0.2 + 0.6 * prey.pheno.get(Trait::Senses))).powf(1.0 + 0.3 * herd.min(20.0));
        let early = rng.chance(vigilance as f64);
        let speed = pred.d.vmax_kmh / prey.d.vmax_kmh.max(1.0);
        let endure = (pred.d.stamina_s / prey.d.stamina_s.max(1.0)).clamp(0.2, 5.0);
        let logit = 3.0 * (speed - 1.0) + 0.8 * (endure.ln()) + 0.5 * packmates
            - 2.5 * (prey.mass / pred.mass - 0.6).max(0.0)
            - if early { 1.2 } else { -0.6 }
            - 0.5
            + 0.6 * pred.pheno.get(Trait::Boldness);
        let success = rng.chance(sigmoid(logit) as f64);
        let chase_m = pred.d.vmax_kmh / 3.6 * pred.d.stamina_s.min(90.0);
        let chase_kj = locomotion_kj(pred.mass, chase_m);
        let (prey_mass, px, py) = (prey.mass, prey.x, prey.y);
        self.animals[i].hunt_cooldown = 3;
        self.animals[i].energy -= chase_kj;
        if success {
            self.kill(j, Cause::Predation);
            let carcass = prey_mass * 1000.0 * 0.6 * self.params.meat_kj_per_g as f32;
            let p = &self.animals[i];
            let eat = carcass
                .min(p.d.max_intake_kj_day * 1.5)
                .min((p.d.fat_cap_kj - p.energy).max(0.0) / p.d.meat_eff.max(0.05));
            let eff = p.d.meat_eff;
            self.animals[i].energy += eat * eff;
            let pt = self.world.tile_of(px, py);
            self.world.carcass[pt] += carcass - eat;
            self.animals[i].x = px;
            self.animals[i].y = py;
        }
        // Chase energy was charged above; no further travel cost this tick.
        0.0
    }

    fn seek_mate(&mut self, i: usize, tile: usize, reach: f32, ctx: &Ctx, rng: &mut Rng) -> f32 {
        let me = &self.animals[i];
        if me.sex == Sex::Male {
            // Males move toward the nearest receptive-looking female.
            return self.wander(i, tile, rng);
        }
        let near = tiles_around(&self.world, me.x, me.y, reach);
        let pref = me.pheno.get(Trait::Preference);
        let choosy = me.pheno.get(Trait::Choosiness);
        let mut best: Option<(f32, usize)> = None;
        let mut nearest_male: Option<(f32, f32, f32)> = None;
        for &t in near.iter() {
            for &j in self.grid.cell(t) {
                let j = j as usize;
                let o = &self.animals[j];
                if !o.alive || o.sex != Sex::Male || o.arch != me.arch || !self.receptive(o, ctx) {
                    continue;
                }
                let dx = wrap_dx(o.x - me.x, self.world.w as f32);
                let dy = o.y - me.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > reach {
                    continue;
                }
                if nearest_male.is_none_or(|b| dist < b.0) {
                    nearest_male = Some((dist, o.x, o.y));
                }
                let acc = mate_acceptance(pref, choosy, o.pheno.get(Trait::Ornament))
                    * (0.6 + 0.4 * (o.energy / o.d.fat_cap_kj).min(1.0));
                if best.is_none_or(|b| acc > b.0) {
                    best = Some((acc, j));
                }
            }
        }
        let Some((acc, m)) = best else {
            if let Some((_, x, y)) = nearest_male {
                let km = self.animals[i].d.cruise_kmh;
                return self.move_toward(i, (x, y), km);
            }
            return self.wander(i, tile, rng);
        };
        if !rng.chance(acc as f64) {
            return 0.0;
        }
        self.conceive(i, m, ctx, rng);
        0.0
    }

    fn conceive(&mut self, f: usize, m: usize, ctx: &Ctx, rng: &mut Rng) {
        let arch_id = self.animals[f].arch as usize;
        let rates = self.mutation_rates();
        let year = self.year() as f32;
        let fed = (self.animals[f].energy / self.animals[f].d.fat_cap_kj).min(1.0);
        let litter_mean = self.animals[f].pheno.get(Trait::LitterSize) * (0.6 + 0.4 * fed);
        let litter =
            (litter_mean.floor() as u32 + u32::from(rng.f32() < litter_mean.fract())).max(1);
        let mut embryos = vec![];
        let mut stillborn = 0;
        let mut mrng = Rng::keyed(self.seed, ctx.tick, self.animals[f].id, stream::MEIOSIS);
        for _ in 0..litter {
            let arch = &self.archs[arch_id];
            let mut ga = arch.meiosis(&self.animals[f].genome, &mut mrng);
            let mut gb = arch.meiosis(&self.animals[m].genome, &mut mrng);
            let mut muts = arch.mutate(&mut ga, &mut self.alleles, &rates, year, &mut mrng);
            muts += arch.mutate(&mut gb, &mut self.alleles, &rates, year, &mut mrng);
            let g = Genome { hap: [ga, gb] };
            let v = dmi_viability(
                arch,
                &self.alleles,
                &g,
                self.seed,
                self.params.dmi_probability,
                self.params.dmi_cost,
            );
            if mrng.chance(v) {
                embryos.push((g, muts));
            } else {
                stillborn += 1;
            }
        }
        self.deaths[Cause::Stillbirth as usize] += stillborn;
        self.animals[m].matings += 1;
        let father_id = self.animals[m].id;
        let fa = &mut self.animals[f];
        fa.matings += 1;
        if embryos.is_empty() {
            return;
        }
        let neonate = fa.d.neonate_kg;
        fa.pregnancy = Some(Pregnancy {
            father: father_id,
            due_minute: ctx.now + (fa.d.gestation_days * ctx.day_minutes as f32) as u64,
            litter_kg: neonate * embryos.len() as f32,
            embryos,
        });
    }

    /// Mother feeds a dependent young (lactation, TRD TR-R6/R7).
    fn nurse(&mut self, m: usize, c: usize) {
        let need = (self.animals[c].d.fat_cap_kj * 0.8 - self.animals[c].energy).max(0.0);
        let give = need.min(self.animals[m].energy * 0.04);
        self.animals[m].energy -= give * 1.25;
        self.animals[c].energy += give;
        let water = self.animals[c]
            .water_deficit
            .min(self.animals[m].d.water_cap_g * 0.02);
        self.animals[c].water_deficit -= water;
        self.animals[m].water_deficit += water;
    }
}
