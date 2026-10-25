//! "Why?" explanations for Chronicle events (PRD FR-O8): trait shifts, population
//! crashes, extinctions and speciations, in plain language with the numbers behind them.
//!
//! The engine keeps, per species:
//! - a short history of yearly *vitals*: the hazard from each cause of death, food,
//!   fecundity, and the selection gradient on every trait split into the hazards and
//!   reproduction behind it. Population mode reads these from the same fitness model
//!   that moves allele frequencies; Individual mode measures them from who died of what.
//! - a longer, decimated series of the genetic mean and additive variance of each trait
//!   with the effective size, for the drift-versus-selection test (Lande 1976: under
//!   drift alone the mean moves by about √(V_A·t/Nₑ) in t generations).
//!
//! Explanations are written when an event happens and kept with it. Like `popgen`, all
//! of this is read from the state and never changes it: it is saved after the
//! population-genetics series, so it is not part of `state_hash()`, and saves made
//! before it existed load without it.

use crate::animal::Cause;
use crate::genetics::NO_TRAIT;
use crate::physiology::derive;
use crate::record::EventKind;
use crate::sim::{Mode, Simulation};
use crate::species::PopSummary;
use crate::traits::{Phenotype, Trait, N_TRAITS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// What pushes a trait's fitness up or down: five hazards, ageing and reproduction.
pub const N_DRIVERS: usize = 7;
pub const DRIVERS: [&str; N_DRIVERS] = [
    "starvation",
    "dehydration",
    "predation",
    "heat stress",
    "cold stress",
    "ageing",
    "reproduction",
];
/// Yearly vitals kept per species.
pub const RECENT_YEARS: usize = 60;
/// Genetic-series samples kept per species (decimated like `Metrics`).
pub const GENETIC_CAP: usize = 400;
/// Drift z-scores below this are "consistent with drift"; at or above `Z_SELECTION`
/// the change is "selection"; between them, "mixed".
pub const Z_DRIFT: f64 = 2.0;
pub const Z_SELECTION: f64 = 3.0;

/// One year of a species' vital rates and selection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vital {
    pub year: f64,
    pub n: u32,
    /// Hazard per year from each of the first six drivers (ageing is 0 in Population
    /// mode, where it depends on age class).
    pub hazard: [f32; 6],
    /// Food intake relative to need (Population mode), or −1.
    pub food: f32,
    /// Offspring per female per year surviving to age 1 (Population mode), or −1.
    pub fecundity: f32,
    /// Selection gradient per latent trait unit.
    pub beta: Vec<f32>,
    /// The gradient split by driver: `drivers[trait][driver]`.
    pub drivers: Vec<[f32; N_DRIVERS]>,
    /// Global temperature offset, °C.
    pub climate: f32,
    /// True when the gradient comes from the fitness model (Population mode), false
    /// when it was measured from deaths (Individual mode).
    pub modelled: bool,
}

/// A species' numbers and hazards in one year, kept longer than `Vital`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Demog {
    pub year: f64,
    pub n: u32,
    pub hazard: [f32; 6],
    pub food: f32,
    pub fecundity: f32,
    pub climate: f32,
    pub modelled: bool,
}

/// Genetic mean and additive variance of each trait (latent scale) at one time.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenTrack {
    pub year: f64,
    pub mean: Vec<f32>,
    pub va: Vec<f32>,
    /// Effective population size.
    pub ne: f32,
    /// Years per generation of drift (1 in Population mode, which resamples yearly).
    pub generation: f32,
}

/// Deaths of a species in Individual mode since the last vital sample.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeathAcc {
    /// Individual-years lived.
    pub exposure: f64,
    pub deaths: [f64; 6],
    /// Sum of the latent trait values of those that died, per driver.
    pub latent: Vec<[f64; N_TRAITS]>,
}

/// An explanation shown by the Chronicle's "Why?" button.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Why {
    /// "trait", "crash", "extinction" or "speciation".
    pub kind: String,
    pub species: u32,
    pub year: f64,
    /// Plain-language answer.
    pub summary: String,
    /// Supporting points, one sentence each.
    pub points: Vec<String>,
    /// (label, value) pairs behind the answer.
    pub numbers: Vec<(String, String)>,
    /// For trait shifts: "selection", "drift" or "mixed".
    pub verdict: Option<String>,
    /// Main cause, as a key: a driver ("cold stress", …), "food", "predator",
    /// "climate", "intervention", "reproduction", "allopatric" or "parapatric".
    pub cause: Option<String>,
    /// The chart that shows it: "trait:<key>", "population", "popgen" or "tree".
    pub chart: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Insight {
    pub recent: BTreeMap<u32, VecDeque<Vital>>,
    pub genetic: BTreeMap<u32, Vec<GenTrack>>,
    /// Numbers and hazards per year, decimated (for crashes and extinctions).
    pub demog: BTreeMap<u32, Vec<Demog>>,
    /// Explanations by event index.
    pub why: BTreeMap<u32, Why>,
    pub acc: BTreeMap<u32, DeathAcc>,
    /// Start of the Individual-mode accumulation, years; negative when none.
    pub acc_since: f64,
    pub last_exposure_year: f64,
    pub last_genetic_year: f64,
}

impl Default for Insight {
    fn default() -> Insight {
        Insight::new()
    }
}

impl Insight {
    pub fn new() -> Insight {
        Insight {
            recent: BTreeMap::new(),
            genetic: BTreeMap::new(),
            demog: BTreeMap::new(),
            why: BTreeMap::new(),
            acc: BTreeMap::new(),
            acc_since: -1.0,
            last_exposure_year: -1.0,
            last_genetic_year: -1.0,
        }
    }

    fn push_vital(&mut self, species: u32, v: Vital) {
        decimated_push(
            self.demog.entry(species).or_default(),
            Demog {
                year: v.year,
                n: v.n,
                hazard: v.hazard,
                food: v.food,
                fecundity: v.fecundity,
                climate: v.climate,
                modelled: v.modelled,
            },
        );
        let q = self.recent.entry(species).or_default();
        q.push_back(v);
        while q.len() > RECENT_YEARS {
            q.pop_front();
        }
    }

    fn push_genetic(&mut self, species: u32, g: GenTrack) {
        decimated_push(self.genetic.entry(species).or_default(), g);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Selection,
    Drift,
    Mixed,
}

impl Verdict {
    pub fn key(self) -> &'static str {
        match self {
            Verdict::Selection => "selection",
            Verdict::Drift => "drift",
            Verdict::Mixed => "mixed",
        }
    }
}

