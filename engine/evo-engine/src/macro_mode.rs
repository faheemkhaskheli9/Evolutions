//! Macro mode (Mode M, TRD §5.12): deep time in 1,000-year steps.
//!
//! The state is the same as in Population mode (each species in each habitat patch is
//! a deme with age × sex counts and allele frequencies), so switching between the two
//! modes changes nothing (TRD TR-X3/TR-X4 hold exactly). What changes is how a step is
//! taken. Each step:
//!
//! 1. **Demography.** The Population-mode age-structured model (the same ecology,
//!    physiology and dispersal) is projected in expected values for
//!    [`PROJECTION_YEARS`] years with the traits held fixed, so abundance and range
//!    settle near their equilibrium. A decline that is not caused by crowding (the
//!    deme is well fed) is carried on for the rest of the step. Counts are then drawn
//!    with Poisson noise, so small demes can die out.
//! 2. **Traits.** Each trait mean follows an Ornstein–Uhlenbeck process (TRD TR-MM2):
//!    it relaxes towards the optimum of the Population-mode fitness surface at rate
//!    V_A·|curvature| (Lande 1976), exchanges genes with neighbouring demes at the
//!    migration rates of step 1, and drifts with variance V_A/Nₑ per year.
//! 3. **Alleles.** Allele frequencies take one step of gene flow, Wright–Fisher
//!    drift over the whole step (a Dirichlet draw with F = 1 − e^(−T/2Nₑ)),
//!    selection against incompatible combinations and mutation, and are then tilted
//!    so that each deme's trait means match step 2.
//!
//! What Macro mode leaves out: year-to-year fluctuations (population cycles, a bad
//! winter), the order of events within a step, and anything faster than the step
//! itself. Interventions and climate changes take effect at the next step.

use crate::genetics::{GenomeArch, LocusKind, NO_TRAIT};
use crate::physiology::{derive, gompertz_hazard, Derived, MassTerms};
use crate::population::{
    annual_heat, eco_for_mean, ecology, emigration_rate, fed_rates, fed_rates_heat, fitness_parts,
    gradient, merge_sorted, neighbour_patches, normalise, prune_rare, select_dmi, year_heat, Deme,
    DemeSelection, Gradient, Unit, YearHeat, AGE_CLASSES, FITNESS_NEUTRAL, GRADIENT_DELTA,
    MAX_ALLELES_PER_LOCUS, MAX_NEW_FUNCTIONAL, MAX_NEW_NEUTRAL,
};
use crate::rng::{stream, Rng};
use crate::sim::Simulation;
use crate::traits::{Phenotype, Trait, N_TRAITS};
use std::collections::HashMap;

/// Simulated years per Macro-mode step.
pub const MACRO_STEP_YEARS: f64 = 1000.0;
/// Years of demography projected at the start of each step.
pub const PROJECTION_YEARS: usize = 40;
/// Sub-steps of the trait-mean integration within a step; the fitness surface is
/// measured again at the start of each.
const TRAIT_SUBSTEPS: usize = 10;
/// Gauss–Seidel sweeps per sub-step (the implicit migration–selection balance).
const SWEEPS: usize = 8;
/// Largest move of a trait mean's optimum from the current mean, in latent units: the
/// fitness surface is measured near the mean, so it is not trusted further than this.
pub const MAX_SHIFT: f32 = 0.5;
/// Projected flows smaller than this (animals per year) do not found a new deme.
const MIN_COLONISTS: f64 = 0.25;
/// Largest share of a deme's genes replaced by immigrants in one step.
const MAX_GENE_FLOW: f32 = 0.8;
/// Passes of the allele-frequency tilt that matches trait means to their targets.
const TILT_PASSES: usize = 3;
/// New mutations drawn per trait locus per step, of which the one most favoured by
/// selection is kept.
const MUTANT_CANDIDATES: u64 = 8;
const STREAM: u64 = stream::POPULATION ^ 0x4d41_4352_4f00; // "MACRO"

/// One deme during a step's demographic projection.
struct Cell {
    species: u32,
    arch: u16,
    patch: u32,
    /// The deme (before the step) whose traits and alleles this one carries: itself,
    /// or the deme its founders came from.
    src: usize,
    f: [f64; AGE_CLASSES],
    m: [f64; AGE_CLASSES],
    heat: YearHeat,
    /// Births in the last projected year.
    births: f64,
    /// Σ 1/Nₑ and the number of years it sums over (harmonic mean Nₑ).
    inv_ne: f64,
    ne_years: u32,
    /// Total at the end of each of the last 11 projected years (oldest first).
    hist: Vec<f64>,
    food: f32,
}

impl Cell {
    fn total(&self) -> f64 {
        self.f.iter().sum::<f64>() + self.m.iter().sum::<f64>()
    }

    fn ne(&self) -> f64 {
        if self.ne_years == 0 || self.inv_ne <= 0.0 {
            return self.total().max(2.0);
        }
        (self.ne_years as f64 / self.inv_ne).max(2.0)
    }
}

