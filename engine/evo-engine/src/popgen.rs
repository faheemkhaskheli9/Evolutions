//! Population-genetics measures per species over time (PRD FR-O7): expected
//! heterozygosity, F_ST between patches, effective population size, the number of
//! segregating alleles, and the frequencies of common alleles for trajectory charts.
//!
//! These are read from the gene pool at each metrics sample and change nothing in
//! the simulation. They are saved after the replay log, so they are not part of
//! `state_hash()`, and saves made before they existed load with an empty series.

use crate::animal::Sex;
use crate::physiology::derive;
use crate::population::MAX_NEW_NEUTRAL;
use crate::record::SERIES_CAP;
use crate::sim::{Mode, Simulation};
use crate::traits::{Phenotype, Trait};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Allele frequencies kept per sample (the most variable ones).
pub const MAX_TRACKED_ALLELES: usize = 48;
/// Alleles between these frequencies are tracked for trajectory charts.
pub const TRACK_MIN: f64 = 0.01;
/// Generations of history the heterozygosity-based Nₑ is estimated over.
pub const NE_WINDOW_GENERATIONS: f64 = 20.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenSample {
    pub year: f64,
    /// Expected heterozygosity of the whole species (H_T), mean over neutral loci.
    pub ht: f32,
    /// Mean expected heterozygosity within patches (H_S), weighted by patch size.
    pub hs: f32,
    /// F_ST = (H_T − H_S) / H_T between patches (Nei's G_ST), clamped at 0.
    pub fst: f32,
    /// Nₑ from the adult sex ratio: 4·N_m·N_f / (N_m + N_f).
    pub ne: f32,
    /// Nₑ from the loss of heterozygosity over the last generations, corrected for
    /// mutation; negative when it cannot be estimated (too little history, or
    /// heterozygosity not falling faster than mutation alone allows).
    pub ne_h: f32,
    /// Generation time used for `ne_h`, years.
    pub generation: f32,
    /// Alleles present at loci with more than one allele (all loci).
    pub segregating: u32,
    /// (allele uid, frequency × 65535) for alleles between 1% and 99%, plus those
    /// tracked in the previous sample (so a trajectory shows its loss or fixation).
    pub freqs: Vec<(u64, u16)>,
}

impl GenSample {
    pub fn freq(&self, uid: u64) -> Option<f32> {
        self.freqs
            .iter()
            .find(|x| x.0 == uid)
            .map(|x| x.1 as f32 / 65535.0)
    }
}

/// Per-species series, decimated like `Metrics`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PopGen {
    pub series: BTreeMap<u32, Vec<GenSample>>,
}

impl PopGen {
    pub fn push(&mut self, species: u32, s: GenSample) {
        let v = self.series.entry(species).or_default();
        v.push(s);
        if v.len() > SERIES_CAP {
            let kept: Vec<GenSample> = v
                .drain(..)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, s)| s)
                .collect();
            *v = kept;
        }
    }
}

/// Gene copies per allele at one locus: (allele id, copies).
type Pool = Vec<(u32, f64)>;

fn add(pool: &mut Pool, allele: u32, copies: f64) {
    match pool.iter_mut().find(|x| x.0 == allele) {
        Some(x) => x.1 += copies,
        None => pool.push((allele, copies)),
    }
}

/// Unbiased expected heterozygosity (Nei 1978) of one locus from `n` diploids:
/// 2n/(2n − 1) · (1 − Σp²).
fn expected_het(pool: &Pool, n: f64) -> f64 {
    let copies = 2.0 * n;
    if copies < 2.0 {
        return 0.0;
    }
    let sum_p2: f64 = pool.iter().map(|x| (x.1 / copies).powi(2)).sum();
    (copies / (copies - 1.0) * (1.0 - sum_p2)).max(0.0)
}

#[derive(Default)]
struct Acc {
    arch: u16,
    /// Diploid individuals counted.
    n: f64,
    /// Every locus, pooled over the species.
    total: Vec<Pool>,
    /// Per patch: individuals and neutral-locus pools.
    patches: BTreeMap<u32, (f64, Vec<Pool>)>,
    adults_f: f64,
    adults_m: f64,
    /// Neutral mutations expected per year and neutral gene copies born per year in
    /// demes, for the effective mutation rate (Population mode tracks at most
    /// `MAX_NEW_NEUTRAL` new neutral alleles per deme per year).
    deme_mutations: f64,
    deme_n: f64,
    deme_copies: f64,
}

