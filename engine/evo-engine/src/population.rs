//! Population mode (Mode P, TRD §5.11): each species in each habitat patch is a
//! deme with an age × sex structure and per-locus allele frequencies. One step
//! is one year:
//!
//! 1. Vital rates come from the *same* physiology functions as Individual mode,
//!    evaluated on the deme's mean phenotype in the patch's climate.
//! 2. Selection: the gradient of log fitness with respect to each latent trait
//!    (Lande 1979) changes allele frequencies through their additive effects.
//! 3. Demography with binomial/Poisson noise, Wright–Fisher drift with Nₑ,
//!    mutation, and stepping-stone migration (with rare rafting).

use crate::animal::{Animal, Sex};
use crate::genetics::{incompatible, AlleleTable, Genome, GenomeArch, NO_TRAIT};
use crate::physiology::{
    camouflage, derive, gompertz_hazard, mate_acceptance, roughage_quality, thermal, Derived,
    EVAP_J_PER_G, TISSUE_KJ_PER_KG,
};
use crate::rng::{stream, Rng};
use crate::sim::Simulation;
use crate::species::PopSummary;
use crate::traits::{Phenotype, Trait, N_TRAITS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const AGE_CLASSES: usize = 40;
/// Rare-allele tail kept per locus per deme (bounds cost in very large demes).
pub const MAX_ALLELES_PER_LOCUS: usize = 16;
/// New mutations tracked per deme per year (see the mutation step).
pub const MAX_NEW_MUTATIONS: u64 = 6;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Deme {
    pub species: u32,
    pub arch: u16,
    pub patch: u32,
    pub females: Vec<u32>,
    pub males: Vec<u32>,
    /// Per locus: (allele id, frequency), sorted by id.
    pub freqs: Vec<Vec<(u32, f32)>>,
    pub condition: f32,
}

impl Deme {
    pub fn total(&self) -> u32 {
        self.females.iter().sum::<u32>() + self.males.iter().sum::<u32>()
    }

    /// Expected latent trait values from allele frequencies (additive approximation).
    pub fn mean_latent(&self, arch: &GenomeArch, table: &AlleleTable) -> [f32; N_TRAITS] {
        let mut v = arch.base;
        for &li in &arch.trait_loci {
            for &(a, p) in &self.freqs[li] {
                for &(t, e) in &table.get(a).effects {
                    if t != NO_TRAIT {
                        v[t as usize] += 2.0 * p * e;
                    }
                }
            }
        }
        v
    }

    /// Mean phenotype and mean neutral heterozygosity (1 − Σp²).
    pub fn mean_phenotype(&self, arch: &GenomeArch, table: &AlleleTable) -> ([f32; N_TRAITS], f32) {
        let p = Phenotype::from_latent(&self.mean_latent(arch, table));
        let het = arch
            .neutral_loci
            .iter()
            .map(|&l| 1.0 - self.freqs[l].iter().map(|(_, f)| f * f).sum::<f32>())
            .sum::<f32>()
            / arch.neutral_loci.len().max(1) as f32;
        (p.0, het)
    }

    pub fn sample_genome(&self, rng: &mut Rng) -> Genome {
        let draw = |pool: &Vec<(u32, f32)>, rng: &mut Rng| -> u32 {
            let mut r = rng.f32();
            for &(a, p) in pool {
                if r < p {
                    return a;
                }
                r -= p;
            }
            pool.last().map(|x| x.0).unwrap_or(0)
        };
        let mut hap = [
            Vec::with_capacity(self.freqs.len()),
            Vec::with_capacity(self.freqs.len()),
        ];
        for h in hap.iter_mut() {
            for pool in &self.freqs {
                h.push(draw(pool, rng));
            }
        }
        Genome { hap }
    }

    /// Removes a fraction of individuals at random. Returns how many died.
    pub fn cull(&mut self, frac: f64, rng: &mut Rng) -> u64 {
        let mut k = 0;
        for c in self.females.iter_mut().chain(self.males.iter_mut()) {
            let d = rng.binomial(*c as u64, frac);
            *c -= d as u32;
            k += d;
        }
        k
    }

    pub fn mark_live(&self, live: &mut [bool]) {
        for pool in &self.freqs {
            for &(a, _) in pool {
                live[a as usize] = true;
            }
        }
    }

    pub fn remap(&mut self, remap: &[u32]) {
        for pool in self.freqs.iter_mut() {
            for e in pool.iter_mut() {
                e.0 = remap[e.0 as usize];
            }
            pool.sort_by_key(|e| e.0);
        }
    }

    fn merge(&mut self, other: &Deme) {
        let (n1, n2) = (self.total() as f32, other.total() as f32);
        if n2 <= 0.0 {
            return;
        }
        let w1 = n1 / (n1 + n2);
        for (l, pool) in self.freqs.iter_mut().enumerate() {
            *pool = merge_sorted(pool, &other.freqs[l], w1);
        }
        for k in 0..AGE_CLASSES {
            self.females[k] += other.females[k];
            self.males[k] += other.males[k];
        }
        self.condition = self.condition * w1 + other.condition * (1.0 - w1);
    }
}

/// Weighted merge of two allele-frequency pools sorted by allele id.
fn merge_sorted(a: &[(u32, f32)], b: &[(u32, f32)], wa: f32) -> Vec<(u32, f32)> {
    let wb = 1.0 - wa;
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 2);
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if j >= b.len() || (i < a.len() && a[i].0 < b[j].0) {
            out.push((a[i].0, a[i].1 * wa));
            i += 1;
        } else if i >= a.len() || b[j].0 < a[i].0 {
            out.push((b[j].0, b[j].1 * wb));
            j += 1;
        } else {
            out.push((a[i].0, a[i].1 * wa + b[j].1 * wb));
            i += 1;
            j += 1;
        }
    }
    out
}