/// Classifies a change by how many drift standard deviations it spans.
pub fn verdict(z: f64) -> Verdict {
    if z >= Z_SELECTION {
        Verdict::Selection
    } else if z < Z_DRIFT {
        Verdict::Drift
    } else {
        Verdict::Mixed
    }
}

/// A change in a trait's genetic mean compared with what drift alone would give.
#[derive(Clone, Copy, Debug)]
pub struct DriftTest {
    /// Change in the genetic mean (latent scale).
    pub delta: f64,
    /// Standard deviation of the change expected from drift alone.
    pub sd: f64,
    /// |delta| / sd.
    pub z: f64,
    pub generations: f64,
    /// Harmonic-mean effective size over the interval.
    pub ne: f64,
    pub verdict: Verdict,
}

/// Lande's (1976) neutral test on the genetic series between `from` and `to` (years):
/// drift adds variance V_A/Nₑ to the mean each generation, so over the interval the
/// change has variance Σ V_A·Δt/Nₑ. None with fewer than two samples in the interval.
pub fn drift_test(track: &[GenTrack], t: usize, from: f64, to: f64) -> Option<DriftTest> {
    let s: Vec<&GenTrack> = track
        .iter()
        .filter(|g| g.year >= from - 1e-9 && g.year <= to + 1e-9)
        .collect();
    if s.len() < 2 {
        return None;
    }
    let (a, b) = (s[0], s[s.len() - 1]);
    let delta = (b.mean[t] - a.mean[t]) as f64;
    let (mut var, mut gens, mut inv_ne) = (0.0, 0.0, 0.0);
    for w in s.windows(2) {
        let (p, q) = (w[0], w[1]);
        let gen = 0.5 * (p.generation + q.generation).max(1e-3) as f64;
        let g = (q.year - p.year) / gen;
        let rate = |x: &GenTrack| x.va[t] as f64 / (x.ne as f64).max(1.0);
        var += g * 0.5 * (rate(p) + rate(q));
        gens += g;
        inv_ne += g * 0.5 * (1.0 / (p.ne as f64).max(1.0) + 1.0 / (q.ne as f64).max(1.0));
    }
    let sd = var.max(0.0).sqrt();
    let z = if sd > 1e-12 {
        delta.abs() / sd
    } else if delta.abs() < 1e-9 {
        0.0
    } else {
        f64::INFINITY
    };
    Some(DriftTest {
        delta,
        sd,
        z,
        generations: gens,
        ne: if inv_ne > 0.0 { gens / inv_ne } else { 0.0 },
        verdict: verdict(z),
    })
}

/// The driver a cause of death counts towards.
pub fn driver_of(cause: Cause) -> Option<usize> {
    match cause {
        Cause::Starvation => Some(0),
        Cause::Dehydration => Some(1),
        Cause::Predation => Some(2),
        Cause::Heat => Some(3),
        Cause::Cold => Some(4),
        Cause::OldAge => Some(5),
        _ => None,
    }
}

/// What a driver means for an individual with more (or less) of a trait.
fn benefit(driver: usize) -> &'static str {
    match driver {
        0 => "are less likely to starve",
        1 => "are less likely to die of thirst",
        2 => "are caught by predators less often",
        3 => "suffer less heat stress",
        4 => "suffer less cold stress",
        5 => "live longer",
        _ => "raise more offspring",
    }
}

fn cost(driver: usize) -> &'static str {
    match driver {
        0 => "need more food",
        1 => "lose more water",
        2 => "are caught by predators more often",
        3 => "suffer more heat stress",
        4 => "suffer more cold stress",
        5 => "age sooner",
        _ => "raise fewer offspring",
    }
}

fn fmt_n(x: f64) -> String {
    if x >= 1e6 {
        format!("{:.1} million", x / 1e6)
    } else {
        format!("{:.0}", x)
    }
}

/// Per-trait genetic mean and additive variance (latent scale) of pooled allele counts.
struct Pooled {
    arch: u16,
    /// Diploids counted.
    n: f64,
    /// Per trait locus (in `arch.trait_loci` order): (allele, copies).
    pools: Vec<Vec<(u32, f64)>>,
}

impl Simulation {
    /// Counts a death in Individual mode (see `DeathAcc`).
    pub(crate) fn note_death(&mut self, i: usize, cause: Cause) {
        let Some(k) = driver_of(cause) else {
            return;
        };
        let a = &self.animals[i];
        let acc = self.insight.acc.entry(a.species).or_default();
        if acc.latent.is_empty() {
            acc.latent = vec![[0.0; N_TRAITS]; 6];
        }
        acc.deaths[k] += 1.0;
        for t in 0..N_TRAITS {
            acc.latent[k][t] += a.latent[t] as f64;
        }
    }

    /// Records vitals and the genetic series at a metrics sample, then writes the
    /// explanations of the events listed in `pending` (event index, trait for shifts).
    pub(crate) fn record_insight(&mut self, year: f64, pending: &[(usize, Option<Trait>)]) {
        // Individual-years lived, for death rates.
        if self.mode == Mode::Individual && self.insight.last_exposure_year >= 0.0 {
            let dt = year - self.insight.last_exposure_year;
            let mut counts: BTreeMap<u32, f64> = BTreeMap::new();
            for a in &self.animals {
                *counts.entry(a.species).or_default() += 1.0;
            }
            for (sp, n) in counts {
                self.insight.acc.entry(sp).or_default().exposure += n * dt;
            }
        }
        self.insight.last_exposure_year = year;
        let climate = self.world.global_offset;

        if self.mode == Mode::Population && !self.selection.is_empty() {
            self.vitals_from_demes(year, climate);
            self.insight.acc.clear();
            self.insight.acc_since = year;
        } else if self.mode == Mode::Individual {
            if self.insight.acc_since < 0.0 {
                self.insight.acc_since = year;
            } else if year - self.insight.acc_since >= 1.0 - 1e-9 {
                self.vitals_from_deaths(year, climate);
                self.insight.acc.clear();
                self.insight.acc_since = year;
            }
        }
        if self.insight.last_genetic_year < 0.0
            || year - self.insight.last_genetic_year >= 1.0 - 1e-9
        {
            self.insight.last_genetic_year = year;
            self.record_genetic(year);
        }
        for &(idx, t) in pending {
            let Some(e) = self.events.get(idx) else {
                continue;
            };
            let Some(sp) = e.species else {
                continue;
            };
            let why = match (e.kind, t) {
                (EventKind::Adaptation, Some(t)) => {
                    let from = self.species.get(sp).origin_year;
                    Some(self.explain_trait(sp, t, from, year))
                }
                (EventKind::Crash, _) => Some(self.explain_decline(sp, false)),
                (EventKind::Extinction, _) => Some(self.explain_decline(sp, true)),
                _ => None,
            };
            if let Some(w) = why {
                self.insight.why.insert(idx as u32, w);
            }
        }
    }