/// Nₑ from inbreeding F₀ → F₁ over `g` generations with neutral mutation rate `mu`
/// (infinite alleles): F' = (1 − μ)² · (1/2Nₑ + (1 − 1/2Nₑ)·F). Solved by bisection.
/// None when F₁ is no higher than mutation alone would leave it (no measurable drift).
pub fn ne_from_inbreeding(f0: f64, f1: f64, g: f64, mu: f64) -> Option<f64> {
    if g.is_nan() || g <= 0.0 || !(0.0..1.0).contains(&f0) || f1.is_nan() || f1 >= 1.0 {
        return None;
    }
    let lambda = (1.0 - mu).powi(2);
    let predict = |a: f64| {
        let fixed = lambda * a / (1.0 - lambda * (1.0 - a));
        fixed + (lambda * (1.0 - a)).powf(g) * (f0 - fixed)
    };
    let (mut lo, mut hi) = (1e-10f64.ln(), 0.5f64.ln());
    if f1 <= predict(lo.exp()) {
        return None;
    }
    if f1 >= predict(hi.exp()) {
        return Some(1.0);
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if predict(mid.exp()) < f1 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(1.0 / (2.0 * (0.5 * (lo + hi)).exp()))
}

impl Simulation {
    /// Records a population-genetics sample for every living species (see `GenSample`).
    pub(crate) fn record_popgen(&mut self, year: f64) {
        let mut acc: BTreeMap<u32, Acc> = BTreeMap::new();
        let new_acc = |arch: u16, loci: usize| Acc {
            arch,
            total: vec![Vec::new(); loci],
            ..Default::default()
        };
        for a in &self.animals {
            let arch = &self.archs[a.arch as usize];
            let patch = self.patch_of_tile_xy(a.x, a.y);
            let e = acc
                .entry(a.species)
                .or_insert_with(|| new_acc(a.arch, arch.loci.len()));
            e.n += 1.0;
            for h in &a.genome.hap {
                for (l, &x) in h.iter().enumerate() {
                    add(&mut e.total[l], x, 1.0);
                }
            }
            let p = e
                .patches
                .entry(patch)
                .or_insert_with(|| (0.0, vec![Vec::new(); arch.neutral_loci.len()]));
            p.0 += 1.0;
            for (k, &l) in arch.neutral_loci.iter().enumerate() {
                for h in &a.genome.hap {
                    add(&mut p.1[k], h[l], 1.0);
                }
            }
            if a.is_mature() {
                match a.sex {
                    Sex::Female => e.adults_f += 1.0,
                    Sex::Male => e.adults_m += 1.0,
                }
            }
        }
        let mu = self.mutation_rates().neutral;
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
            let e = acc
                .entry(d.species)
                .or_insert_with(|| new_acc(d.arch, arch.loci.len()));
            e.n += n;
            e.deme_n += n;
            for (l, pool) in d.freqs.iter().enumerate() {
                for &(x, f) in pool {
                    add(&mut e.total[l], x, 2.0 * n * f as f64);
                }
            }
            let p = e
                .patches
                .entry(d.patch)
                .or_insert_with(|| (0.0, vec![Vec::new(); arch.neutral_loci.len()]));
            p.0 += n;
            for (k, &l) in arch.neutral_loci.iter().enumerate() {
                for &(x, f) in &d.freqs[l] {
                    add(&mut p.1[k], x, 2.0 * n * f as f64);
                }
            }
            // Newborns are age class 0 after a step.
            let copies = 2.0 * (d.females[0] + d.males[0]) as f64 * arch.neutral_loci.len() as f64;
            e.deme_mutations += (copies * mu).min(MAX_NEW_NEUTRAL as f64);
            e.deme_copies += copies;
            // Breeding adults, as the Population-mode step counts them.
            let (mean, _) = d.mean_phenotype(arch, &self.alleles);
            let pheno = Phenotype(mean);
            let mat = derive(&pheno, pheno.get(Trait::BodyMass)).maturity_days / 365.25;
            for age in 0..d.females.len() {
                let breeding = (age as f32 + 1.0 - mat).clamp(0.0, 1.0) as f64;
                e.adults_f += breeding * d.females[age] as f64;
                e.adults_m += breeding * d.males[age] as f64;
            }
        }

        for (species, e) in acc {
            if e.n <= 0.0 || self.species.get(species).extinct_year.is_some() {
                continue;
            }
            let arch = &self.archs[e.arch as usize];
            let nl = arch.neutral_loci.len().max(1) as f64;
            let ht = arch
                .neutral_loci
                .iter()
                .map(|&l| expected_het(&e.total[l], e.n))
                .sum::<f64>()
                / nl;
            let hs = e
                .patches
                .values()
                .map(|(n, pools)| n * pools.iter().map(|p| expected_het(p, *n)).sum::<f64>() / nl)
                .sum::<f64>()
                / e.n;
            let fst = if ht > 1e-9 {
                ((ht - hs) / ht).max(0.0)
            } else {
                0.0
            };
            let ne = if e.adults_f + e.adults_m > 0.0 {
                4.0 * e.adults_f * e.adults_m / (e.adults_f + e.adults_m)
            } else {
                0.0
            };

            // Generation time: age at maturity of the mean phenotype, at least the
            // one-year step in which Population mode applies drift.
            let mean = self
                .metrics
                .series
                .get(&species)
                .and_then(|v| v.last())
                .filter(|s| s.year == year)
                .map(|s| s.mean.clone());
            let generation = mean
                .and_then(|m| <[f32; crate::traits::N_TRAITS]>::try_from(m).ok())
                .map(|m| {
                    let p = Phenotype(m);
                    derive(&p, p.get(Trait::BodyMass)).maturity_days / 365.25
                })
                .unwrap_or(1.0)
                .max(1.0) as f64;
            // Mutation rate per gene copy: nominal for animals, as tracked for demes.
            let animal_copies = 2.0 * (e.n - e.deme_n) * nl;
            let mu_eff =
                (animal_copies * mu + e.deme_mutations) / (animal_copies + e.deme_copies).max(1.0);
            let history = self.popgen.series.get(&species);
            let ne_h = history
                .and_then(|v| {
                    v.iter()
                        .rev()
                        .find(|s| year - s.year >= NE_WINDOW_GENERATIONS * generation)
                })
                .and_then(|s| {
                    let g = (year - s.year) / generation;
                    ne_from_inbreeding(1.0 - s.ht as f64, 1.0 - ht, g, mu_eff)
                })
                .map_or(-1.0, |x| x as f32);

            let mut segregating = 0;
            let mut tracked: Vec<(f64, u64, f64)> = Vec::new();
            for pool in &e.total {
                let live = pool.iter().filter(|x| x.1 > 0.0).count();
                if live > 1 {
                    segregating += live as u32;
                }
                for &(x, c) in pool {
                    let p = c / (2.0 * e.n);
                    if (TRACK_MIN..=1.0 - TRACK_MIN).contains(&p) {
                        tracked.push((p * (1.0 - p), self.alleles.get(x).uid, p));
                    }
                }
            }
            tracked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            tracked.truncate(MAX_TRACKED_ALLELES);
            let mut freqs: Vec<(u64, u16)> = tracked
                .iter()
                .map(|&(_, uid, p)| (uid, (p * 65535.0).round() as u16))
                .collect();
            // Alleles tracked last time that are now rare, lost or fixed: record where they went.
            if let Some(prev) = history.and_then(|v| v.last()) {
                for &(uid, _) in &prev.freqs {
                    if freqs.iter().any(|x| x.0 == uid) {
                        continue;
                    }
                    let p = self.allele_pool_freq(&e.total, uid, e.n);
                    freqs.push((uid, (p * 65535.0).round() as u16));
                }
            }
            freqs.sort_by_key(|x| x.0);

            self.popgen.push(
                species,
                GenSample {
                    year,
                    ht: ht as f32,
                    hs: hs as f32,
                    fst: fst as f32,
                    ne: ne as f32,
                    ne_h,
                    generation: generation as f32,
                    segregating,
                    freqs,
                },
            );
        }
    }

    /// Frequency of the allele `uid` in pooled per-locus counts of `n` diploids.
    fn allele_pool_freq(&self, total: &[Pool], uid: u64, n: f64) -> f64 {
        let Some(id) = self.alleles.alleles.iter().position(|a| a.uid == uid) else {
            return 0.0;
        };
        let locus = self.alleles.get(id as u32).locus as usize;
        total
            .get(locus)
            .and_then(|p| p.iter().find(|x| x.0 == id as u32))
            .map_or(0.0, |x| (x.1 / (2.0 * n)).min(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbreeding_inverts_the_drift_recursion() {
        for &(ne, g, mu) in &[(50.0, 40.0, 0.0), (300.0, 100.0, 2e-4), (5.0, 3.0, 1e-5)] {
            let a: f64 = 1.0 / (2.0 * ne);
            let lambda = (1.0f64 - mu).powi(2);
            let mut f = 0.2;
            for _ in 0..g as usize {
                f = lambda * (a + (1.0 - a) * f);
            }
            let est = ne_from_inbreeding(0.2, f, g, mu).unwrap();
            assert!((est - ne).abs() / ne < 1e-3, "{est} vs {ne}");
        }
    }

    #[test]
    fn no_estimate_without_drift() {
        // Heterozygosity rising (F falling): mutation outweighs drift.
        assert_eq!(ne_from_inbreeding(0.3, 0.25, 10.0, 1e-4), None);
        assert_eq!(ne_from_inbreeding(0.3, 0.4, 0.0, 1e-4), None);
    }

    #[test]
    fn expected_het_is_unbiased_for_two_copies() {
        // One heterozygote: 2/(2−1) · (1 − 0.5) = 1.
        assert!((expected_het(&vec![(0, 1.0), (1, 1.0)], 1.0) - 1.0).abs() < 1e-12);
        assert_eq!(expected_het(&vec![(0, 2.0)], 1.0), 0.0);
    }
}