/// Climate and resources of one patch, recomputed each year.
#[derive(Clone, Default)]
struct PatchCtx {
    land: Vec<usize>,
    temp_month: [f32; 12],
    diurnal: f32,
    water_access: f32,
    luminance: f32,
    cover: f32,
    /// Sustainable offtake of high-quality and coarse forage, g / yr.
    plant_supply_g: f32,
    rough_supply_g: f32,
}

/// Ecological context of one deme for the year.
#[derive(Clone, Copy, Default)]
struct Eco {
    food_ratio: f32,
    pred_hazard: f32,
    mean_catch: f32,
    mean_plant_eff: f32,
    mean_pref: f32,
    mean_choosy: f32,
    mean_acc: f32,
}

#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct Annual {
    /// Ecological hazard per year (excluding senescence), and its parts.
    pub hazard: f32,
    pub starvation: f32,
    pub dehydration: f32,
    pub predation: f32,
    pub heat: f32,
    pub cold: f32,
    pub juvenile_survival: f32,
    pub food_ratio: f32,
    /// Offspring per female per year surviving to age 1.
    pub fecundity: f32,
    pub need_kj: f32,
    pub mating: f32,
    pub maturity_years: f32,
}

/// Heat balance with the same behavioural buffering as Individual mode: animals
/// rest in shade when active heat load would overheat them, and shelter when cold.
pub fn behavioural_thermal(d: &Derived, ta: f32) -> (f32, crate::physiology::Thermal) {
    let heat = d.bmr_w * 1.7;
    let th = thermal(d, heat, ta);
    if th.heat_stress > 0.0 {
        let heat = d.bmr_w;
        return (
            heat,
            thermal(d, heat, ta - crate::physiology::SHADE_COOLING),
        );
    }
    if th.extra_w > 0.0 {
        return (
            heat,
            thermal(d, heat, ta + crate::physiology::SHELTER_WARMING * 0.5),
        );
    }
    (heat, th)
}

fn catchability(p: &Phenotype, d: &Derived, lum: f32) -> f32 {
    (1.0 - 0.6 * camouflage(p.get(Trait::CoatLuminance), lum))
        * (1.0 + 0.5 * p.get(Trait::Ornament))
        * (1.0 - 0.3 * p.get(Trait::Senses))
        * (40.0 / d.vmax_kmh.max(1.0)).powf(0.7)
}

/// Annual vital rates of a phenotype in a patch (shared physiology, TRD TR-MP2).
fn annual(p: &Phenotype, pc: &PatchCtx, eco: &Eco, bg: f32) -> Annual {
    let adult = p.get(Trait::BodyMass);
    let d = derive(p, adult);
    let mut need = 0.0;
    let mut evap_g = 0.0;
    let (mut heat_s, mut cold_s) = (0.0, 0.0);
    for &t in &pc.temp_month {
        for ta in [t - pc.diurnal, t + pc.diurnal] {
            let (heat, th) = behavioural_thermal(&d, ta);
            need += (heat + th.extra_w) * 86.4 * 15.22;
            evap_g += th.evap_w * 86400.0 / EVAP_J_PER_G * 15.22;
            heat_s += th.heat_stress / 24.0;
            cold_s += th.cold_stress / 24.0;
        }
    }
    let cycle = d.gestation_days + d.weaning_days;
    let litters = if cycle < 300.0 {
        (90.0 / cycle).clamp(1.0, 2.5)
    } else {
        365.0 / cycle
    };
    let litter = p.get(Trait::LitterSize);
    let repro_kj = litters * litter * d.neonate_kg * TISSUE_KJ_PER_KG / 0.5
        + litters * 0.5 * d.bmr_w * 86.4 * d.weaning_days;
    let plant_share = d.plant_eff / (d.plant_eff + d.meat_eff).max(1e-3);
    let eff_rel = if plant_share > 0.5 {
        (d.plant_eff / eco.mean_plant_eff.max(0.01)).powf(0.5)
    } else {
        1.0
    };
    let food = (eco.food_ratio * eff_rel).clamp(0.0, 1.5);
    let starve = if food < 1.0 {
        -1.5 * food.max(0.02).ln()
    } else {
        0.0
    };
    let water_need = d.water_loss_g_day * 365.0 + evap_g;
    let water = (1.0 - pc.water_access).powi(2)
        * (water_need / (d.water_cap_g * 365.0)).min(3.0)
        * (1.3 - p.get(Trait::Kidney))
        * 0.8;
    let pred = if eco.mean_catch > 0.0 {
        eco.pred_hazard * catchability(p, &d, pc.luminance) / eco.mean_catch
    } else {
        0.0
    };
    let hazard = starve + water + pred + 0.6 * heat_s + 0.6 * cold_s + bg;
    // Newborns face the season they are born into.
    let birth_month =
        ((p.get(Trait::BreedingDay) + d.gestation_days / 365.25) * 12.0) as usize % 12;
    let dn = derive(p, d.neonate_kg);
    let tb = thermal(&dn, dn.bmr_w * 1.2, pc.temp_month[birth_month]);
    let birth_stress = 0.3 * tb.cold_stress
        + 0.3 * tb.heat_stress
        + 0.2 * (tb.extra_w / dn.bmr_w.max(1e-3)).min(4.0);
    let juv =
        (-(0.5 * hazard + birth_stress)).exp() * (d.neonate_kg / (0.07 * adult)).powf(0.3) * 0.6;
    let fecundity = litters * litter * juv * food.min(1.0);
    let acc = mate_acceptance(eco.mean_pref, eco.mean_choosy, p.get(Trait::Ornament));
    Annual {
        hazard,
        starvation: starve,
        dehydration: water,
        predation: pred,
        heat: 0.6 * heat_s,
        cold: 0.6 * cold_s,
        juvenile_survival: juv,
        food_ratio: food,
        fecundity,
        need_kj: need + 0.5 * repro_kj,
        mating: acc / eco.mean_acc.max(1e-4),
        maturity_years: d.maturity_days / 365.25,
    }
}