    fn vitals_from_demes(&mut self, year: f64, climate: f32) {
        struct Sum {
            n: f64,
            hazard: [f64; 6],
            food: f64,
            fec: f64,
            beta: [f64; N_TRAITS],
            drivers: [[f64; N_DRIVERS]; N_TRAITS],
        }
        let mut by: BTreeMap<u32, Sum> = BTreeMap::new();
        for s in &self.selection {
            let e = by.entry(s.species).or_insert(Sum {
                n: 0.0,
                hazard: [0.0; 6],
                food: 0.0,
                fec: 0.0,
                beta: [0.0; N_TRAITS],
                drivers: [[0.0; N_DRIVERS]; N_TRAITS],
            });
            let w = s.n as f64;
            e.n += w;
            for t in 0..N_TRAITS {
                e.beta[t] += w * s.beta[t] as f64;
                for k in 0..N_DRIVERS {
                    e.drivers[t][k] += w * s.drivers[t][k] as f64;
                }
            }
        }
        for (sp, _, n, a) in &self.diagnostics {
            if let Some(e) = by.get_mut(sp) {
                let w = *n as f64;
                for (k, h) in [a.starvation, a.dehydration, a.predation, a.heat, a.cold]
                    .into_iter()
                    .enumerate()
                {
                    e.hazard[k] += w * h as f64;
                }
                e.food += w * a.food_ratio as f64;
                e.fec += w * a.fecundity as f64;
            }
        }
        for (sp, e) in by {
            if e.n <= 0.0 {
                continue;
            }
            let f = |x: f64| (x / e.n) as f32;
            self.insight.push_vital(
                sp,
                Vital {
                    year,
                    n: e.n as u32,
                    hazard: e.hazard.map(f),
                    food: f(e.food),
                    fecundity: f(e.fec),
                    beta: e.beta.iter().map(|&b| f(b)).collect(),
                    drivers: e.drivers.iter().map(|d| d.map(f)).collect(),
                    climate,
                    modelled: true,
                },
            );
        }
        self.selection.clear();
    }

    /// Individual mode: hazards are death rates, and each cause's share of the
    /// selection gradient is the viability selection differential it caused (dead
    /// animals' mean minus the living mean, times the death rate) over V_P (Lande &
    /// Arnold 1983), on the latent scale.
    fn vitals_from_deaths(&mut self, year: f64, climate: f32) {
        let mut living: BTreeMap<u32, (f64, [f64; N_TRAITS], [f64; N_TRAITS])> = BTreeMap::new();
        for a in &self.animals {
            let e = living
                .entry(a.species)
                .or_insert((0.0, [0.0; N_TRAITS], [0.0; N_TRAITS]));
            e.0 += 1.0;
            for t in 0..N_TRAITS {
                let x = a.latent[t] as f64;
                e.1[t] += x;
                e.2[t] += x * x;
            }
        }
        let acc = std::mem::take(&mut self.insight.acc);
        for (sp, d) in acc {
            if d.exposure <= 0.0 {
                continue;
            }
            let (n, sum, sq) =
                living
                    .get(&sp)
                    .copied()
                    .unwrap_or((0.0, [0.0; N_TRAITS], [0.0; N_TRAITS]));
            let mut beta = vec![0f32; N_TRAITS];
            let mut drivers = vec![[0f32; N_DRIVERS]; N_TRAITS];
            if n >= 2.0 && !d.latent.is_empty() {
                for t in 0..N_TRAITS {
                    let mean = sum[t] / n;
                    let vp = (sq[t] / n - mean * mean).max(1e-6);
                    for k in 0..6 {
                        if d.deaths[k] < 1.0 {
                            continue;
                        }
                        let dead = d.latent[k][t] / d.deaths[k];
                        let s = -(d.deaths[k] / d.exposure) * (dead - mean);
                        drivers[t][k] = (s / vp) as f32;
                        beta[t] += (s / vp) as f32;
                    }
                }
            }
            let mut hazard = [0f32; 6];
            for k in 0..6 {
                hazard[k] = (d.deaths[k] / d.exposure) as f32;
            }
            let years = (year - self.insight.acc_since).max(1e-9);
            self.insight.push_vital(
                sp,
                Vital {
                    year,
                    n: (d.exposure / years) as u32,
                    hazard,
                    food: -1.0,
                    fecundity: -1.0,
                    beta,
                    drivers,
                    climate,
                    modelled: false,
                },
            );
        }
    }

