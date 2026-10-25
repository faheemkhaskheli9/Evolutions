//! Read-only views of the simulation for clients (UI, CLI, tests): status,
//! species, phylogeny, metrics, events, inspector, map layers and frames
//! (TRD TR-API1 queries).

use crate::animal::Cause;
use crate::sim::{Mode, Simulation};
use crate::species_lib;
use crate::templates;
use crate::traits::{Trait, N_TRAITS};
use crate::world::{hydrology, Biome, World};
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
                "founders": t.founders, "gen": t.gen,
            })).collect::<Vec<_>>(),
            "default_gen": crate::world::WorldGenParams::default(),
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
            "axial_tilt": self.world.gen.axial_tilt,
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
            // Days since the start, with the time of day as the fraction.
            "days": self.minutes as f64 / self.params.day_minutes as f64,
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
                    let last = self.last_sample(s.id);
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

    /// Latest non-empty trait sample, so extinct species keep their final trait values.
    fn last_sample(&self, species: u32) -> Option<&crate::record::Sample> {
        self.metrics
            .series
            .get(&species)
            .and_then(|v| v.iter().rev().find(|l| l.n > 0))
    }

    pub fn metrics_json(&self, species: u32) -> Value {
        match self.metrics.series.get(&species) {
            None => json!([]),
            Some(v) => serde_json::to_value(v).unwrap_or(json!([])),
        }
    }

    /// Population-genetics series of a species (PRD FR-O7), with the frequency of each
    /// allele in `uids` per sample (null where it was not tracked).
    pub fn popgen_json(&self, species: u32, uids: &[u64]) -> Value {
        let Some(v) = self.popgen.series.get(&species) else {
            return json!([]);
        };
        Value::Array(
            v.iter()
                .map(|g| {
                    json!({
                        "year": g.year,
                        "ht": g.ht,
                        "hs": g.hs,
                        "fst": g.fst,
                        "ne": g.ne,
                        "ne_h": (g.ne_h >= 0.0).then_some(g.ne_h),
                        "generation": g.generation,
                        "segregating": g.segregating,
                        "freqs": uids.iter().map(|&u| g.freq(u)).collect::<Vec<_>>(),
                    })
                })
                .collect(),
        )
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
            let origins: Vec<f64> = children
                .iter()
                .map(|&c| sim.species.get(c).origin_year)
                .collect();
            // A split leaves the parent continuing as a sibling of each daughter.
            let mut node = format!(
                "{}:{:.1}",
                label,
                end - origins.iter().copied().fold(f64::MIN, f64::max)
            );
            // Nest from the most recent split outwards so the oldest split sits at the root
            // of this clade; each internal branch spans the time back to the previous split.
            let mut order: Vec<usize> = (0..children.len()).collect();
            order.sort_by(|&a, &b| origins[a].total_cmp(&origins[b]));
            for (k, &i) in order.iter().enumerate().rev() {
                let prev = if k == 0 {
                    s.origin_year
                } else {
                    origins[order[k - 1]]
                };
                node = format!(
                    "({},{}):{:.1}",
                    node,
                    rec(sim, children[i], now),
                    (origins[i] - prev).max(0.0)
                );
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
            "temperature" => {
                let sc = World::season_cos(doy);
                (0..n).map(|i| w.temp_season(i, sc)).collect()
            }
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
            "river_flow" => w.river_flow.clone(),
            // Index of the tile each tile drains into, or -1 at an outlet.
            "downstream" => hydrology::priority_flood(w)
                .1
                .iter()
                .map(|&d| if d == usize::MAX { -1.0 } else { d as f32 })
                .collect(),
            // Water surface (m): a lake's level, otherwise the ground.
            "water_level" => hydrology::priority_flood(w).0,
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
        let Some(s) = self.species.list.get(species as usize) else {
            return Value::Null;
        };
        let last = self.last_sample(species);
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

    /// The demes counted in the current mode (Individual mode counts only those held
    /// outside the focus region; the rest are animals).
    fn counted_demes(&self) -> &[crate::population::Deme] {
        match self.mode {
            Mode::Individual => &self.frozen,
            Mode::Population => &self.demes,
        }
    }

    /// Polymorphic alleles of a species (overall frequency 1–99%), for the
    /// allele-frequency map layer (PRD FR-O1): trait loci first, then
    /// incompatibility and neutral loci, each by expected heterozygosity.
    /// Alleles are named by `uid`, which survives allele-table compaction.
    pub fn allele_list(&self, species: u32, limit: usize) -> Value {
        let mut copies: std::collections::HashMap<u32, f64> = Default::default();
        let mut genomes = 0f64;
        let mut arch = None;
        for a in self.animals.iter().filter(|a| a.species == species) {
            arch = Some(a.arch);
            genomes += 1.0;
            for h in &a.genome.hap {
                for &x in h {
                    *copies.entry(x).or_default() += 1.0;
                }
            }
        }
        for d in self.counted_demes().iter().filter(|d| d.species == species) {
            let n = d.total() as f64;
            arch = Some(d.arch);
            genomes += n;
            for locus in &d.freqs {
                for &(x, f) in locus {
                    *copies.entry(x).or_default() += 2.0 * n * f as f64;
                }
            }
        }
        let Some(arch) = arch.and_then(|g| self.archs.get(g as usize)) else {
            return json!([]);
        };
        if genomes == 0.0 {
            return json!([]);
        }
        let mut rows: Vec<(u8, f64, u32, f64)> = copies
            .into_iter()
            .filter_map(|(x, c)| {
                let p = c / (2.0 * genomes);
                if !(0.01..=0.99).contains(&p) {
                    return None;
                }
                let rank = match arch.loci.get(self.alleles.get(x).locus as usize)?.kind {
                    crate::genetics::LocusKind::Trait(_) => 0,
                    crate::genetics::LocusKind::Dmi => 1,
                    crate::genetics::LocusKind::Neutral => 2,
                };
                Some((rank, -p * (1.0 - p), x, p))
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        Value::Array(
            rows.into_iter()
                .take(limit)
                .map(|(rank, _, x, p)| {
                    const KINDS: [&str; 3] = ["trait", "incompatibility", "neutral"];
                    let a = self.alleles.get(x);
                    let effects: Vec<Value> = a
                        .effects
                        .iter()
                        .filter(|e| e.0 != crate::genetics::NO_TRAIT && e.1 != 0.0)
                        .map(|e| {
                            let t = Trait::from_index(e.0 as usize).info();
                            json!({"key": t.key, "label": t.label, "effect": e.1})
                        })
                        .collect();
                    json!({
                        "uid": a.uid,
                        "locus": a.locus,
                        "kind": KINDS[rank as usize],
                        "derived": a.derived,
                        "origin_year": a.origin_year,
                        "dominance": a.dominance,
                        "effects": effects,
                        "freq": p,
                    })
                })
                .collect(),
        )
    }

    /// Frequency of one allele (by `uid`) in a species, per tile: pooled over each
    /// patch's animals and demes and laid over the patch's land tiles; -1 where the
    /// species is absent or the allele is unknown.
    pub fn allele_frequency(&self, species: u32, uid: u64) -> Vec<f32> {
        let n = self.world.w * self.world.h;
        let mut out = vec![-1f32; n];
        let Some(id) = self.alleles.alleles.iter().position(|a| a.uid == uid) else {
            return out;
        };
        let locus = self.alleles.get(id as u32).locus as usize;
        // Per patch: (copies of the allele, genome copies).
        let mut acc: std::collections::HashMap<u32, (f64, f64)> = Default::default();
        for a in self.animals.iter().filter(|a| a.species == species) {
            let e = acc.entry(self.patch_of_tile_xy(a.x, a.y)).or_default();
            for h in &a.genome.hap {
                if h.get(locus) == Some(&(id as u32)) {
                    e.0 += 1.0;
                }
            }
            e.1 += 2.0;
        }
        for d in self.counted_demes().iter().filter(|d| d.species == species) {
            let n2 = 2.0 * d.total() as f64;
            let f = d
                .freqs
                .get(locus)
                .and_then(|l| l.iter().find(|x| x.0 == id as u32))
                .map_or(0.0, |x| x.1 as f64);
            let e = acc.entry(d.patch).or_default();
            e.0 += f * n2;
            e.1 += n2;
        }
        for (i, v) in out.iter_mut().enumerate() {
            if !self.world.is_land(i) {
                continue;
            }
            if let Some(&(c, t)) = acc.get(&self.patch_of_tile(i)) {
                if t > 0.0 {
                    *v = (c / t) as f32;
                }
            }
        }
        out
    }
}
