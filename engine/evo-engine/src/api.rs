//! Read-only views of the simulation for clients (UI, CLI, tests): status,
//! species, phylogeny, metrics, events, inspector, map layers and frames
//! (TRD TR-API1 queries).

use crate::animal::Cause;
use crate::sim::{Mode, Simulation};
use crate::species_lib;
use crate::templates;
use crate::traits::{Trait, N_TRAITS};
use crate::world::Biome;
use serde_json::{json, Value};

/// Visible organisms for one rendered frame.
#[derive(Default)]
pub struct Frame {
    /// x, y pairs in tile units.
    pub positions: Vec<f32>,
    pub species: Vec<u32>,
    pub actions: Vec<u8>,
    pub ids: Vec<f64>,
}

impl Simulation {
    pub fn catalog() -> Value {
        json!({
            "templates": templates::templates().iter().map(|t| json!({
                "key": t.key, "name": t.name, "description": t.description,
                "founders": t.founders,
            })).collect::<Vec<_>>(),
            "species": species_lib::library().iter().map(|s| json!({
                "key": s.key, "name": s.name, "description": s.description,
                "color": s.color,
            })).collect::<Vec<_>>(),
            "traits": Trait::ALL.iter().map(|t| { let i = t.info(); json!({
                "key": i.key, "label": i.label, "unit": i.unit,
            })}).collect::<Vec<_>>(),
            "biomes": Biome::ALL.iter().map(|b| b.props().name).collect::<Vec<_>>(),
        })
    }

    pub fn world_info(&self) -> Value {
        json!({
            "w": self.world.w, "h": self.world.h,
            "tile_km": self.world.tile_km,
            "lat_north": self.world.gen.lat_north, "lat_south": self.world.gen.lat_south,
            "template": self.template_name, "template_key": self.template_key,
            "seed": self.seed,
            "patch_size": self.params.patch_size,
        })
    }

    pub fn status(&self) -> Value {
        let deaths: serde_json::Map<String, Value> = Cause::ALL
            .iter()
            .map(|c| (c.label().to_string(), json!(self.deaths[*c as usize])))
            .collect();
        json!({
            "year": self.year(),
            "day_of_year": self.day_of_year(),
            "hour": (self.minutes % self.params.day_minutes) / 60,
            "mode": match self.mode { Mode::Individual => "Individual", Mode::Population => "Population" },
            "focus": self.focus,
            "individuals": self.animals.len(),
            "population": self.population(),
            "species_alive": self.species.alive().count(),
            "species_total": self.species.list.len(),
            "alleles": self.alleles.len(),
            "births": self.births,
            "deaths": deaths,
            "realism": format!("{:?}", self.params.realism),
            "mutation_rate": self.params.effective_mutation_rate(),
            "global_temp_offset": self.world.global_offset,
            "individual_cap": self.params.individual_cap,
            "events": self.events.len(),
        })
    }

    pub fn species_json(&self) -> Value {
        let counts = self.species_counts();
        Value::Array(
            self.species
                .list
                .iter()
                .map(|s| {
                    let last = self.metrics.series.get(&s.id).and_then(|v| v.last());
                    json!({
                        "id": s.id,
                        "name": s.name,
                        "common_name": s.common_name,
                        "color": s.color,
                        "parent": s.parent,
                        "origin_year": s.origin_year,
                        "extinct_year": s.extinct_year,
                        "split_cause": s.split_cause,
                        "n": counts.get(&s.id).copied().unwrap_or(0),
                        "mean": last.map(|l| l.mean.clone()),
                        "het": last.map(|l| l.het),
                        "origin_mean": s.origin_mean.map(|m| m.to_vec()),
                    })
                })
                .collect(),
        )
    }

    pub fn metrics_json(&self, species: u32) -> Value {
        match self.metrics.series.get(&species) {
            None => json!([]),
            Some(v) => serde_json::to_value(v).unwrap_or(json!([])),
        }
    }

    pub fn events_json(&self, since: usize) -> Value {
        serde_json::to_value(&self.events[since.min(self.events.len())..]).unwrap_or(json!([]))
    }