    fn record_genetic(&mut self, year: f64) {
        let mut pooled: BTreeMap<u32, Pooled> = BTreeMap::new();
        let new_pool = |arch: u16, loci: usize| Pooled {
            arch,
            n: 0.0,
            pools: vec![Vec::new(); loci],
        };
        let add =
            |pool: &mut Vec<(u32, f64)>, x: u32, c: f64| match pool.iter_mut().find(|p| p.0 == x) {
                Some(p) => p.1 += c,
                None => pool.push((x, c)),
            };
        for a in &self.animals {
            let arch = &self.archs[a.arch as usize];
            let e = pooled
                .entry(a.species)
                .or_insert_with(|| new_pool(a.arch, arch.trait_loci.len()));
            e.n += 1.0;
            for (k, &l) in arch.trait_loci.iter().enumerate() {
                for h in &a.genome.hap {
                    add(&mut e.pools[k], h[l], 1.0);
                }
            }
        }
        let demes = match self.mode {
            Mode::Individual => &self.frozen,
            Mode::Population => &self.demes,
        };
        for d in demes {
            let n = d.total() as f64;
            if n == 0.0 {
                continue;
            }
            let arch = &self.archs[d.arch as usize];
            let e = pooled
                .entry(d.species)
                .or_insert_with(|| new_pool(d.arch, arch.trait_loci.len()));
            e.n += n;
            for (k, &l) in arch.trait_loci.iter().enumerate() {
                for &(x, f) in &d.freqs[l] {
                    add(&mut e.pools[k], x, 2.0 * n * f as f64);
                }
            }
        }
        for (sp, p) in pooled {
            if p.n <= 0.0 {
                continue;
            }
            let arch = &self.archs[p.arch as usize];
            let mut mean = [0f64; N_TRAITS];
            for t in 0..N_TRAITS {
                mean[t] = arch.base[t] as f64;
            }
            let mut va = [0f64; N_TRAITS];
            let copies = 2.0 * p.n;
            for pool in &p.pools {
                // Per-locus mean effect on each trait, then the variance around it.
                let mut locus_mean = [0f64; N_TRAITS];
                let effect = |x: u32| {
                    let mut e = [0f64; N_TRAITS];
                    for &(t, v) in &self.alleles.get(x).effects {
                        if t != NO_TRAIT {
                            e[t as usize] += v as f64;
                        }
                    }
                    e
                };
                for &(x, c) in pool {
                    let e = effect(x);
                    for t in 0..N_TRAITS {
                        locus_mean[t] += c / copies * e[t];
                    }
                }
                for &(x, c) in pool {
                    let e = effect(x);
                    for t in 0..N_TRAITS {
                        va[t] += 2.0 * c / copies * (e[t] - locus_mean[t]).powi(2);
                    }
                }
                for t in 0..N_TRAITS {
                    mean[t] += 2.0 * locus_mean[t];
                }
            }
            let gen = self.popgen.series.get(&sp).and_then(|v| v.last());
            let (ne, generation) = match gen.filter(|g| (g.year - year).abs() < 1e-9) {
                // Population mode resamples allele frequencies every year with Nₑ from
                // the breeding sex ratio, which is exactly `g.ne`. Individuals vary in
                // how many young they raise, so in Individual mode Nₑ is lower: the
                // estimate from lost heterozygosity is used when it is.
                Some(g) => match self.mode {
                    Mode::Population => (g.ne, 1.0),
                    Mode::Individual => {
                        let ne = if g.ne_h > 0.0 { g.ne.min(g.ne_h) } else { g.ne };
                        (ne, g.generation)
                    }
                },
                None => (p.n as f32, 1.0),
            };
            self.insight.push_genetic(
                sp,
                GenTrack {
                    year,
                    mean: mean.iter().map(|&x| x as f32).collect(),
                    va: va.iter().map(|&x| x as f32).collect(),
                    ne: ne.max(1.0),
                    generation,
                },
            );
        }
    }

    /// Mean of the recorded vitals of a species within [from, to].
    fn mean_vital(&self, species: u32, from: f64, to: f64) -> Option<Vital> {
        let q = self.insight.recent.get(&species)?;
        let s: Vec<&Vital> = q
            .iter()
            .filter(|v| v.year >= from - 1e-9 && v.year <= to + 1e-9)
            .collect();
        mean_of(&s)
    }