/// Gamma(shape, 1) variate (Marsaglia & Tsang 2000; shape < 1 by boosting).
pub fn gamma(rng: &mut Rng, shape: f64) -> f64 {
    if shape <= 0.0 {
        return 0.0;
    }
    if shape < 1.0 {
        let u = rng.f64().max(1e-300);
        return gamma(rng, shape + 1.0) * u.powf(1.0 / shape);
    }
    if shape > 1e5 {
        return (shape + shape.sqrt() * rng.normal()).max(0.0);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = rng.normal();
        let v = 1.0 + c * x;
        if v <= 0.0 {
            continue;
        }
        let v = v * v * v;
        let u = rng.f64();
        if u < 1.0 - 0.0331 * x.powi(4) || u.max(1e-300).ln() < 0.5 * x * x + d * (1.0 - v + v.ln())
        {
            return d * v;
        }
    }
}

/// Wright–Fisher drift over many generations in one draw: frequencies become
/// Dirichlet(p·(1/F − 1)), which has the right mean (p) and variance (p(1 − p)F).
/// Alleles left rarer than `min_freq` are lost.
pub fn dirichlet_drift(pool: &mut Vec<(u32, f32)>, f: f64, min_freq: f32, rng: &mut Rng) {
    if pool.len() < 2 || f <= 1e-9 {
        return;
    }
    let alpha = if f >= 1.0 - 1e-12 {
        1e-12
    } else {
        1.0 / f - 1.0
    };
    let draws: Vec<f64> = pool
        .iter()
        .map(|&(_, p)| gamma(rng, alpha * p as f64))
        .collect();
    let s: f64 = draws.iter().sum();
    if !(s > 0.0 && s.is_finite()) {
        // Every share underflowed: one allele is fixed, with probability p.
        let w: Vec<f64> = pool.iter().map(|x| x.1 as f64).collect();
        let keep = pool[rng.weighted(&w)].0;
        pool.retain(|x| x.0 == keep);
        pool[0].1 = 1.0;
        return;
    }
    for (x, g) in pool.iter_mut().zip(&draws) {
        x.1 = (g / s) as f32;
    }
    let best = pool
        .iter()
        .map(|x| x.1)
        .fold(0.0f32, f32::max)
        .min(min_freq);
    pool.retain(|x| x.1 >= best);
    normalise(pool);
}

/// Drift of a range of demes that exchange genes: per locus, the pooled frequencies
/// (weighted by abundance) take one `dirichlet_drift` draw with the range's `ne`, and
/// each deme's frequencies change by as much as the pool's (an allele lost from the
/// range is lost everywhere; one fixed in the range is fixed everywhere).
fn range_drift(demes: &mut [Deme], xs: &[usize], ne: f64, years: f64, rng: &mut Rng) {
    let f = 1.0 - (-years / (2.0 * ne)).exp();
    let min_freq = (0.25 / ne) as f32;
    let wsum: f64 = xs.iter().map(|&x| demes[x].total() as f64).sum();
    if wsum <= 0.0 {
        return;
    }
    let nloci = demes[xs[0]].freqs.len();
    let weights: Vec<f64> = xs.iter().map(|&x| demes[x].total() as f64 / wsum).collect();
    let mut all: Vec<(u32, f64)> = Vec::new();
    let mut pooled: Vec<(u32, f64)> = Vec::new();
    for l in 0..nloci {
        // Pooled frequencies, sorted by allele id.
        all.clear();
        for (k, &x) in xs.iter().enumerate() {
            let w = weights[k];
            all.extend(demes[x].freqs[l].iter().map(|&(a, p)| (a, w * p as f64)));
        }
        all.sort_unstable_by_key(|e| e.0);
        pooled.clear();
        for &(a, p) in &all {
            match pooled.last_mut() {
                Some(last) if last.0 == a => last.1 += p,
                _ => pooled.push((a, p)),
            }
        }
        if pooled.len() < 2 {
            continue;
        }
        let mut pool: Vec<(u32, f32)> = pooled.iter().map(|&(a, p)| (a, p as f32)).collect();
        dirichlet_drift(&mut pool, f, min_freq, rng);
        // `pool` keeps the order of `pooled` (some alleles may be gone).
        for &x in xs {
            let fr = &mut demes[x].freqs[l];
            for e in fr.iter_mut() {
                let old = pooled[pooled.binary_search_by_key(&e.0, |q| q.0).unwrap()].1;
                e.1 = match pool.binary_search_by_key(&e.0, |q| q.0) {
                    Ok(i) => (e.1 as f64 + pool[i].1 as f64 - old).max(0.0) as f32,
                    Err(_) => 0.0,
                };
            }
            fr.retain(|e| e.1 > 0.0);
            if fr.is_empty() {
                fr.extend_from_slice(&pool);
            }
            normalise(fr);
        }
    }
}

/// Additive variance and mean of every latent trait in a deme (additive approximation,
/// as in `Deme::mean_latent`).
fn trait_moments(
    d: &Deme,
    arch: &GenomeArch,
    table: &crate::genetics::AlleleTable,
) -> ([f32; N_TRAITS], [f32; N_TRAITS]) {
    let mut mean = arch.base;
    let mut va = [0f32; N_TRAITS];
    for &li in &arch.trait_loci {
        let pool = &d.freqs[li];
        let mut m = [[0f32; 2]; 1];
        let mut ts = [NO_TRAIT; 2];
        for &(a, p) in pool {
            for (s, &(t, e)) in table.get(a).effects.iter().enumerate() {
                ts[s] = t;
                m[0][s] += p * e;
            }
        }
        for &(a, p) in pool {
            for (s, &(t, e)) in table.get(a).effects.iter().enumerate() {
                if t != NO_TRAIT {
                    va[t as usize] += 2.0 * p * (e - m[0][s]).powi(2);
                }
            }
        }
        for s in 0..2 {
            if ts[s] != NO_TRAIT {
                mean[ts[s] as usize] += 2.0 * m[0][s];
            }
        }
    }
    (mean, va)
}