/// Fitness used for the selection gradient: survival plus credited reproduction.
fn fitness(p: &Phenotype, pc: &PatchCtx, eco: &Eco, bg: f32, mean_fec: f32) -> f32 {
    let a = annual(p, pc, eco, bg);
    let d = derive(p, p.get(Trait::BodyMass));
    let s = (-(a.hazard + gompertz_hazard(&d, a.maturity_years + 1.0, 0.0))).exp();
    (s + 0.25 * (a.fecundity + mean_fec * a.mating.min(3.0))).max(1e-6)
}

impl Simulation {
    fn patch_contexts(&self) -> Vec<PatchCtx> {
        let (pw, ph) = (self.patches_w(), self.patches_h());
        let ps = self.params.patch_size;
        let w = &self.world;
        let area = self.params.habitat_m2_per_tile as f32;
        let mut out = vec![PatchCtx::default(); pw * ph];
        for (p, pc) in out.iter_mut().enumerate() {
            let (px, py) = (p % pw, p / pw);
            let mut tm = [0f32; 12];
            let (mut acc, mut lum, mut cov, mut dry) = (0.0, 0.0, 0.0, 0.0);
            for y in py * ps..((py + 1) * ps).min(w.h) {
                for x in px * ps..((px + 1) * ps).min(w.w) {
                    let i = y * w.w + x;
                    if !w.is_land(i) {
                        continue;
                    }
                    pc.land.push(i);
                    let mut season = 0.0;
                    for (m, v) in tm.iter_mut().enumerate() {
                        let t = w.temp_day(i, m as f64 * 30.44 + 15.0);
                        *v += t;
                        season += (t / 10.0).clamp(0.0, 1.0) / 12.0;
                    }
                    pc.plant_supply_g += 0.5 * w.forage_production(i) * season * area;
                    pc.rough_supply_g += 0.5 * w.roughage_production(i) * season * area;
                    acc += w.water_access[i];
                    lum += w.biome[i].props().luminance;
                    cov += w.biome[i].props().cover;
                    dry += 1.0 - (w.precip[i] / 1500.0).clamp(0.0, 1.0);
                }
            }
            let n = pc.land.len().max(1) as f32;
            for (m, v) in tm.iter().enumerate() {
                pc.temp_month[m] = v / n;
            }
            pc.water_access = acc / n;
            pc.luminance = lum / n;
            pc.cover = cov / n;
            pc.diurnal = 3.0 + 6.0 * dry / n;
        }
        out
    }