    /// Why a trait's mean changed between `from` and `to` (years): drift or selection,
    /// and which hazard (or reproduction) the trait affects most.
    pub fn explain_trait(&self, species: u32, t: Trait, from: f64, to: f64) -> Why {
        let info = self.species.get(species);
        let label = t.info().label.to_lowercase();
        let mut points = vec![];
        let mut numbers = vec![];
        let track = self
            .insight
            .genetic
            .get(&species)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let test = drift_test(track, t as usize, from, to);

        // The phenotypic change, from the metrics series.
        let inside: Vec<f32> = self
            .metrics
            .series
            .get(&species)
            .map(|v| {
                v.iter()
                    .filter(|s| s.n > 0 && s.year >= from - 1e-9 && s.year <= to + 1e-9)
                    .map(|s| s.mean[t as usize])
                    .collect()
            })
            .unwrap_or_default();
        let (m0, m1) = (inside.first().copied(), inside.last().copied());
        let years = to - from;
        let change = match (m0, m1) {
            (Some(a), Some(b)) if a.abs() > 1e-9 => {
                numbers.push((
                    format!("Mean {label}"),
                    format!("{} → {} {}", fmt_val(a), fmt_val(b), t.info().unit),
                ));
                Some(100.0 * (b - a) / a.abs())
            }
            _ => None,
        };
        let dir = match (test, change) {
            (Some(d), _) if d.delta.abs() > 1e-12 => d.delta.signum(),
            (_, Some(c)) => (c as f64).signum(),
            _ => 0.0,
        };
        let moved = if dir >= 0.0 { "rose" } else { "fell" };
        let more = if dir >= 0.0 { "higher" } else { "lower" };
        let mut summary = match change {
            Some(c) => format!(
                "{}'s {label} {moved} by {:.0}% over {:.0} years.",
                info.common_name,
                c.abs(),
                years
            ),
            None => format!(
                "{}'s {label} {moved} over {:.0} years.",
                info.common_name, years
            ),
        };

        // Drift or selection.
        let verdict = test.map(|d| d.verdict);
        match test {
            Some(d) => {
                let ratio = if d.sd > 0.0 {
                    d.delta.abs() / d.sd
                } else {
                    f64::INFINITY
                };
                summary.push(' ');
                summary.push_str(&match d.verdict {
                    Verdict::Selection => format!(
                        "That is {} times what drift alone would typically give at this population size, so selection caused it.",
                        if ratio.is_finite() { format!("{ratio:.0}") } else { "many".into() }
                    ),
                    Verdict::Drift => "That is no more than drift alone would typically give at this population size, so the change is consistent with drift.".into(),
                    Verdict::Mixed => "That is somewhat more than drift alone would typically give, but not clearly so: selection and drift both may have played a part.".into(),
                });
                points.push(format!(
                    "Random drift moves a trait's genetic mean by about √(V_A·t/Nₑ): here Nₑ ≈ {} over {:.0} generations, an expected {:.3} latent units, against an actual change of {:.3} (z = {}).",
                    fmt_n(d.ne),
                    d.generations,
                    d.sd,
                    d.delta.abs(),
                    if d.z.is_finite() { format!("{:.1}", d.z) } else { "∞".into() }
                ));
                numbers.push(("Genetic change (latent)".into(), format!("{:+.3}", d.delta)));
                numbers.push(("Expected from drift (SD)".into(), format!("{:.3}", d.sd)));
                numbers.push((
                    "Change ÷ drift SD (z)".into(),
                    if d.z.is_finite() {
                        format!("{:.1}", d.z)
                    } else {
                        "∞".into()
                    },
                ));
                numbers.push(("Effective size Nₑ (harmonic mean)".into(), fmt_n(d.ne)));
                numbers.push(("Generations".into(), format!("{:.0}", d.generations)));
            }
            None => {
                summary.push_str(
                    " Too little genetic history was recorded to compare the change with drift.",
                );
            }
        }

        // What the trait does: the drivers of its selection gradient.
        let mut cause = None;
        if let Some(v) = self.mean_vital(species, from, to) {
            let dr = v.drivers[t as usize];
            let beta = v.beta[t as usize];
            numbers.push((
                "Selection gradient β (per latent unit)".into(),
                format!("{beta:+.3}"),
            ));
            for (k, name) in DRIVERS.iter().enumerate() {
                if dr[k].abs() > 1e-4 {
                    numbers.push((format!("  from {name}"), format!("{:+.3}", dr[k])));
                }
            }
            // The strongest driver of all, and the strongest pushing the way it moved.
            let strongest = (0..N_DRIVERS)
                .max_by(|&a, &b| dr[a].abs().total_cmp(&dr[b].abs()))
                .filter(|&k| dr[k].abs() > 1e-4);
            let along = (0..N_DRIVERS)
                .filter(|&k| dir * dr[k] as f64 > 1e-4)
                .max_by(|&a, &b| (dir * dr[a] as f64).total_cmp(&(dir * dr[b] as f64)));
            let against = (0..N_DRIVERS)
                .filter(|&k| dir * (dr[k] as f64) < -1e-4)
                .max_by(|&a, &b| (-dir * dr[a] as f64).total_cmp(&(-dir * dr[b] as f64)));
            let how = if v.modelled {
                "In the fitness model"
            } else {
                "Among the animals that died"
            };
            if let Some(k) = along {
                if verdict != Some(Verdict::Drift) {
                    cause = Some(DRIVERS[k].to_string());
                    summary.push_str(&format!(
                        " The main pressure is {}: animals with {more} {label} {}.",
                        DRIVERS[k],
                        benefit(k)
                    ));
                }
                points.push(format!(
                    "{how}, {} pushes {label} {} hardest (β = {:+.3}): animals with {more} {label} {}.",
                    DRIVERS[k],
                    if dir >= 0.0 { "up" } else { "down" },
                    dr[k],
                    benefit(k)
                ));
            }
            if let Some(k) = against {
                points.push(format!(
                    "It is held back by {}: animals with {more} {label} {} (β = {:+.3}).",
                    DRIVERS[k],
                    cost(k),
                    dr[k]
                ));
            }
            if let Some(k) = strongest {
                if Some(k) != along && Some(k) != against {
                    points.push(format!("What {label} affects most is {}.", DRIVERS[k]));
                }
            }
            if verdict != Some(Verdict::Selection) && dir * (beta as f64) < -0.01 {
                points.push(format!(
                    "Selection pushes {label} the other way (β = {beta:+.3}), but drift outweighed it."
                ));
            }
            if strongest.is_none() {
                points.push(format!(
                    "No hazard or part of reproduction depends on {label} at present: it is free to drift."
                ));
            }
            let total: f32 = v.hazard.iter().sum();
            if total > 0.0 {
                let (k, h) = v
                    .hazard
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(k, h)| (k, *h))
                    .unwrap_or((0, 0.0));
                points.push(format!(
                    "The species' main cause of death is {} ({:.0}% of the hazard from these causes).",
                    DRIVERS[k],
                    100.0 * h / total
                ));
            }
        } else {
            points.push("No hazard data were recorded for this period (it began before this save kept them).".into());
        }
        Why {
            kind: "trait".into(),
            species,
            year: to,
            summary,
            points,
            numbers,
            verdict: verdict.map(|v| v.key().to_string()),
            cause,
            chart: Some(format!("trait:{}", t.info().key)),
        }
    }

    /// Why a species' population fell (a crash) or died out: the hazard that grew most
    /// between the population's peak (since its last crash) and now, or the one that
    /// stayed high, matched to a cause.
    pub(crate) fn explain_decline(&self, species: u32, extinct: bool) -> Why {
        let info = self.species.get(species);
        let name = &info.common_name;
        let year = self.year();
        let kind = if extinct { "extinction" } else { "crash" };
        let what = if extinct { "died out" } else { "crashed" };
        let mut points = vec![];
        let mut numbers = vec![];
        let since = self
            .events
            .iter()
            .rev()
            .find(|e| {
                e.kind == EventKind::Crash && e.species == Some(species) && e.year < year - 0.5
            })
            .map_or(f64::NEG_INFINITY, |e| e.year);
        let v: Vec<&Demog> = self
            .insight
            .demog
            .get(&species)
            .map(|s| s.iter().filter(|d| d.year >= since).collect())
            .unwrap_or_default();
        if v.len() < 2 {
            return Why {
                kind: kind.into(),
                species,
                year,
                summary: format!("{name} {what}, but too little history was recorded to say why."),
                points,
                numbers,
                verdict: None,
                cause: None,
                chart: Some("population".into()),
            };
        }
        // The peak and present numbers, from the metrics series (vitals describe the
        // start of each year's step).
        let counts: Vec<(f64, u32)> = self
            .metrics
            .series
            .get(&species)
            .map(|m| {
                m.iter()
                    .filter(|x| x.year >= since)
                    .map(|x| (x.year, x.n))
                    .collect()
            })
            .unwrap_or_default();
        let (peak_year, peak_n) = counts
            .iter()
            .copied()
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.total_cmp(&a.0)))
            .unwrap_or((v[0].year, v[0].n));
        let now_n = counts.last().map_or(0, |x| x.1);
        // Hazards: "before" around the peak, "now" the last three samples, "during"
        // everything after the peak.
        let tail = 3.min(v.len() - 1);
        let head = &v[..v.len() - tail];
        let peak = head
            .iter()
            .rposition(|d| d.year <= peak_year + 1.0)
            .unwrap_or(0);
        let before = mean_demog(&head[peak.saturating_sub(1)..(peak + 2).min(head.len())]);
        let now = mean_demog(&v[v.len() - tail..]);
        let during = mean_demog(&v[peak + 1..]);
        let t0 = v[peak.saturating_sub(1)].year.min(peak_year);
        numbers.push((
            "Population".into(),
            format!(
                "{} (year {:.0}) → {}",
                fmt_n(peak_n as f64),
                peak_year,
                fmt_n(now_n as f64)
            ),
        ));
        for k in 0..6 {
            if before.hazard[k] > 1e-3 || now.hazard[k] > 1e-3 {
                numbers.push((
                    format!("Hazard: {}", DRIVERS[k]),
                    format!("{:.3} → {:.3} per year", before.hazard[k], now.hazard[k]),
                ));
            }
        }
        if before.food >= 0.0 && now.food >= 0.0 {
            numbers.push((
                "Food (intake ÷ need)".into(),
                format!("{:.2} → {:.2}", before.food, now.food),
            ));
        }
        if before.fecundity >= 0.0 && now.fecundity >= 0.0 {
            numbers.push((
                "Surviving young per female per year".into(),
                format!("{:.2} → {:.2}", before.fecundity, now.fecundity),
            ));
        }
        let dclimate = now.climate - before.climate;
        if dclimate.abs() >= 0.05 {
            numbers.push((
                "Global temperature change".into(),
                format!("{dclimate:+.1} °C"),
            ));
        }

        // Interventions since the peak.
        let mut intervention = None;
        let mut climate_intervention = false;
        for e in self.events.iter().rev() {
            if e.year < t0 - 1.0 {
                break;
            }
            if e.kind != EventKind::Intervention {
                continue;
            }
            if e.text.starts_with("Plague") && e.species == Some(species) {
                intervention = Some(format!("a plague you released (year {:.0})", e.year));
            } else if e.text.starts_with("Meteor") {
                intervention.get_or_insert(format!("a meteor impact (year {:.0})", e.year));
            } else if e.text.starts_with("Climate shift") {
                climate_intervention = true;
            }
        }
        let rises: Vec<f32> = (0..6).map(|k| now.hazard[k] - before.hazard[k]).collect();
        let (mut k, rise) = (0..6)
            .map(|k| (k, rises[k]))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, 0.0));
        // No hazard rose: the one that stayed highest while the population fell.
        let persistent = rise <= 0.02;
        if persistent {
            k = (0..6)
                .max_by(|&a, &b| during.hazard[a].total_cmp(&during.hazard[b]))
                .unwrap_or(0);
        }
        let high = persistent && during.hazard[k] > 0.1;
        let fec_ratio = if before.fecundity > 0.0 && now.fecundity >= 0.0 {
            now.fecundity / before.fecundity
        } else {
            1.0
        };
        let rising = |what: &str| {
            if persistent {
                format!("persistently high {what}")
            } else {
                format!("rising {what}")
            }
        };
        let iv = if climate_intervention {
            ", your intervention"
        } else {
            ""
        };
        let (cause, because) = if let Some(iv) = intervention {
            ("intervention".to_string(), format!("an intervention: {iv}"))
        } else if persistent && fec_ratio < 0.6 {
            (
                "reproduction".to_string(),
                format!(
                    "fewer young surviving (from {:.2} to {:.2} per female per year), while deaths did not rise",
                    before.fecundity, now.fecundity
                ),
            )
        } else if persistent && !high {
            (
                "unknown".to_string(),
                "no clear single cause: no hazard rose or stayed high".to_string(),
            )
        } else {
            match k {
                2 => {
                    let (pred, new) = self.likely_predator(species, t0);
                    let who = match (&pred, new) {
                        (Some(p), true) => format!("a new predator, {p}"),
                        (Some(p), false) if persistent => format!("predation by {p}"),
                        (Some(p), false) => format!("more predators ({p})"),
                        (None, _) => rising("predation"),
                    };
                    ("predator".to_string(), who)
                }
                0 if dclimate.abs() >= 0.5 => (
                    "climate".to_string(),
                    format!("a climate shift ({dclimate:+.1} °C{iv}) that cut its food"),
                ),
                0 => {
                    let outgrew = before.food >= 0.0 && now.food < before.food;
                    (
                        "food".to_string(),
                        format!(
                            "a food shortage{}",
                            if outgrew {
                                ": the population outgrew its food supply"
                            } else if persistent {
                                ": starvation stayed high"
                            } else {
                                ""
                            }
                        ),
                    )
                }
                1 | 3 | 4 if dclimate.abs() >= 0.5 => (
                    "climate".to_string(),
                    format!("a climate shift ({dclimate:+.1} °C{iv}) that raised {}", DRIVERS[k]),
                ),
                5 => ("ageing".to_string(), "an ageing population with few young to replace it".to_string()),
                _ => (
                    DRIVERS[k].to_string(),
                    format!(
                        "{} (with no global climate change, the species likely spread into harsher places)",
                        rising(DRIVERS[k])
                    ),
                ),
            }
        };
        let mut summary = format!("{name} {what} mainly because of {because}.");
        if cause != "intervention" && cause != "reproduction" && cause != "unknown" {
            summary.push_str(&if persistent {
                format!(
                    " The hazard from {} stayed at about {:.2} per year as numbers fell.",
                    DRIVERS[k], during.hazard[k]
                )
            } else {
                format!(
                    " The hazard from {} grew most, from {:.3} to {:.3} per year.",
                    DRIVERS[k], before.hazard[k], now.hazard[k]
                )
            });
        }
        for j in 0..6 {
            if j != k && rises[j] > 0.02 {
                points.push(format!(
                    "{} also rose ({:+.3} per year).",
                    capitalise(DRIVERS[j]),
                    rises[j]
                ));
            }
        }
        if now.food >= 0.0 && now.food < 0.8 {
            points.push(format!(
                "Animals got only {:.0}% of the food they need.",
                100.0 * now.food
            ));
        }
        if peak_n < 100 {
            points.push(
                "With so few animals, chance alone (demographic stochasticity) can finish off a population."
                    .into(),
            );
        }
        if v.iter().any(|d| !d.modelled) {
            points.push(
                "Some hazards were measured from the causes of death in Individual mode.".into(),
            );
        }
        Why {
            kind: kind.into(),
            species,
            year,
            summary,
            points,
            numbers,
            verdict: None,
            cause: Some(cause),
            chart: Some("population".into()),
        }
    }

    /// The meat-eating species that most likely preys on `species`, and whether it is
    /// new (appeared since `since`).
    fn likely_predator(&self, species: u32, since: f64) -> (Option<String>, bool) {
        let mut best: Option<(f64, String, bool)> = None;
        for s in &self.species.list {
            if s.id == species {
                continue;
            }
            let Some(v) = self.metrics.series.get(&s.id) else {
                continue;
            };
            let Some(last) = v.iter().rev().find(|x| x.n > 0) else {
                continue;
            };
            let Ok(m) = <[f32; N_TRAITS]>::try_from(last.mean.clone()) else {
                continue;
            };
            let p = Phenotype(m);
            if derive(&p, p.get(Trait::BodyMass)).meat_eff < 0.3 {
                continue;
            }
            let first = v.iter().find(|x| x.n > 0).map_or(last.year, |x| x.year);
            let then = v
                .iter()
                .rev()
                .find(|x| x.year <= since)
                .map_or(0.0, |x| x.n as f64);
            let new = first >= since - 5.0;
            let growth = last.n as f64 - then;
            let score = if new { f64::INFINITY } else { growth };
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, s.common_name.clone(), new));
            }
        }
        match best {
            Some((_, n, new)) => (Some(n), new),
            None => (None, false),
        }
    }

    /// Why a new species split off: the cause, the barrier, and which isolation factor
    /// contributed most. `daughter` and `main` are the two groups' population summaries.
    pub(crate) fn explain_speciation(
        &self,
        daughter_id: u32,
        parent_id: u32,
        allopatric: bool,
        daughter: &[&PopSummary],
        main: &[&PopSummary],
    ) -> Why {
        let d_name = self.species.get(daughter_id).common_name.clone();
        let p_name = self.species.get(parent_id).common_name.clone();
        let mut points = vec![];
        let mut numbers = vec![];
        let barrier = if allopatric {
            let b = self.barrier_between(
                &daughter.iter().map(|p| p.patch).collect::<Vec<_>>(),
                &main.iter().map(|p| p.patch).collect::<Vec<_>>(),
            );
            points.push(format!(
                "The two ranges share no land border: they are separated by {b}, so little or no gene flow joined them."
            ));
            Some(b)
        } else {
            points.push(
                "The two ranges still touch: they diverged despite meeting along a contact zone."
                    .into(),
            );
            None
        };
        let mut summary = if allopatric {
            format!(
                "{d_name} split from {p_name} by allopatric speciation: their populations were cut off from each other by {}.",
                barrier.clone().unwrap_or_default()
            )
        } else {
            format!(
                "{d_name} split from {p_name} without a geographic barrier (parapatric or sympatric speciation)."
            )
        };
        let (Some(a), Some(b)) = (
            crate::species::merge_summaries(daughter),
            crate::species::merge_summaries(main),
        ) else {
            return self.speciation_why(daughter_id, allopatric, summary, points, numbers);
        };
        let (via, mate, time) = crate::species::compatibility_parts(
            &a,
            &b,
            self.seed,
            self.params.dmi_probability,
            self.params.dmi_cost,
        );
        let factors = [
            (
                "hybrid incompatibilities (DMI)",
                via,
                "hybrids between them would be less viable",
            ),
            (
                "mate choice",
                mate,
                "they rarely accept each other as mates",
            ),
            (
                "breeding time",
                time,
                "they breed at different times of year",
            ),
        ];
        let iso: Vec<f64> = factors.iter().map(|f| -f.1.max(1e-12).ln()).collect();
        let total: f64 = iso.iter().sum();
        numbers.push((
            "Compatibility (1 = fully compatible)".into(),
            format!("{:.2}", via * mate * time),
        ));
        for (f, x) in factors.iter().zip(&iso) {
            numbers.push((
                format!("  {}", capitalise(f.0)),
                format!(
                    "{:.2}{}",
                    f.1,
                    if total > 0.0 {
                        format!(" ({:.0}% of the isolation)", 100.0 * x / total)
                    } else {
                        String::new()
                    }
                ),
            ));
        }
        if total > 0.0 {
            let best = (0..3)
                .max_by(|&i, &j| iso[i].total_cmp(&iso[j]))
                .unwrap_or(0);
            summary.push_str(&format!(
                " Reproductive isolation comes mostly from {}: {}.",
                factors[best].0, factors[best].2
            ));
        }
        self.speciation_why(daughter_id, allopatric, summary, points, numbers)
    }

    fn speciation_why(
        &self,
        species: u32,
        allopatric: bool,
        summary: String,
        points: Vec<String>,
        numbers: Vec<(String, String)>,
    ) -> Why {
        Why {
            kind: "speciation".into(),
            species,
            year: self.year(),
            summary,
            points,
            numbers,
            verdict: None,
            cause: Some(
                if allopatric {
                    "allopatric"
                } else {
                    "parapatric"
                }
                .into(),
            ),
            chart: Some("tree".into()),
        }
    }

    /// What lies between two groups of patches: along the straight line between their
    /// nearest patch centres, water or mountains if any, otherwise a gap in the range.
    fn barrier_between(&self, a: &[u32], b: &[u32]) -> String {
        let pw = self.patches_w();
        let ps = self.params.patch_size as f64;
        let w = self.world.w as f64;
        let centre = |p: u32| {
            (
                ((p as usize % pw) as f64 + 0.5) * ps,
                ((p as usize / pw) as f64 + 0.5) * ps,
            )
        };
        let mut best: Option<(f64, (f64, f64), f64, f64)> = None;
        for &pa in a {
            for &pb in b {
                let (ax, ay) = centre(pa);
                let (bx, by) = centre(pb);
                let mut dx = bx - ax;
                if dx > w / 2.0 {
                    dx -= w;
                } else if dx < -w / 2.0 {
                    dx += w;
                }
                let dy = by - ay;
                let d2 = dx * dx + dy * dy;
                if best.is_none_or(|x| d2 < x.0) {
                    best = Some((d2, (ax, ay), dx, dy));
                }
            }
        }
        let Some((d2, (ax, ay), dx, dy)) = best else {
            return "distance".into();
        };
        let steps = d2.sqrt().ceil().max(1.0) as usize;
        let (mut water, mut mountain) = (0, 0);
        for i in 0..=steps {
            let f = i as f64 / steps as f64;
            let x = (ax + f * dx).rem_euclid(w) as f32;
            let y = (ay + f * dy).clamp(0.0, self.world.h as f64 - 1.0) as f32;
            let t = self.world.tile_of(x, y);
            if !self.world.is_land(t) {
                water += 1;
            } else if self.world.elevation[t] > 2000.0 {
                mountain += 1;
            }
        }
        let km = d2.sqrt() * self.world.tile_km;
        if water > 0 {
            format!(
                "open water (about {:.0} km between the nearest populations)",
                km
            )
        } else if mountain > 0 {
            "mountains".into()
        } else {
            format!(
                "a gap in the range (about {:.0} km of unoccupied land between them)",
                km
            )
        }
    }
}