/// Moves trait means by `shift` (latent units, per trait) by tilting allele
/// frequencies at trait loci: p ∝ p·exp(Σ c_t·(e − ē)), with c_t = shift_t / V_A,t,
/// which changes the mean by ≈ shift to first order.
fn tilt(d: &mut Deme, arch: &GenomeArch, table: &crate::genetics::AlleleTable, c: &[f32]) {
    for &li in &arch.trait_loci {
        let pool = &mut d.freqs[li];
        let mut mean_e = [0f32; 2];
        for &(a, p) in pool.iter() {
            let al = table.get(a);
            for s in 0..2 {
                mean_e[s] += p * al.effects[s].1;
            }
        }
        let mut any = false;
        for (a, p) in pool.iter_mut() {
            let al = table.get(*a);
            let mut x = 0.0;
            for s in 0..2 {
                let t = al.effects[s].0;
                if t != NO_TRAIT {
                    x += c[t as usize] * (al.effects[s].1 - mean_e[s]);
                }
            }
            if x != 0.0 {
                any = true;
                *p *= x.clamp(-3.0, 3.0).exp();
            }
        }
        if any {
            normalise(pool);
            pool.retain(|x| x.1 > 1e-7);
            normalise(pool);
        }
    }
}

impl Simulation {
    /// One Macro-mode step of [`MACRO_STEP_YEARS`] years (see the module notes).
    pub(crate) fn step_macro(&mut self) {
        let years = MACRO_STEP_YEARS as f32;
        let ctxs = self.patch_contexts();
        let bg = self.params.background_mortality as f32;
        let tick = self.minutes;
        let seed = self.seed;
        let pw = self.patches_w() as i64;
        let ph = self.patches_h() as i64;
        let kj_g = self.params.forage_kj_per_g as f32;
        let patch_km = self.params.patch_size as f32 * self.world.tile_km as f32;
        let old = std::mem::take(&mut self.demes);

        // Traits are fixed during the projection.
        let latents: Vec<[f32; N_TRAITS]> = old
            .iter()
            .map(|d| d.mean_latent(&self.archs[d.arch as usize], &self.alleles))
            .collect();
        let means: Vec<Phenotype> = latents.iter().map(Phenotype::from_latent).collect();
        let derived: Vec<Derived> = means
            .iter()
            .map(|p| derive(p, p.get(Trait::BodyMass)))
            .collect();
        let senescence: Vec<[f64; AGE_CLASSES]> = derived
            .iter()
            .map(|d| {
                std::array::from_fn(|age| (-gompertz_hazard(d, age as f32 + 0.5, 0.0)).exp() as f64)
            })
            .collect();
        let maturity: Vec<f32> = derived.iter().map(|d| d.maturity_days / 365.25).collect();

        // ---- 1. Demography ---------------------------------------------------------
        let mut cells: Vec<Cell> = Vec::with_capacity(old.len());
        let mut need: Vec<f32> = vec![];
        let mut fec: Vec<f32> = vec![];
        let mut where_is: HashMap<(u32, u32), usize> = HashMap::new();
        let new_cell = |src: usize, species: u32, arch: u16, patch: u32| -> (Cell, f32, f32) {
            let pc = &ctxs[patch as usize];
            let (nd, fd) = fed_rates(&means[src], &derived[src], pc, bg);
            (
                Cell {
                    species,
                    arch,
                    patch,
                    src,
                    f: [0.0; AGE_CLASSES],
                    m: [0.0; AGE_CLASSES],
                    heat: year_heat(&derived[src], pc),
                    births: 0.0,
                    inv_ne: 0.0,
                    ne_years: 0,
                    hist: vec![],
                    food: 1.0,
                },
                nd,
                fd,
            )
        };
        for (k, d) in old.iter().enumerate() {
            let (mut c, nd, fd) = new_cell(k, d.species, d.arch, d.patch);
            for a in 0..AGE_CLASSES {
                c.f[a] = d.females[a] as f64;
                c.m[a] = d.males[a] as f64;
            }
            where_is.insert((d.species, d.patch), cells.len());
            cells.push(c);
            need.push(nd);
            fec.push(fd);
        }
        // Flows of the last projected year: (from, to, animals per year).
        let mut flows: Vec<(usize, usize, f64)> = vec![];
        for y in 0..PROJECTION_YEARS {
            let units: Vec<Unit> = cells
                .iter()
                .map(|c| Unit {
                    patch: c.patch,
                    arch: c.arch,
                    n: c.total() as f32,
                })
                .collect();
            let cm: Vec<Phenotype> = cells.iter().map(|c| means[c.src]).collect();
            let cd: Vec<Derived> = cells.iter().map(|c| derived[c.src]).collect();
            let eco = ecology(&units, &cm, &cd, &need, &fec, &ctxs, kj_g);
            for (i, c) in cells.iter_mut().enumerate() {
                let n = c.total();
                if n <= 0.0 {
                    continue;
                }
                let pc = &ctxs[c.patch as usize];
                let a = annual_heat(&cm[i], &cd[i], &c.heat, pc, &eco[i], bg);
                let mat = maturity[c.src];
                let s_eco = (-a.hazard).exp() as f64;
                let (mut births, mut af, mut am) = (0.0, 0.0, 0.0);
                let mut nf = [0f64; AGE_CLASSES];
                let mut nm = [0f64; AGE_CLASSES];
                for age in 0..AGE_CLASSES {
                    let breeding = (age as f32 + 1.0 - mat).clamp(0.0, 1.0) as f64;
                    births += c.f[age] * breeding * a.fecundity as f64;
                    af += c.f[age] * breeding;
                    am += c.m[age] * breeding;
                    let s = s_eco * senescence[c.src][age];
                    let next = (age + 1).min(AGE_CLASSES - 1);
                    nf[next] += c.f[age] * s;
                    nm[next] += c.m[age] * s;
                }
                if am < 1e-9 {
                    births = 0.0;
                }
                let births = births.min(5e6);
                nf[0] = births / 2.0;
                nm[0] = births / 2.0;
                c.f = nf;
                c.m = nm;
                c.births = births;
                c.food = eco[i].food_ratio;
                if af > 0.0 && am > 0.0 {
                    let ne = (4.0 * af * am / (af + am)).min(n).max(2.0);
                    c.inv_ne += 1.0 / ne;
                    c.ne_years += 1;
                }
            }
            // Dispersal, in expected numbers, with young animals moving first.
            flows.clear();
            for (i, c) in cells.iter().enumerate() {
                let n = c.total();
                if n < 2.0 {
                    continue;
                }
                let rate = emigration_rate(&cm[i], eco[i].food_ratio, patch_km) as f64;
                let (targets, weights) = neighbour_patches(&ctxs, c.patch, pw, ph);
                let wsum: f64 = weights.iter().sum();
                if targets.is_empty() || wsum <= 0.0 {
                    continue;
                }
                let emig = n * rate * (wsum / 4.0).min(1.0);
                // As in Population mode, the year's emigrants leave in (at most) two
                // groups, each taking a few young animals from each age and sex class (on
                // average 2·(1 − 2^−count) from a class).
                let groups = targets.len().min(2) as f64;
                let per_group: f64 =
                    c.f.iter()
                        .chain(c.m.iter())
                        .map(|&k| 2.0 * (1.0 - 0.5f64.powf(k)))
                        .sum();
                let emig = groups * (emig / groups).min(per_group);
                for (t, w) in targets.iter().zip(&weights) {
                    let flow = emig * w / wsum;
                    let to = match where_is.get(&(c.species, *t)) {
                        Some(&j) => j,
                        None if flow >= MIN_COLONISTS => usize::MAX - *t as usize,
                        None => continue,
                    };
                    flows.push((i, to, flow));
                }
            }
            // New demes for colonised patches.
            for fl in flows.iter_mut() {
                if fl.1 > usize::MAX - (1 << 31) {
                    let patch = (usize::MAX - fl.1) as u32;
                    let src = &cells[fl.0];
                    let key = (src.species, patch);
                    let j = match where_is.get(&key) {
                        Some(&j) => j,
                        None => {
                            let (c, nd, fd) = new_cell(src.src, src.species, src.arch, patch);
                            where_is.insert(key, cells.len());
                            cells.push(c);
                            need.push(nd);
                            fec.push(fd);
                            cells.len() - 1
                        }
                    };
                    fl.1 = j;
                }
            }
            let mut moved: Vec<([f64; AGE_CLASSES], [f64; AGE_CLASSES], usize)> = vec![];
            for &(i, j, flow) in &flows {
                let c = &mut cells[i];
                let (mut mf, mut mm) = ([0f64; AGE_CLASSES], [0f64; AGE_CLASSES]);
                let mut left = flow;
                for age in 0..AGE_CLASSES {
                    if left <= 0.0 {
                        break;
                    }
                    let here = c.f[age] + c.m[age];
                    if here <= 0.0 {
                        continue;
                    }
                    let take = left.min(here);
                    let (tf, tm) = (take * c.f[age] / here, take * c.m[age] / here);
                    c.f[age] -= tf;
                    c.m[age] -= tm;
                    mf[age] = tf;
                    mm[age] = tm;
                    left -= take;
                }
                moved.push((mf, mm, j));
            }
            for (mf, mm, j) in moved {
                let c = &mut cells[j];
                for age in 0..AGE_CLASSES {
                    c.f[age] += mf[age];
                    c.m[age] += mm[age];
                }
            }
            if y + 11 >= PROJECTION_YEARS {
                for c in cells.iter_mut() {
                    let n = c.total();
                    c.hist.push(n);
                }
            }
        }
        // A decline in a well-fed deme is not crowding: it goes on for the rest of the step.
        let rest = years as f64 - PROJECTION_YEARS as f64;
        for c in cells.iter_mut() {
            if c.hist.len() < 11 || c.food < 0.95 {
                continue;
            }
            let (a, b) = (c.hist[0], c.hist[10]);
            if a <= 0.0 || b <= 0.0 {
                continue;
            }
            let rho = (b / a).powf(0.1);
            if rho < 0.999 {
                let k = rho.powf(rest);
                for age in 0..AGE_CLASSES {
                    c.f[age] *= k;
                    c.m[age] *= k;
                }
            }
        }

        // Counts with demographic noise; demes that come out empty are gone.
        let mut keep: Vec<usize> = vec![];
        let mut demes: Vec<Deme> = vec![];
        for (i, c) in cells.iter().enumerate() {
            let mut rng = Rng::keyed(
                seed,
                tick,
                ((c.species as u64) << 32) | c.patch as u64,
                STREAM,
            );
            let mut d = Deme {
                species: c.species,
                arch: c.arch,
                patch: c.patch,
                females: vec![0; AGE_CLASSES],
                males: vec![0; AGE_CLASSES],
                freqs: vec![],
                condition: c.food.min(1.0),
            };
            for age in 0..AGE_CLASSES {
                d.females[age] = rng.poisson(c.f[age]).min(u32::MAX as u64 / 4) as u32;
                d.males[age] = rng.poisson(c.m[age]).min(u32::MAX as u64 / 4) as u32;
            }
            if d.total() == 0 {
                continue;
            }
            d.freqs = old[c.src].freqs.clone();
            keep.push(i);
            demes.push(d);
        }
        // Rafting across water: over a step, each deme has this chance of sending a
        // breeding pair to a patch up to three away.
        let mut rafts: Vec<(usize, u32)> = vec![];
        for (x, &i) in keep.iter().enumerate() {
            let c = &cells[i];
            let disp = means[c.src].get(Trait::Dispersal) as f64;
            let p = 1.0 - (1.0 - 0.002 * disp).powf(years as f64);
            let mut rng = Rng::keyed(seed, tick, i as u64, STREAM ^ 0x55);
            if demes[x].total() < 2 || !rng.chance(p) {
                continue;
            }
            let (px, py) = (c.patch as i64 % pw, c.patch as i64 / pw);
            let dx = rng.below(7) as i64 - 3;
            let dy = rng.below(7) as i64 - 3;
            let ny = py + dy;
            if (0..ph).contains(&ny) {
                let t = (ny * pw + (px + dx).rem_euclid(pw)) as u32;
                let pc = &ctxs[t as usize];
                if pc.land.len() >= 2 && pc.plant_supply_g > 0.0 && t != c.patch {
                    rafts.push((x, t));
                }
            }
        }
        let mut deme_at: HashMap<(u32, u32), usize> = demes
            .iter()
            .enumerate()
            .map(|(x, d)| ((d.species, d.patch), x))
            .collect();
        for (x, t) in rafts {
            let key = (demes[x].species, t);
            match deme_at.get(&key) {
                Some(&j) => {
                    demes[j].females[1] += 1;
                    demes[j].males[1] += 1;
                }
                None => {
                    let src = &demes[x];
                    let mut d = Deme {
                        species: src.species,
                        arch: src.arch,
                        patch: t,
                        females: vec![0; AGE_CLASSES],
                        males: vec![0; AGE_CLASSES],
                        freqs: src.freqs.clone(),
                        condition: src.condition,
                    };
                    d.females[1] = 1;
                    d.males[1] = 1;
                    let c = &cells[keep[x]];
                    let (cell, nd, fd) = new_cell(c.src, c.species, c.arch, t);
                    deme_at.insert(key, demes.len());
                    demes.push(d);
                    keep.push(cells.len());
                    cells.push(cell);
                    need.push(nd);
                    fec.push(fd);
                }
            }
        }

        // ---- Ecology at the end of the projection --------------------------------------
        let units: Vec<Unit> = demes
            .iter()
            .map(|d| Unit {
                patch: d.patch,
                arch: d.arch,
                n: d.total() as f32,
            })
            .collect();
        let cm: Vec<Phenotype> = keep.iter().map(|&i| means[cells[i].src]).collect();
        let cd: Vec<Derived> = keep.iter().map(|&i| derived[cells[i].src]).collect();
        let kn: Vec<f32> = keep.iter().map(|&i| need[i]).collect();
        let kf: Vec<f32> = keep.iter().map(|&i| fec[i]).collect();
        let eco = ecology(&units, &cm, &cd, &kn, &kf, &ctxs, kj_g);
        let nd = demes.len();
        let mut diag = Vec::with_capacity(nd);
        let mut selection = Vec::with_capacity(nd);
        let mut va_eff = vec![[0f32; N_TRAITS]; nd];
        let mut z = vec![[0f32; N_TRAITS]; nd];
        let mut ne = vec![0f64; nd];
        for (x, d) in demes.iter().enumerate() {
            let c = &cells[keep[x]];
            let pc = &ctxs[d.patch as usize];
            let base = annual_heat(&cm[x], &cd[x], &c.heat, pc, &eco[x], bg);
            diag.push((d.species, d.patch, d.total(), base));
            let adult = cm[x].get(Trait::BodyMass);
            let mass = MassTerms::new(adult, adult);
            let latent = latents[c.src];
            let Gradient { beta, drivers, .. } = gradient(
                &cm[x],
                &latent,
                pc,
                &eco[x],
                bg,
                base.fecundity,
                &c.heat,
                &mass,
            );
            selection.push(DemeSelection {
                species: d.species,
                n: d.total(),
                beta,
                drivers,
            });
            ne[x] = c.ne().min(d.total().max(2) as f64);
            z[x] = latent;
        }

        // Gene flow between demes: immigrants per year into each deme, from each source.
        let mut inflow: Vec<Vec<(usize, f32)>> = vec![vec![]; nd];
        {
            let cell_to_deme: HashMap<usize, usize> =
                keep.iter().enumerate().map(|(x, &i)| (i, x)).collect();
            for &(i, j, flow) in &flows {
                if let (Some(&a), Some(&b)) = (cell_to_deme.get(&i), cell_to_deme.get(&j)) {
                    if a != b {
                        let n = demes[b].total().max(1) as f64;
                        inflow[b].push((a, (flow / n).min(1.0) as f32));
                    }
                }
            }
        }

        let m_in: Vec<f32> = inflow.iter().map(|v| v.iter().map(|f| f.1).sum()).collect();
        // Ranges connected by gene flow.
        let mut comp: Vec<usize> = (0..nd).collect();
        fn root(c: &mut [usize], mut x: usize) -> usize {
            while c[x] != x {
                c[x] = c[c[x]];
                x = c[x];
            }
            x
        }
        for x in 0..nd {
            for &(j, _) in &inflow[x] {
                let (a, b) = (root(&mut comp, x), root(&mut comp, j));
                if a != b {
                    comp[a.max(b)] = a.min(b);
                }
            }
        }
        let roots: Vec<usize> = (0..nd).map(|x| root(&mut comp, x)).collect();
        let mut label: HashMap<usize, usize> = HashMap::new();
        for x in 0..nd {
            let next = label.len();
            comp[x] = *label.entry(roots[x]).or_insert(next);
        }
        let n_comp = label.len();

        // ---- 2. Alleles: gene flow, mutation, drift and incompatibilities -------------
        // Gene flow: one mixing step with the share of immigrant genes over the step.
        let snapshot: Vec<Vec<Vec<(u32, f32)>>> = demes.iter().map(|d| d.freqs.clone()).collect();
        let mut scratch = Vec::new();
        let shares: Vec<f32> = m_in
            .iter()
            .map(|&m| (1.0 - (-m * years).exp()).min(MAX_GENE_FLOW))
            .collect();
        let mut ne_comp = vec![0f64; n_comp];
        for x in 0..nd {
            ne_comp[comp[x]] += ne[x];
        }
        for x in 0..nd {
            let m_sum = m_in[x];
            if m_sum <= 0.0 {
                continue;
            }
            let share = shares[x];
            let mut kept = 1.0 - share;
            for &(j, m) in &inflow[x] {
                let q = share * m / m_sum;
                let total = kept + q;
                for (l, pool) in demes[x].freqs.iter_mut().enumerate() {
                    merge_sorted(pool, &snapshot[j][l], kept / total, &mut scratch);
                }
                kept = total;
            }
        }
        drop(snapshot);
        let rates = self.mutation_rates();
        let (dmi_p, dmi_s) = (self.params.dmi_probability, self.params.dmi_cost);
        let year = self.year() as f32;
        for (x, deme) in demes.iter_mut().enumerate() {
            let mut rng = Rng::keyed(
                seed,
                tick,
                ((deme.species as u64) << 32) | deme.patch as u64,
                STREAM ^ 0x99,
            );
            let arch = &self.archs[deme.arch as usize];
            // Mutation, at the rate Population mode would add new alleles over the step:
            // there, each locus gains at most one new allele a year (with probability
            // 1 − e^(−2·B·μ)), up to `MAX_NEW_FUNCTIONAL` functional and
            // `MAX_NEW_NEUTRAL` neutral ones per deme. The step's new copies at a locus are
            // kept as one new allele at their combined frequency (the drift of later steps
            // decides whether it spreads).
            let n = deme.total() as f64;
            let births = cells[keep[x]].births.min(n);
            if births > 0.0 {
                let chance: Vec<(f64, bool)> = arch
                    .loci
                    .iter()
                    .map(|l| {
                        let (mu, neutral) = match l.kind {
                            LocusKind::Trait(_) => (rates.functional, false),
                            LocusKind::Neutral => (rates.neutral, true),
                            LocusKind::Dmi => (rates.dmi, false),
                        };
                        (1.0 - (-2.0 * births * mu).exp(), neutral)
                    })
                    .collect();
                let sum = |neutral: bool| -> f64 {
                    chance.iter().filter(|c| c.1 == neutral).map(|c| c.0).sum()
                };
                let scale_f = (MAX_NEW_FUNCTIONAL as f64 / sum(false).max(1e-12)).min(1.0);
                let scale_n = (MAX_NEW_NEUTRAL as f64 / sum(true).max(1e-12)).min(1.0);
                for (li, &(p, neutral)) in chance.iter().enumerate() {
                    let per_year = p * if neutral { scale_n } else { scale_f };
                    let copies = rng.poisson(per_year * years as f64);
                    if copies == 0 {
                        continue;
                    }
                    let pool = &deme.freqs[li];
                    let weights: Vec<f64> = pool.iter().map(|x| x.1 as f64).collect();
                    let parent = pool[rng.weighted(&weights)].0;
                    // Of the step's new mutations at a trait locus, those favoured by
                    // selection are the ones likely to spread: keep the best of a few.
                    let tries = if matches!(arch.loci[li].kind, LocusKind::Trait(_)) {
                        copies.min(MUTANT_CANDIDATES)
                    } else {
                        1
                    };
                    let pa = self.alleles.get(parent).clone();
                    let mut best = (f32::NEG_INFINITY, None);
                    for _ in 0..tries {
                        let m = arch.mutation(&pa, li, &rates, &mut rng);
                        let mut score = 0.0;
                        for k in 0..2 {
                            let t = m.0[k].0;
                            if t != NO_TRAIT {
                                score +=
                                    selection[x].beta[t as usize] * (m.0[k].1 - pa.effects[k].1);
                            }
                        }
                        if score > best.0 {
                            best = (score, Some(m));
                        }
                    }
                    let id = arch.push_allele(parent, li, &mut self.alleles, year, best.1.unwrap());
                    let fr = (copies as f64 / (2.0 * n)).min(0.25) as f32;
                    let pool = &mut deme.freqs[li];
                    for e in pool.iter_mut() {
                        e.1 *= 1.0 - fr;
                    }
                    pool.push((id, fr));
                }
            }
        }

        // Drift over the whole step. Demes that exchange genes drift together: the range's
        // pooled frequencies take one Wright–Fisher draw with the range's Nₑ (the sum of
        // its demes'), and every deme follows it in proportion.
        let mut members: Vec<Vec<usize>> = vec![vec![]; n_comp];
        for x in 0..nd {
            members[comp[x]].push(x);
        }
        for (c, xs) in members.iter().enumerate() {
            let first = &demes[xs[0]];
            let mut rng = Rng::keyed(
                seed,
                tick,
                ((first.species as u64) << 32) | first.patch as u64,
                STREAM ^ 0xab,
            );
            range_drift(&mut demes, xs, ne_comp[c], years as f64, &mut rng);
        }
        for (x, deme) in demes.iter_mut().enumerate() {
            let mut rng = Rng::keyed(
                seed,
                tick,
                ((deme.species as u64) << 32) | deme.patch as u64,
                STREAM ^ 0x9a,
            );
            // Differences between the demes of a range: drift within each deme, up to the
            // island model's equilibrium F_ST = 1 / (1 + 4·Nₑ·m).
            if members[comp[x]].len() > 1 {
                let f = (1.0 - (-(years as f64) / (2.0 * ne[x])).exp())
                    .min(1.0 / (1.0 + 4.0 * ne[x] * m_in[x] as f64));
                let min_freq = (0.25 / ne_comp[comp[x]]) as f32;
                for pool in deme.freqs.iter_mut() {
                    dirichlet_drift(pool, f, min_freq, &mut rng);
                }
            }
            // Selection against incompatible combinations among the alleles drift kept.
            let arch = &self.archs[deme.arch as usize];
            select_dmi(deme, arch, &self.alleles, seed, dmi_p, dmi_s, years);
        }

        // Additive variance for the trait response, after this step's mutations; it
        // shrinks under drift over the step, so its mean is used.
        for (x, d) in demes.iter().enumerate() {
            let (_, va) = trait_moments(d, &self.archs[d.arch as usize], &self.alleles);
            let xd = years as f64 / (2.0 * ne_comp[comp[x]]);
            let keep_va = if xd < 1e-6 {
                1.0
            } else {
                ((1.0 - (-xd).exp()) / xd) as f32
            };
            for t in 0..N_TRAITS {
                va_eff[x][t] = va[t] * keep_va;
            }
        }

        // ---- 3. Trait means: selection, gene flow and drift --------------------------
        // The fitness surface is measured again at the start of every sub-step, because
        // traits co-evolve and much of the fitness model compares an animal with its
        // deme, so optima move as the means do.
        let dt = years / TRAIT_SUBSTEPS as f32;
        let mut theta = vec![[0f32; N_TRAITS]; nd];
        let mut rate = vec![[0f32; N_TRAITS]; nd];
        for s in 0..TRAIT_SUBSTEPS {
            for x in 0..nd {
                let pc = &ctxs[demes[x].patch as usize];
                let pm = Phenotype::from_latent(&z[x]);
                let adult = pm.get(Trait::BodyMass);
                let dm = derive(&pm, adult);
                let heat = year_heat(&dm, pc);
                let (need_m, _) = fed_rates_heat(&pm, &dm, &heat, pc, bg);
                let em = eco_for_mean(&eco[x], &pm, &dm, &cd[x], pc, need_m);
                let fec_m = annual_heat(&pm, &dm, &heat, pc, &em, bg).fecundity;
                let mass = MassTerms::new(adult, adult);
                let g = gradient(&pm, &z[x], pc, &em, bg, fec_m, &heat, &mass);
                let w0 = fitness_parts(&pm, pc, &em, bg, fec_m, Some(&mass), Some(&heat))
                    .w
                    .ln();
                for t in 0..N_TRAITS {
                    theta[x][t] = z[x][t];
                    rate[x][t] = 0.0;
                    if FITNESS_NEUTRAL.contains(&Trait::from_index(t)) {
                        continue; // a trait the fitness model does not read
                    }
                    let (lh, ll) = g.lnw[t];
                    let h = GRADIENT_DELTA;
                    let b = (lh - ll) / (2.0 * h);
                    let curv = (lh - 2.0 * w0 + ll) / (h * h);
                    let va = va_eff[x][t];
                    let (shift, a) = if curv < -1e-3 && (b / curv).abs() < MAX_SHIFT {
                        // Stabilising: the mean relaxes towards the optimum at rate
                        // V_A·|curvature| (Lande 1976), at most at the speed V_A·|β|max.
                        (
                            -b / curv,
                            va * (-curv).min(1.0 / (b / curv).abs().max(1e-6)),
                        )
                    } else if b.abs() > 1e-6 {
                        // Directional: move at the speed V_A·β (β capped as in
                        // Population mode) towards a target `MAX_SHIFT` away.
                        (MAX_SHIFT * b.signum(), va * b.abs().min(1.0) / MAX_SHIFT)
                    } else {
                        (0.0, 0.0)
                    };
                    theta[x][t] = z[x][t] + shift;
                    rate[x][t] = a;
                }
                let mut rng = Rng::keyed(seed, tick, (x as u64) << 8 | s as u64, STREAM ^ 0x77);
                for t in 0..N_TRAITS {
                    if va_eff[x][t] > 0.0 {
                        z[x][t] += rng.normal_f32(0.0, (va_eff[x][t] * dt / ne[x] as f32).sqrt());
                    }
                }
            }
            let start = z.clone();
            // Backward Euler: (1 + dt·(a + Σm))·z − dt·Σ m·z_j = z₀ + dt·a·θ, solved by
            // Gauss–Seidel sweeps. Sweeps settle local differences quickly but a shift
            // of a whole connected range slowly, so after each sweep that shift is solved
            // exactly (for a uniform change the gene-flow terms cancel).
            for _ in 0..SWEEPS {
                for x in 0..nd {
                    let m_sum = m_in[x];
                    for t in 0..N_TRAITS {
                        let a = rate[x][t];
                        let pull: f32 = inflow[x].iter().map(|&(j, m)| m * z[j][t]).sum();
                        z[x][t] = (start[x][t] + dt * (a * theta[x][t] + pull))
                            / (1.0 + dt * (a + m_sum));
                    }
                }
                let mut num = vec![[0f64; N_TRAITS]; n_comp];
                let mut den = vec![[0f64; N_TRAITS]; n_comp];
                for x in 0..nd {
                    let w = demes[x].total() as f64;
                    for t in 0..N_TRAITS {
                        let a = rate[x][t];
                        let pull: f32 = inflow[x].iter().map(|&(j, m)| m * z[j][t]).sum();
                        let lhs = (1.0 + dt * (a + m_in[x])) * z[x][t] - dt * pull;
                        let rhs = start[x][t] + dt * a * theta[x][t];
                        num[comp[x]][t] += w * (rhs - lhs) as f64;
                        den[comp[x]][t] += w * (1.0 + dt * a) as f64;
                    }
                }
                for x in 0..nd {
                    for t in 0..N_TRAITS {
                        let d = den[comp[x]][t];
                        if d > 0.0 {
                            z[x][t] += (num[comp[x]][t] / d) as f32;
                        }
                    }
                }
            }
        }

        // Allele frequencies follow the trait means.
        for (x, deme) in demes.iter_mut().enumerate() {
            let arch = &self.archs[deme.arch as usize];
            // Trait means to their targets.
            for _ in 0..TILT_PASSES {
                let (mean, va) = trait_moments(deme, arch, &self.alleles);
                let mut c = [0f32; N_TRAITS];
                let mut any = false;
                for t in 0..N_TRAITS {
                    let gap = z[x][t] - mean[t];
                    if va[t] > 1e-6 && gap.abs() > 1e-4 {
                        c[t] = gap / va[t];
                        any = true;
                    }
                }
                if !any {
                    break;
                }
                tilt(deme, arch, &self.alleles, &c);
            }
            for pool in deme.freqs.iter_mut() {
                pool.sort_by_key(|e| e.0);
                prune_rare(pool, MAX_ALLELES_PER_LOCUS);
            }
        }
        demes.retain(|d| d.total() > 0);
        demes.sort_by_key(|d| (d.species, d.patch));
        self.demes = demes;
        self.diagnostics = diag;
        self.selection = selection;
        self.world.grow_vegetation_year();
        self.minutes += (MACRO_STEP_YEARS * self.minutes_per_year()) as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_has_the_right_mean_and_variance() {
        let mut rng = Rng::new(9);
        for shape in [0.3, 1.0, 4.5, 50.0, 2e5] {
            let n = 20_000;
            let xs: Vec<f64> = (0..n).map(|_| gamma(&mut rng, shape)).collect();
            let m = xs.iter().sum::<f64>() / n as f64;
            let v = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n as f64;
            assert!((m / shape - 1.0).abs() < 0.05, "shape {shape}: mean {m}");
            assert!((v / shape - 1.0).abs() < 0.1, "shape {shape}: var {v}");
        }
    }

    #[test]
    fn dirichlet_drift_keeps_the_mean_and_has_wright_fisher_variance() {
        let mut rng = Rng::new(4);
        for f in [0.01, 0.2, 0.9] {
            let reps = 20_000;
            let (mut s, mut s2) = (0.0, 0.0);
            for _ in 0..reps {
                let mut pool = vec![(1, 0.3f32), (2, 0.7f32)];
                dirichlet_drift(&mut pool, f, 0.0, &mut rng);
                let p = pool.iter().find(|x| x.0 == 1).map(|x| x.1).unwrap_or(0.0) as f64;
                s += p;
                s2 += p * p;
            }
            let m = s / reps as f64;
            let v = s2 / reps as f64 - m * m;
            assert!((m - 0.3).abs() < 0.01, "F {f}: mean {m}");
            let want = 0.3 * 0.7 * f;
            assert!((v / want - 1.0).abs() < 0.1, "F {f}: var {v} want {want}");
        }
        // Complete drift fixes one allele.
        let mut pool = vec![(1, 0.5f32), (2, 0.5f32)];
        dirichlet_drift(&mut pool, 1.0, 0.0, &mut rng);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool[0].1, 1.0);
    }
}