    pub(crate) fn step_population(&mut self) {
        let ctxs = self.patch_contexts();
        let bg = self.params.background_mortality as f32;
        let year = self.year() as f32;
        let tick = self.minutes;
        let nd = self.demes.len();

        // Mean phenotypes.
        let means: Vec<Phenotype> = self
            .demes
            .iter()
            .map(|d| {
                Phenotype::from_latent(&d.mean_latent(&self.archs[d.arch as usize], &self.alleles))
            })
            .collect();
        let derived: Vec<Derived> = means
            .iter()
            .map(|p| derive(p, p.get(Trait::BodyMass)))
            .collect();

        // Ecology per patch: plant demand vs supply, and predation (Holling type II).
        let mut by_patch: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for (k, d) in self.demes.iter().enumerate() {
            by_patch.entry(d.patch).or_default().push(k);
        }
        let mut eco = vec![Eco::default(); nd];
        let mut base_need = vec![0f32; nd];
        for (k, d) in self.demes.iter().enumerate() {
            let pc = &ctxs[d.patch as usize];
            let probe = Eco {
                food_ratio: 1.0,
                mean_plant_eff: derived[k].plant_eff,
                mean_acc: 1.0,
                ..Default::default()
            };
            base_need[k] = annual(&means[k], pc, &probe, bg).need_kj;
        }
        let kj_g = self.params.forage_kj_per_g as f32;
        let kill_kj = |prey: &Derived, m: f32| -> f32 {
            let _ = prey;
            m * 1000.0 * 0.6 * 7.0
        };
        for (&patch, ks) in &by_patch {
            let pc = &ctxs[patch as usize];
            // Plant food: everyone wants high-quality forage first; what is left over is
            // sought in coarse forage by those whose guts can digest it (Jarman–Bell).
            let mut demand_hq = 0.0;
            let mut eff_sum = 0.0;
            let mut eff_n = 0.0;
            let mut need_kj_plant = vec![0f32; ks.len()];
            for (x, &k) in ks.iter().enumerate() {
                let dd = &derived[k];
                let share = dd.plant_eff / (dd.plant_eff + dd.meat_eff).max(1e-3);
                if dd.meat_eff < 0.3 || share > 0.3 {
                    let n = self.demes[k].total() as f32;
                    need_kj_plant[x] =
                        n * base_need[k] * if dd.meat_eff >= 0.3 { share } else { 1.0 };
                    demand_hq += need_kj_plant[x] / (kj_g * dd.plant_eff.max(0.01));
                    eff_sum += n * dd.plant_eff;
                    eff_n += n;
                }
            }
            let ratio_hq = if demand_hq > 0.0 {
                (pc.plant_supply_g / demand_hq).min(1.0)
            } else {
                1.0
            };
            // Coarse forage goes preferentially to guts that digest it (competitive weight q).
            let mut weighted_lq = 0.0;
            for (x, &k) in ks.iter().enumerate() {
                let q = roughage_quality(means[k].get(Trait::BodyMass));
                if q > 0.1 {
                    weighted_lq += q * need_kj_plant[x] * (1.0 - ratio_hq)
                        / (kj_g * derived[k].plant_eff.max(0.01) * q);
                }
            }
            let surplus = if demand_hq > 0.0 {
                (pc.plant_supply_g / demand_hq).clamp(1.0, 1.5)
            } else {
                1.5
            };
            let plant_ratio_of = |k: usize| -> f32 {
                let q = roughage_quality(means[k].get(Trait::BodyMass));
                let ratio_lq = if q <= 0.1 {
                    0.0
                } else if weighted_lq > 0.0 {
                    (pc.rough_supply_g * q / weighted_lq).min(1.5)
                } else {
                    1.5
                };
                if ratio_hq >= 1.0 {
                    surplus.max(ratio_lq)
                } else {
                    ratio_hq + (1.0 - ratio_hq) * ratio_lq.min(1.0)
                }
            };
            // Predation, in two passes: predators request kills (type III functional
            // response), then each prey deme can lose at most what is outside refuges.
            // "Doomed surplus" (Errington 1946): mostly young, old and weak animals are
            // taken, so the yearly loss a prey deme can suffer scales with its
            // reproductive output; cover shelters part of it.
            let cap_of = |j: usize| -> f32 {
                let fec = annual(
                    &means[j],
                    pc,
                    &Eco {
                        food_ratio: 1.0,
                        mean_plant_eff: derived[j].plant_eff,
                        mean_acc: 1.0,
                        ..Default::default()
                    },
                    bg,
                )
                .fecundity;
                let vulnerable = (0.08 + 0.3 * fec).clamp(0.08, 0.7) * (1.0 - 0.4 * pc.cover);
                vulnerable * self.demes[j].total() as f32
            };
            let mut requests: Vec<(usize, usize, f32, f32)> = vec![]; // (predator, prey, kills, need_kills)
            for &k in ks {
                let pd = &derived[k];
                if pd.meat_eff < 0.3 {
                    continue;
                }
                let pm = means[k].get(Trait::BodyMass);
                let npred = self.demes[k].total() as f32;
                let prey: Vec<(usize, f32)> = ks
                    .iter()
                    .filter(|&&j| j != k && self.demes[j].arch != self.demes[k].arch)
                    .filter(|&&j| {
                        let m = means[j].get(Trait::BodyMass);
                        m < 2.0 * pm * (1.0 + 0.4 * means[k].get(Trait::Sociality) * 3.0)
                            && m > 0.005 * pm
                    })
                    // Some prey are always out of reach (refuges in cover).
                    .map(|&j| {
                        let refuge = 0.1 + 0.4 * pc.cover;
                        (
                            j,
                            self.demes[j].total() as f32
                                * (1.0 - refuge)
                                * catchability(&means[j], &derived[j], pc.luminance),
                        )
                    })
                    .collect();
                // Work in energy: catchable prey energy per predator vs. energy needed.
                let avail_kj: f32 = prey
                    .iter()
                    .map(|&(j, w)| w * kill_kj(&derived[j], means[j].get(Trait::BodyMass)))
                    .sum();
                if avail_kj <= 0.0 {
                    continue;
                }
                // Omnivores only hunt for the meat part of their diet.
                let meat_share = pd.meat_eff / (pd.plant_eff + pd.meat_eff).max(1e-3);
                let need_kj = base_need[k] * meat_share / pd.meat_eff.max(0.05);
                let attack =
                    (pd.vmax_kmh / 40.0).clamp(0.3, 3.0) * (0.5 + means[k].get(Trait::Senses));
                // Ratio-dependent (Arditi & Ginzburg 1989), sigmoid (type III) response:
                // intake depends on catchable prey energy per predator, so the result does
                // not depend on tile size, and hunting eases off when prey are scarce.
                let ratio = avail_kj / npred.max(1.0);
                let half = 0.4 * need_kj / attack;
                // Satiation: predators kill little beyond their needs.
                let intake_kj = 1.1 * need_kj * ratio * ratio / (ratio * ratio + half * half);
                for &(j, w) in &prey {
                    let kj = kill_kj(&derived[j], means[j].get(Trait::BodyMass));
                    let share = w * kj / avail_kj;
                    // (predator, prey, kills of this prey, predator need in kills-equivalent)
                    requests.push((k, j, intake_kj * npred * share / kj, need_kj * npred / kj));
                }
            }
            let mut asked: BTreeMap<usize, f32> = BTreeMap::new();
            for &(_, j, kills, _) in &requests {
                *asked.entry(j).or_default() += kills;
            }
            let mut kills_on: BTreeMap<usize, f32> = BTreeMap::new();
            let mut got: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
            for &(k, j, kills, need_eq) in &requests {
                let scale = (cap_of(j) / asked[&j].max(1e-6)).min(1.0);
                *kills_on.entry(j).or_default() += kills * scale;
                let kj = kill_kj(&derived[j], means[j].get(Trait::BodyMass));
                let e = got.entry(k).or_insert((0.0, need_eq * kj));
                e.0 += kills * scale * kj;
            }
            // Carrion: a share of animals dying of other causes is found by scavengers.
            let carrion_kj: f32 = ks
                .iter()
                .filter(|&&j| derived[j].meat_eff < 0.3)
                .map(|&j| {
                    self.demes[j].total() as f32
                        * 0.3
                        * 0.25
                        * kill_kj(&derived[j], means[j].get(Trait::BodyMass))
                })
                .sum();
            let scavenger_need: f32 = ks
                .iter()
                .filter(|&&k| derived[k].meat_eff >= 0.3)
                .map(|&k| {
                    self.demes[k].total() as f32 * base_need[k] / derived[k].meat_eff.max(0.05)
                })
                .sum();
            let carrion_ratio = if scavenger_need > 0.0 {
                (carrion_kj / scavenger_need).min(0.5)
            } else {
                0.0
            };
            for &k in ks {
                let n = self.demes[k].total() as f32;
                let dd = &derived[k];
                let hunted = got
                    .get(&k)
                    .map(|&(g, need)| g / need.max(1e-6))
                    .unwrap_or(0.0);
                let meat_ratio = if dd.meat_eff >= 0.3 {
                    (hunted + carrion_ratio).min(1.5)
                } else {
                    0.0
                };
                let plant = plant_ratio_of(k).min(1.5);
                let food = if dd.meat_eff >= 0.3 {
                    // Omnivores mix sources, or fall back on one at its own efficiency.
                    let best = dd.plant_eff.max(dd.meat_eff);
                    let share = dd.plant_eff / (dd.plant_eff + dd.meat_eff).max(1e-3);
                    (share * plant + (1.0 - share) * meat_ratio)
                        .max(plant * dd.plant_eff / best)
                        .max(meat_ratio * dd.meat_eff / best)
                } else {
                    plant
                };
                let kills = kills_on.get(&k).copied().unwrap_or(0.0);
                let catch = catchability(&means[k], dd, pc.luminance);
                let pref = means[k].get(Trait::Preference);
                let choosy = means[k].get(Trait::Choosiness);
                eco[k] = Eco {
                    food_ratio: food,
                    pred_hazard: if n > 0.0 {
                        -(1.0 - kills / n).max(0.01).ln()
                    } else {
                        0.0
                    },
                    mean_catch: catch,
                    mean_plant_eff: if eff_n > 0.0 {
                        eff_sum / eff_n
                    } else {
                        dd.plant_eff
                    },
                    mean_pref: pref,
                    mean_choosy: choosy,
                    mean_acc: mate_acceptance(pref, choosy, means[k].get(Trait::Ornament)),
                };
            }
        }

        let rates = self.mutation_rates();
        let seed = self.seed;
        let (dmi_p, dmi_s) = (self.params.dmi_probability, self.params.dmi_cost);
        let mut demes = std::mem::take(&mut self.demes);
        let mut diag = Vec::with_capacity(demes.len());
        for (k, deme) in demes.iter_mut().enumerate() {
            let mut rng = Rng::keyed(
                seed,
                tick,
                k as u64 ^ ((deme.species as u64) << 32) ^ ((deme.patch as u64) << 48),
                stream::POPULATION,
            );
            let arch = &self.archs[deme.arch as usize];
            let pc = &ctxs[deme.patch as usize];
            let e = eco[k];
            let base = annual(&means[k], pc, &e, bg);
            diag.push((deme.species, deme.patch, deme.total(), base));
            let d = &derived[k];

            // Selection gradient on the latent scale.
            let latent = deme.mean_latent(arch, &self.alleles);
            let delta = 0.05f32;
            let mut beta = [0f32; N_TRAITS];
            for (t, b) in beta.iter_mut().enumerate() {
                let mut hi = latent;
                let mut lo = latent;
                hi[t] += delta;
                lo[t] -= delta;
                let wh = fitness(&Phenotype::from_latent(&hi), pc, &e, bg, base.fecundity);
                let wl = fitness(&Phenotype::from_latent(&lo), pc, &e, bg, base.fecundity);
                *b = ((wh.ln() - wl.ln()) / (2.0 * delta)).clamp(-3.0, 3.0);
            }
            for &li in &arch.trait_loci {
                let pool = &mut deme.freqs[li];
                let mut mean_e = [0f32; 2];
                for &(a, p) in pool.iter() {
                    let al = self.alleles.get(a);
                    for s in 0..2 {
                        mean_e[s] += p * al.effects[s].1;
                    }
                }
                for (a, p) in pool.iter_mut() {
                    let al = self.alleles.get(*a);
                    let mut excess = 0.0;
                    for s in 0..2 {
                        let t = al.effects[s].0;
                        if t != NO_TRAIT {
                            excess += beta[t as usize] * (al.effects[s].1 - mean_e[s]);
                        }
                    }
                    *p = (*p * (1.0 + excess.clamp(-0.5, 0.5))).max(0.0);
                }
                normalise(pool);
            }
            // Selection against incompatible combinations within the deme. Alleles rarer
            // than 0.5% are ignored here (they almost never meet a partner).
            let common: Vec<(usize, u32, f32)> = arch
                .dmi_loci
                .iter()
                .flat_map(|&l| deme.freqs[l].iter().map(move |&(a, p)| (l, a, p)))
                .filter(|&(_, a, p)| p > 0.005 && self.alleles.get(a).derived)
                .collect();
            if common.len() > 1 {
                for &(l, a, _) in &common {
                    let cost: f32 = common
                        .iter()
                        .filter(|&&(lb, b, _)| lb != l && incompatible(seed, a, b, dmi_p))
                        .map(|&(_, _, q)| 2.0 * q * dmi_s as f32)
                        .sum();
                    if cost > 0.0 {
                        let pool = &mut deme.freqs[l];
                        if let Some(x) = pool.iter_mut().find(|x| x.0 == a) {
                            x.1 *= (1.0 - cost).max(0.0);
                        }
                        normalise(pool);
                    }
                }
            }
            // Demography.
            let mat = base.maturity_years;
            let mut births = 0u64;
            let mut adults_f = 0u32;
            let mut adults_m = 0u32;
            for age in (0..AGE_CLASSES).rev() {
                let s = (-(base.hazard + gompertz_hazard(d, age as f32 + 0.5, 0.0))).exp() as f64;
                let breeding = (age as f32 + 1.0 - mat).clamp(0.0, 1.0);
                let nf = deme.females[age];
                births += rng.poisson(nf as f64 * breeding as f64 * base.fecundity as f64);
                if breeding > 0.0 {
                    adults_f += nf;
                    adults_m += deme.males[age];
                }
                let sf = rng.binomial(nf as u64, s) as u32;
                let sm = rng.binomial(deme.males[age] as u64, s) as u32;
                let next = (age + 1).min(AGE_CLASSES - 1);
                if age == AGE_CLASSES - 1 {
                    deme.females[next] = sf;
                    deme.males[next] = sm;
                } else {
                    deme.females[next] += sf;
                    deme.males[next] += sm;
                }
                deme.females[age] = if age == AGE_CLASSES - 1 {
                    deme.females[age]
                } else {
                    0
                };
                deme.males[age] = if age == AGE_CLASSES - 1 {
                    deme.males[age]
                } else {
                    0
                };
            }
            if adults_m == 0 {
                births = 0;
            }
            let births = births.min(5_000_000);
            let bf = rng.binomial(births, 0.5) as u32;
            deme.females[0] = bf;
            deme.males[0] = births as u32 - bf;
            deme.condition = e.food_ratio.min(1.0);

            // Drift (Wright–Fisher with effective size) and mutation.
            let n = deme.total();
            if n == 0 {
                continue;
            }
            let (af, am) = (adults_f.max(1) as f64, adults_m.max(1) as f64);
            let ne = (4.0 * af * am / (af + am)).min(n as f64).max(2.0);
            let copies = (2.0 * ne).round().max(2.0) as u64;
            for pool in deme.freqs.iter_mut() {
                multinomial_resample(pool, copies, &mut rng);
                prune_rare(pool, MAX_ALLELES_PER_LOCUS);
            }
            // Mutation. The expected number of new mutations in the deme is 2·B·μ summed over
            // loci; at most MAX_NEW_MUTATIONS are tracked per year (in large demes the rest
            // would almost all be lost to drift within a few generations anyway).
            let new_copies = 2 * births;
            if new_copies > 0 {
                let lt = arch.trait_loci.len() as f64 * rates.functional;
                let ln = arch.neutral_loci.len() as f64 * rates.neutral;
                let ld = arch.dmi_loci.len() as f64 * rates.dmi;
                let k = rng
                    .poisson(new_copies as f64 * (lt + ln + ld))
                    .min(MAX_NEW_MUTATIONS);
                for _ in 0..k {
                    let r = rng.f64() * (lt + ln + ld);
                    let li = if r < lt {
                        arch.trait_loci[rng.below(arch.trait_loci.len())]
                    } else if r < lt + ln {
                        arch.neutral_loci[rng.below(arch.neutral_loci.len())]
                    } else {
                        arch.dmi_loci[rng.below(arch.dmi_loci.len())]
                    };
                    let pool = &deme.freqs[li];
                    let weights: Vec<f64> = pool.iter().map(|x| x.1 as f64).collect();
                    let parent = pool[rng.weighted(&weights)].0;
                    let id = arch.new_allele(parent, li, &mut self.alleles, &rates, year, &mut rng);
                    let f = 1.0 / (2.0 * n as f32);
                    let pool = &mut deme.freqs[li];
                    for x in pool.iter_mut() {
                        x.1 *= 1.0 - f;
                    }
                    pool.push((id, f));
                }
            }
        }

        // Migration and colonisation (stepping-stone, plus rare rafting).
        let pw = self.patches_w() as i64;
        let ph = self.patches_h() as i64;
        let mut moves: Vec<(usize, u32, u32)> = vec![];
        for (k, deme) in demes.iter().enumerate() {
            let n = deme.total();
            if n < 2 {
                continue;
            }
            let mut rng = Rng::keyed(seed, tick, k as u64, stream::POPULATION ^ 0x55);
            let m = means.get(k).copied().unwrap_or(Phenotype([0.5; N_TRAITS]));
            let disp_km =
                (5.0 + 80.0 * m.get(Trait::Dispersal)) * m.get(Trait::BodyMass).powf(0.25);
            let patch_km = self.params.patch_size as f32 * self.world.tile_km as f32;
            // Density-dependent emigration: hungry demes send out more dispersers.
            let hunger = (1.0 - eco.get(k).map(|e| e.food_ratio).unwrap_or(1.0)).max(0.0);
            let rate = ((disp_km / patch_km).clamp(0.0, 0.5) * 0.5 + 0.5 * hunger).min(0.6);
            let (px, py) = (deme.patch as i64 % pw, deme.patch as i64 / pw);
            let mut targets = vec![];
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let ny = py + dy;
                    if ny < 0 || ny >= ph {
                        continue;
                    }
                    let nx = (px + dx).rem_euclid(pw);
                    let t = (ny * pw + nx) as usize;
                    if ctxs[t].land.len() >= 2
                        && ctxs[t].plant_supply_g + ctxs[t].rough_supply_g > 0.0
                    {
                        targets.push(t as u32);
                    }
                }
            }
            let emig = rng.binomial(n as u64, rate as f64) as u32;
            if !targets.is_empty() && emig > 0 {
                // Emigrants go to two random neighbouring patches this year (same expected flow).
                let picks = targets.len().min(2);
                let each = emig / picks as u32;
                for _ in 0..picks {
                    let t = targets[rng.below(targets.len())];
                    if each > 0 {
                        moves.push((k, t, each));
                    }
                }
            }
            // Rafting / long-distance dispersal across water.
            if rng.chance(0.002 * m.get(Trait::Dispersal) as f64) {
                let dx = rng.below(7) as i64 - 3;
                let dy = rng.below(7) as i64 - 3;
                let ny = py + dy;
                if (0..ph).contains(&ny) {
                    let t = (ny * pw + (px + dx).rem_euclid(pw)) as usize;
                    if ctxs[t].land.len() >= 2
                        && ctxs[t].plant_supply_g > 0.0
                        && t as u32 != deme.patch
                    {
                        moves.push((k, t as u32, 2.min(n)));
                    }
                }
            }
        }
        let mut where_is: std::collections::HashMap<(u32, u32), usize> = demes
            .iter()
            .enumerate()
            .map(|(i, d)| ((d.species, d.patch), i))
            .collect();
        for (k, to, count) in moves {
            let mut rng = Rng::keyed(
                seed,
                tick,
                (k as u64) << 20 | to as u64,
                stream::POPULATION ^ 0x77,
            );
            // Young adults disperse: take from the youngest classes with individuals.
            let mut females = vec![0u32; AGE_CLASSES];
            let mut males = vec![0u32; AGE_CLASSES];
            let mut taken = 0;
            {
                let src = &mut demes[k];
                'outer: for age in 0..AGE_CLASSES {
                    for sexf in [true, false] {
                        while taken < count {
                            let c = if sexf {
                                &mut src.females[age]
                            } else {
                                &mut src.males[age]
                            };
                            if *c == 0 {
                                break;
                            }
                            *c -= 1;
                            if sexf {
                                females[age] += 1
                            } else {
                                males[age] += 1
                            }
                            taken += 1;
                            if rng.chance(0.5) {
                                break;
                            }
                        }
                        if taken >= count {
                            break 'outer;
                        }
                    }
                }
            }
            if taken == 0 {
                continue;
            }
            let species = demes[k].species;
            match where_is.get(&(species, to)).copied() {
                Some(j) if j != k => {
                    let (src, dst) = if k < j {
                        let (a, b) = demes.split_at_mut(j);
                        (&a[k], &mut b[0])
                    } else {
                        let (a, b) = demes.split_at_mut(k);
                        (&b[0], &mut a[j])
                    };
                    let (n1, n2) = (dst.total() as f32, taken as f32);
                    let w1 = n1 / (n1 + n2);
                    for (l, pool) in dst.freqs.iter_mut().enumerate() {
                        *pool = merge_sorted(pool, &src.freqs[l], w1);
                    }
                    for a in 0..AGE_CLASSES {
                        dst.females[a] += females[a];
                        dst.males[a] += males[a];
                    }
                    dst.condition = dst.condition * w1 + src.condition * (1.0 - w1);
                }
                Some(_) => {}
                None => {
                    let src = &demes[k];
                    let colonists = Deme {
                        species,
                        arch: src.arch,
                        patch: to,
                        females,
                        males,
                        freqs: src.freqs.clone(),
                        condition: src.condition,
                    };
                    where_is.insert((species, to), demes.len());
                    demes.push(colonists);
                }
            }
        }
        demes.retain(|d| d.total() > 0);
        demes.sort_by_key(|d| (d.species, d.patch));
        self.demes = demes;
        self.diagnostics = diag;
        self.world.grow_vegetation_year();
        self.minutes += self.minutes_per_year() as u64;
    }

    /// Mode I → P: aggregate animals into demes (TRD TR-X1).
    pub(crate) fn absorb_into_demes(&mut self, animals: Vec<Animal>) {
        let mut groups: BTreeMap<(u32, u32), Vec<Animal>> = BTreeMap::new();
        for a in animals {
            let p = self.patch_of_tile_xy(a.x, a.y);
            groups.entry((a.species, p)).or_default().push(a);
        }
        let now = self.minutes;
        let dm = self.params.day_minutes;
        for ((species, patch), members) in groups {
            let arch = members[0].arch;
            let nloci = self.archs[arch as usize].n_loci();
            let mut counts: Vec<BTreeMap<u32, u32>> = vec![BTreeMap::new(); nloci];
            let mut d = Deme {
                species,
                arch,
                patch,
                females: vec![0; AGE_CLASSES],
                males: vec![0; AGE_CLASSES],
                freqs: vec![],
                condition: 0.0,
            };
            for a in &members {
                let age = ((a.age_days(now, dm) / 365.25) as usize).min(AGE_CLASSES - 1);
                match a.sex {
                    Sex::Female => d.females[age] += 1,
                    Sex::Male => d.males[age] += 1,
                }
                d.condition += a.energy / a.d.fat_cap_kj.max(1.0);
                for (l, c) in counts.iter_mut().enumerate() {
                    *c.entry(a.genome.hap[0][l]).or_default() += 1;
                    *c.entry(a.genome.hap[1][l]).or_default() += 1;
                }
            }
            d.condition /= members.len() as f32;
            let tot = 2.0 * members.len() as f32;
            d.freqs = counts
                .into_iter()
                .map(|c| c.into_iter().map(|(a, k)| (a, k as f32 / tot)).collect())
                .collect();
            if let Some(dst) = self
                .demes
                .iter_mut()
                .find(|x| x.species == species && x.patch == patch)
            {
                dst.merge(&d);
            } else {
                self.demes.push(d);
            }
        }
        self.demes.sort_by_key(|d| (d.species, d.patch));
    }

    /// Mode P → I: create individuals by sampling genomes from deme frequencies (TRD TR-X2).
    pub(crate) fn materialize_demes(&mut self) {
        let demes = std::mem::take(&mut self.demes);
        let ctxs = self.patch_contexts();
        for (k, d) in demes.iter().enumerate() {
            let mut rng = Rng::keyed(self.seed, self.minutes, k as u64, stream::TRANSITION);
            let land: Vec<usize> = {
                let pc = &ctxs[d.patch as usize];
                let good: Vec<usize> = pc
                    .land
                    .iter()
                    .copied()
                    .filter(|&t| self.world.forage_max[t] > 0.0)
                    .collect();
                if good.is_empty() {
                    pc.land.clone()
                } else {
                    good
                }
            };
            if land.is_empty() {
                continue;
            }
            for age in 0..AGE_CLASSES {
                for (sex, count) in [(Sex::Female, d.females[age]), (Sex::Male, d.males[age])] {
                    for _ in 0..count {
                        let g = d.sample_genome(&mut rng);
                        let t = land[rng.below(land.len())];
                        let pos = (
                            (t % self.world.w) as f32 + rng.f32(),
                            (t / self.world.w) as f32 + rng.f32(),
                        );
                        let age_days = (age as f32 + rng.f32()) * 365.25;
                        let a = self.build_animal(
                            d.species,
                            d.arch,
                            g,
                            sex,
                            pos,
                            age_days,
                            0.5 + 0.3 * d.condition,
                            (0, 0),
                            0,
                            0,
                            &mut rng,
                        );
                        self.animals.push(a);
                    }
                }
            }
        }
    }

    pub(crate) fn pops_from_demes(&self) -> (Vec<PopSummary>, BTreeMap<(u32, u32), usize>) {
        let pops = self
            .demes
            .iter()
            .enumerate()
            .map(|(k, d)| {
                let arch = &self.archs[d.arch as usize];
                let p = Phenotype::from_latent(&d.mean_latent(arch, &self.alleles));
                let dmi = arch
                    .dmi_loci
                    .iter()
                    .flat_map(|&l| d.freqs[l].iter().map(move |&(a, f)| (l as u16, a, f)))
                    .filter(|&(_, a, f)| f > 0.0 && self.alleles.get(a).derived)
                    .collect();
                PopSummary {
                    key: k,
                    species: d.species,
                    arch: d.arch,
                    patch: d.patch,
                    n: d.total(),
                    dmi,
                    ornament: p.get(Trait::Ornament),
                    preference: p.get(Trait::Preference),
                    choosiness: p.get(Trait::Choosiness),
                    breeding_day: p.get(Trait::BreedingDay),
                }
            })
            .collect();
        (pops, BTreeMap::new())
    }
}