/// Appends, dropping every other sample past `GENETIC_CAP` but always keeping the first
/// and last (they bound every interval tested).
fn decimated_push<T>(v: &mut Vec<T>, x: T) {
    v.push(x);
    if v.len() > GENETIC_CAP {
        let last = v.len() - 1;
        let kept: Vec<T> = v
            .drain(..)
            .enumerate()
            .filter(|(i, _)| i % 2 == 0 || *i == last)
            .map(|(_, g)| g)
            .collect();
        *v = kept;
    }
}

fn mean_demog(s: &[&Demog]) -> Demog {
    let k = s.len().max(1) as f32;
    let mut out = Demog {
        year: s.first().map_or(0.0, |d| d.year),
        n: (s.iter().map(|d| d.n as f64).sum::<f64>() / k as f64) as u32,
        hazard: [0.0; 6],
        food: 0.0,
        fecundity: 0.0,
        climate: 0.0,
        modelled: s.iter().all(|d| d.modelled),
    };
    for d in s {
        for h in 0..6 {
            out.hazard[h] += d.hazard[h] / k;
        }
        out.food += d.food / k;
        out.fecundity += d.fecundity / k;
        out.climate += d.climate / k;
    }
    // "Not measured" stays negative.
    if s.iter().any(|d| d.food < 0.0) {
        out.food = -1.0;
    }
    if s.iter().any(|d| d.fecundity < 0.0) {
        out.fecundity = -1.0;
    }
    out
}