    /// Phylogeny as Newick (PRD FR-O5): branch lengths in years.
    pub fn newick(&self) -> String {
        let now = self.year();
        fn rec(sim: &Simulation, id: u32, now: f64) -> String {
            let s = sim.species.get(id);
            let end = s.extinct_year.unwrap_or(now);
            let children: Vec<u32> = sim
                .species
                .list
                .iter()
                .filter(|c| c.parent == Some(id))
                .map(|c| c.id)
                .collect();
            let label = s.name.replace(' ', "_");
            if children.is_empty() {
                return format!("{}:{:.1}", label, end - s.origin_year);
            }
            // A split leaves the parent continuing as a sibling of each daughter.
            let mut node = format!(
                "{}:{:.1}",
                label,
                end - children
                    .iter()
                    .map(|&c| sim.species.get(c).origin_year)
                    .fold(f64::MIN, f64::max)
            );
            let mut last = s.origin_year;
            for &c in &children {
                let co = sim.species.get(c).origin_year;
                node = format!(
                    "({},{}):{:.1}",
                    node,
                    rec(sim, c, now),
                    (co - last).max(0.0)
                );
                last = co;
            }
            node
        }
        let roots: Vec<String> = self
            .species
            .list
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| rec(self, s.id, now))
            .collect();
        format!("({});", roots.join(","))
    }

    pub fn inspect(&self, id: u64) -> Value {
        let Some(a) = self.animals.iter().find(|a| a.id == id) else {
            return Value::Null;
        };
        let tile = self.world.tile_of(a.x, a.y);
        let sp = self.species.get(a.species);
        let traits: Vec<Value> = Trait::ALL
            .iter()
            .map(|t| {
                let i = t.info();
                json!({"key": i.key, "label": i.label, "unit": i.unit, "value": a.pheno.get(*t)})
            })
            .collect();
        let doy = self.day_of_year();
        json!({
            "id": a.id,
            "x": a.x,
            "y": a.y,
            "species": a.species,
            "species_name": sp.name,
            "common_name": sp.common_name,
            "sex": format!("{:?}", a.sex),
            "age_years": a.age_days(self.minutes, self.params.day_minutes) / 365.25,
            "mass_kg": a.mass,
            "adult_mass_kg": a.pheno.get(Trait::BodyMass),
            "mature": a.is_mature(),
            "energy_pct": 100.0 * a.energy / a.d.fat_cap_kj.max(1.0),
            "hydration_pct": 100.0 * (1.0 - a.thirst()),
            "action": a.action.label(),
            "generation": a.generation,
            "mother": a.mother,
            "father": a.father,
            "offspring": a.offspring,
            "matings": a.matings,
            "new_mutations": a.mutations,
            "pregnant": a.pregnancy.as_ref().map(|p| p.embryos.len()),
            "heterozygosity": self.archs[a.arch as usize].neutral_heterozygosity(&a.genome),
            "traits": traits,
            "physiology": {
                "bmr_w": a.d.bmr_w,
                "top_speed_kmh": a.d.vmax_kmh,
                "lifespan_hazard_g": a.d.gompertz_g,
                "maturity_days": a.d.maturity_days,
                "gestation_days": a.d.gestation_days,
                "plant_digestion": a.d.plant_eff,
                "meat_digestion": a.d.meat_eff,
            },
            "tile": {
                "biome": self.world.biome[tile].props().name,
                "temp_now": self.world.temp_at(tile, doy, ((self.minutes % self.params.day_minutes) / 60) as f64),
                "temp_mean": self.world.temp_mean(tile),
                "precip": self.world.precip[tile],
                "forage_pct": 100.0 * self.world.forage[tile] / self.world.forage_max[tile].max(1e-3),
                "elevation": self.world.elevation[tile],
            },
        })
    }

    /// A scalar map layer, one value per tile.
    pub fn layer(&self, kind: &str) -> Vec<f32> {
        let w = &self.world;
        let n = w.w * w.h;
        let doy = self.day_of_year();
        match kind {
            "elevation" => w.elevation.clone(),
            "temperature" => (0..n).map(|i| w.temp_day(i, doy)).collect(),
            "temp_mean" => (0..n).map(|i| w.temp_mean(i)).collect(),
            "precip" => w.precip.clone(),
            "biome" => w.biome.iter().map(|b| *b as u8 as f32).collect(),
            "water" => w.water.iter().map(|x| *x as u8 as f32).collect(),
            "forage" => (0..n)
                .map(|i| {
                    if w.forage_max[i] > 0.0 {
                        w.forage[i] / w.forage_max[i]
                    } else {
                        -1.0
                    }
                })
                .collect(),
            "npp" => w.npp.clone(),
            "density" => self.density(None),
            _ => vec![0.0; n],
        }
    }

    /// Organisms per tile (all species or one). In Population mode deme counts are
    /// spread over the patch's land tiles.
    pub fn density(&self, species: Option<u32>) -> Vec<f32> {
        let n = self.world.w * self.world.h;
        let mut out = vec![0f32; n];
        match self.mode {
            Mode::Individual => {
                for a in &self.animals {
                    if species.is_none_or(|s| s == a.species) {
                        out[self.world.tile_of(a.x, a.y)] += 1.0;
                    }
                }
                self.spread_demes(&self.frozen, species, &mut out);
            }
            Mode::Population => self.spread_demes(&self.demes, species, &mut out),
        }
        out
    }

    fn spread_demes(
        &self,
        demes: &[crate::population::Deme],
        species: Option<u32>,
        out: &mut [f32],
    ) {
        let ps = self.params.patch_size;
        let pw = self.patches_w();
        for d in demes {
            if species.is_some_and(|s| s != d.species) {
                continue;
            }
            let (px, py) = (d.patch as usize % pw, d.patch as usize / pw);
            // Spread each deme over its patch in proportion to food, as animals would be.
            let tiles: Vec<usize> = (py * ps..((py + 1) * ps).min(self.world.h))
                .flat_map(|y| (px * ps..((px + 1) * ps).min(self.world.w)).map(move |x| (x, y)))
                .map(|(x, y)| y * self.world.w + x)
                .filter(|&i| self.world.is_land(i))
                .collect();
            let food = |t: usize| 1.0 + self.world.forage_max[t] + self.world.roughage_max[t];
            let total: f32 = tiles.iter().map(|&t| food(t)).sum();
            for t in tiles {
                out[t] += d.total() as f32 * food(t) / total.max(1e-6);
            }
        }
    }

    /// Dominant species per tile in Population mode (u32::MAX = none), for colouring.
    pub fn dominant_species(&self) -> Vec<u32> {
        let n = self.world.w * self.world.h;
        let mut best = vec![(0u32, u32::MAX); n];
        let ps = self.params.patch_size;
        let pw = self.patches_w();
        for d in self.demes.iter().chain(&self.frozen) {
            let (px, py) = (d.patch as usize % pw, d.patch as usize / pw);
            for y in py * ps..((py + 1) * ps).min(self.world.h) {
                for x in px * ps..((px + 1) * ps).min(self.world.w) {
                    let i = y * self.world.w + x;
                    if self.world.is_land(i) && d.total() > best[i].0 {
                        best[i] = (d.total(), d.species);
                    }
                }
            }
        }
        best.into_iter().map(|b| b.1).collect()
    }

    pub fn frame(&self) -> Frame {
        let mut f = Frame::default();
        for a in &self.animals {
            f.positions.push(a.x);
            f.positions.push(a.y);
            f.species.push(a.species);
            f.actions.push(a.action as u8);
            f.ids.push(a.id as f64);
        }
        f
    }

    /// Nearest animal to (x, y) within `radius` tiles.
    pub fn pick(&self, x: f32, y: f32, radius: f32) -> Option<u64> {
        let w = self.world.w as f32;
        self.animals
            .iter()
            .map(|a| {
                let mut dx = (a.x - x).abs();
                dx = dx.min(w - dx);
                (dx * dx + (a.y - y).powi(2), a.id)
            })
            .filter(|(d, _)| *d <= radius * radius)
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .map(|x| x.1)
    }

    /// Trait-by-trait summary of a species (latest sample vs. its origin).
    pub fn trait_table(&self, species: u32) -> Value {
        let s = self.species.get(species);
        let last = self.metrics.series.get(&species).and_then(|v| v.last());
        Value::Array(
            (0..N_TRAITS)
                .map(|i| {
                    let t = Trait::from_index(i).info();
                    json!({
                        "key": t.key, "label": t.label, "unit": t.unit,
                        "now": last.map(|l| l.mean[i]),
                        "sd": last.map(|l| l.sd[i]),
                        "origin": s.origin_mean.map(|m| m[i]),
                    })
                })
                .collect(),
        )
    }
}