fn normalise(pool: &mut [(u32, f32)]) {
    let s: f32 = pool.iter().map(|x| x.1).sum();
    if s <= 0.0 {
        let n = pool.len().max(1) as f32;
        for x in pool.iter_mut() {
            x.1 = 1.0 / n;
        }
        return;
    }
    for x in pool.iter_mut() {
        x.1 /= s;
    }
}

/// Keeps the `max` most common alleles at a locus (ties broken by id) and renormalises.
fn prune_rare(pool: &mut Vec<(u32, f32)>, max: usize) {
    if pool.len() <= max {
        return;
    }
    pool.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    pool.truncate(max);
    pool.sort_by_key(|x| x.0);
    normalise(pool);
}

/// Wright–Fisher resampling of `copies` gene copies; drops lost alleles.
pub fn multinomial_resample(pool: &mut Vec<(u32, f32)>, copies: u64, rng: &mut Rng) {
    let mut left = copies;
    let mut rest = 1.0f64;
    for x in pool.iter_mut() {
        if left == 0 || rest <= 0.0 {
            x.1 = 0.0;
            continue;
        }
        let p = (x.1 as f64 / rest).clamp(0.0, 1.0);
        let k = rng.binomial(left, p);
        rest -= x.1 as f64;
        left -= k;
        x.1 = k as f32 / copies as f32;
    }
    pool.retain(|x| x.1 > 0.0);
    if pool.is_empty() {
        return;
    }
    normalise(pool);
}