fn mean_of(s: &[&Vital]) -> Option<Vital> {
    let first = s.first()?;
    let k = s.len() as f32;
    let mut out = (*first).clone();
    out.year = first.year;
    let mut n = 0f64;
    out.hazard = [0.0; 6];
    out.food = 0.0;
    out.fecundity = 0.0;
    out.climate = 0.0;
    out.beta = vec![0.0; N_TRAITS];
    out.drivers = vec![[0.0; N_DRIVERS]; N_TRAITS];
    for v in s {
        n += v.n as f64;
        for h in 0..6 {
            out.hazard[h] += v.hazard[h] / k;
        }
        out.food += v.food / k;
        out.fecundity += v.fecundity / k;
        out.climate += v.climate / k;
        for t in 0..N_TRAITS.min(v.beta.len()) {
            out.beta[t] += v.beta[t] / k;
            for d in 0..N_DRIVERS {
                out.drivers[t][d] += v.drivers[t][d] / k;
            }
        }
    }
    out.n = (n / s.len() as f64) as u32;
    // "Not measured" stays negative.
    if s.iter().any(|v| v.food < 0.0) {
        out.food = -1.0;
    }
    if s.iter().any(|v| v.fecundity < 0.0) {
        out.fecundity = -1.0;
    }
    Some(out)
}

fn fmt_val(x: f32) -> String {
    if x.abs() >= 100.0 {
        format!("{x:.0}")
    } else if x.abs() >= 1.0 {
        format!("{x:.2}")
    } else {
        format!("{x:.3}")
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(points: &[(f64, f32, f32, f32)]) -> Vec<GenTrack> {
        points
            .iter()
            .map(|&(year, mean, va, ne)| GenTrack {
                year,
                mean: vec![mean; N_TRAITS],
                va: vec![va; N_TRAITS],
                ne,
                generation: 1.0,
            })
            .collect()
    }

    #[test]
    fn verdict_thresholds() {
        assert_eq!(verdict(0.5), Verdict::Drift);
        assert_eq!(verdict(1.99), Verdict::Drift);
        assert_eq!(verdict(2.5), Verdict::Mixed);
        assert_eq!(verdict(3.0), Verdict::Selection);
        assert_eq!(verdict(f64::INFINITY), Verdict::Selection);
    }

    #[test]
    fn drift_variance_is_va_t_over_ne() {
        // V_A = 0.04, Nₑ = 100, 100 generations: SD = √(0.04·100/100) = 0.2.
        let t: Vec<_> = (0..=100).map(|y| (y as f64, 0.0, 0.04, 100.0)).collect();
        let mut t = track(&t);
        t.last_mut().unwrap().mean = vec![0.3; N_TRAITS];
        let d = drift_test(&t, 0, 0.0, 100.0).unwrap();
        assert!((d.sd - 0.2).abs() < 1e-6, "{}", d.sd);
        assert!((d.z - 1.5).abs() < 1e-5);
        assert_eq!(d.verdict, Verdict::Drift);
        assert!((d.ne - 100.0).abs() < 1e-6 && (d.generations - 100.0).abs() < 1e-9);
    }

    #[test]
    fn large_change_in_big_population_is_selection() {
        // Same change, but Nₑ = 10 000: drift SD 0.02, z = 15.
        let t = track(&[(0.0, 0.0, 0.04, 1e4), (100.0, 0.3, 0.04, 1e4)]);
        let d = drift_test(&t, 3, 0.0, 100.0).unwrap();
        assert_eq!(d.verdict, Verdict::Selection);
        assert!((d.z - 15.0).abs() < 1e-3);
        // And a middling one is mixed.
        let t = track(&[(0.0, 0.0, 0.04, 100.0), (100.0, 0.5, 0.04, 100.0)]);
        assert_eq!(
            drift_test(&t, 0, 0.0, 100.0).unwrap().verdict,
            Verdict::Mixed
        );
    }

    #[test]
    fn longer_generations_mean_less_drift() {
        let mut t = track(&[(0.0, 0.0, 0.04, 100.0), (100.0, 0.3, 0.04, 100.0)]);
        for g in t.iter_mut() {
            g.generation = 4.0;
        }
        let d = drift_test(&t, 0, 0.0, 100.0).unwrap();
        assert!((d.generations - 25.0).abs() < 1e-9);
        assert!((d.sd - 0.1).abs() < 1e-6);
        assert_eq!(d.verdict, Verdict::Selection);
    }

    #[test]
    fn no_test_without_history_or_variance() {
        let t = track(&[(0.0, 0.0, 0.04, 100.0)]);
        assert!(drift_test(&t, 0, 0.0, 10.0).is_none());
        // No genetic variance: any change at all is not drift.
        let t = track(&[(0.0, 0.0, 0.0, 100.0), (10.0, 0.1, 0.0, 100.0)]);
        assert_eq!(
            drift_test(&t, 0, 0.0, 10.0).unwrap().verdict,
            Verdict::Selection
        );
        let t = track(&[(0.0, 0.1, 0.0, 100.0), (10.0, 0.1, 0.0, 100.0)]);
        assert_eq!(
            drift_test(&t, 0, 0.0, 10.0).unwrap().verdict,
            Verdict::Drift
        );
    }

    #[test]
    fn decimation_keeps_both_ends() {
        let mut i = Insight::new();
        for y in 0..(GENETIC_CAP + 1) {
            i.push_genetic(
                0,
                GenTrack {
                    year: y as f64,
                    mean: vec![],
                    va: vec![],
                    ne: 1.0,
                    generation: 1.0,
                },
            );
        }
        let v = &i.genetic[&0];
        assert!(v.len() <= GENETIC_CAP);
        assert_eq!(v[0].year, 0.0);
        assert_eq!(v.last().unwrap().year, GENETIC_CAP as f64);
    }
}
