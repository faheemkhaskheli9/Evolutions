//! Genome model (TRD §5.4): diploid chromosomes with mapped loci, a global
//! append-only allele table (continuum-of-alleles), meiosis with crossing over,
//! mutation, genotype → genetic value with dominance and pleiotropy, and
//! Dobzhansky–Muller incompatibilities.

use crate::rng::{hash4, hash_unit, Rng};
use crate::traits::{Trait, N_TRAITS};
use serde::{Deserialize, Serialize};

pub const NO_TRAIT: u8 = u8::MAX;
pub const NO_PARENT: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocusKind {
    /// Affects quantitative traits (primary trait index).
    Trait(u8),
    /// Neutral marker, used as a molecular clock and for diversity statistics.
    Neutral,
    /// Hybrid-incompatibility locus.
    Dmi,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Locus {
    pub chrom: u8,
    /// Genetic map position, Morgans.
    pub pos: f32,
    pub kind: LocusKind,
}

/// One allele: its effects on up to two traits (pleiotropy) and its dominance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Allele {
    pub locus: u16,
    pub parent: u32,
    pub origin_year: f32,
    pub effects: [(u8, f32); 2],
    /// 0 = recessive, 0.5 = additive, 1 = dominant (for the larger-effect allele).
    pub dominance: f32,
    /// False for founder alleles; true for alleles produced by mutation in the simulation.
    pub derived: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AlleleTable {
    pub alleles: Vec<Allele>,
}

impl AlleleTable {
    pub fn push(&mut self, a: Allele) -> u32 {
        self.alleles.push(a);
        (self.alleles.len() - 1) as u32
    }

    #[inline]
    pub fn get(&self, id: u32) -> &Allele {
        &self.alleles[id as usize]
    }

    pub fn len(&self) -> usize {
        self.alleles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.alleles.is_empty()
    }
}

/// Diploid genome: two haplotypes of allele ids, one entry per locus.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Genome {
    pub hap: [Vec<u32>; 2],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MutationRates {
    pub functional: f64,
    pub neutral: f64,
    pub dmi: f64,
    pub large_effect_fraction: f64,
}

/// Genome architecture shared by a founder species and all its descendants.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenomeArch {
    pub id: u16,
    pub chrom_len: Vec<f32>,
    /// Locus index range per chromosome (loci are sorted by chromosome, position).
    pub chrom_range: Vec<(usize, usize)>,
    pub loci: Vec<Locus>,
    pub trait_loci: Vec<usize>,
    pub neutral_loci: Vec<usize>,
    pub dmi_loci: Vec<usize>,
    /// Latent-scale intercept per trait.
    pub base: [f32; N_TRAITS],
    /// Environmental (non-heritable) SD per trait, latent scale.
    pub env_sd: [f32; N_TRAITS],
    /// SD of mutational effects per trait, latent scale.
    pub mut_sd: [f32; N_TRAITS],
    /// Founder allele ids available at each locus.
    pub founder_alleles: Vec<Vec<u32>>,
}

pub struct FounderSpec {
    /// Latent-scale mean and phenotypic latent SD per trait.
    pub latent_mean: [f32; N_TRAITS],
    pub latent_sd: [f32; N_TRAITS],
    /// 0 = genetically uniform (bottleneck) … 1 = high standing variation.
    pub diversity: f32,
}

impl GenomeArch {
    pub fn n_loci(&self) -> usize {
        self.loci.len()
    }

    /// Builds a new architecture and its founder alleles so that founders have
    /// the requested trait means, phenotypic variances and heritabilities.
    pub fn create(id: u16, spec: &FounderSpec, table: &mut AlleleTable, rng: &mut Rng) -> Self {
        let n_chrom = 8usize;
        let chrom_len: Vec<f32> = (0..n_chrom).map(|_| rng.range_f32(0.6, 1.6)).collect();
        let mut loci: Vec<Locus> = Vec::new();
        let mut place = |kind: LocusKind, rng: &mut Rng| {
            let c = rng.below(n_chrom);
            loci.push(Locus {
                chrom: c as u8,
                pos: rng.f32() * chrom_len[c],
                kind,
            });
        };
        for t in Trait::ALL {
            for _ in 0..t.info().loci {
                place(LocusKind::Trait(t as u8), rng);
            }
        }
        for _ in 0..24 {
            place(LocusKind::Neutral, rng);
        }
        for _ in 0..16 {
            place(LocusKind::Dmi, rng);
        }
        loci.sort_by(|a, b| {
            a.chrom
                .cmp(&b.chrom)
                .then(a.pos.partial_cmp(&b.pos).unwrap())
        });
        let mut chrom_range = vec![(0usize, 0usize); n_chrom];
        for c in 0..n_chrom {
            let s = loci.iter().position(|l| l.chrom as usize == c);
            let e = loci.iter().rposition(|l| l.chrom as usize == c);
            chrom_range[c] = match (s, e) {
                (Some(s), Some(e)) => (s, e + 1),
                _ => (0, 0),
            };
        }
        let mut trait_loci = vec![];
        let mut neutral_loci = vec![];
        let mut dmi_loci = vec![];
        for (i, l) in loci.iter().enumerate() {
            match l.kind {
                LocusKind::Trait(_) => trait_loci.push(i),
                LocusKind::Neutral => neutral_loci.push(i),
                LocusKind::Dmi => dmi_loci.push(i),
            }
        }

        let k = 1 + (spec.diversity.clamp(0.0, 1.0) * 4.0).round() as usize;
        let mut env_sd = [0f32; N_TRAITS];
        let mut mut_sd = [0f32; N_TRAITS];
        let mut sigma_a = [0f32; N_TRAITS];
        for t in Trait::ALL {
            let i = t as usize;
            let vp = spec.latent_sd[i].powi(2);
            let h2 = t.heritability();
            let n_t = t.info().loci as f32;
            let kf = k.max(2) as f32;
            // V_A = 2 Σ var(allele effects); var over k equal-frequency alleles ≈ σ²(k-1)/k.
            sigma_a[i] = (h2 * vp / (2.0 * n_t * (kf - 1.0) / kf)).sqrt();
            env_sd[i] = ((1.0 - h2) * vp).sqrt();
            mut_sd[i] = sigma_a[i] * 0.35;
        }

        let mut founder_alleles = Vec::with_capacity(loci.len());
        for (li, l) in loci.iter().enumerate() {
            let mut ids = vec![];
            match l.kind {
                LocusKind::Trait(t) => {
                    let ti = t as usize;
                    // Pleiotropy: some loci also affect a second trait, with a locus-specific correlation.
                    let second = if rng.chance(0.2) {
                        let s = rng.below(N_TRAITS);
                        if s != ti {
                            Some((s as u8, rng.normal_f32(0.0, 0.5)))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let raw: Vec<f32> = (0..k).map(|_| rng.normal_f32(0.0, sigma_a[ti])).collect();
                    let mean = raw.iter().sum::<f32>() / k as f32;
                    for e in raw {
                        let e = if k > 1 { e - mean } else { 0.0 };
                        let effects = match second {
                            Some((s, c)) => [
                                (t, e),
                                (s, e * c * sigma_a[s as usize] / sigma_a[ti].max(1e-6)),
                            ],
                            None => [(t, e), (NO_TRAIT, 0.0)],
                        };
                        ids.push(table.push(Allele {
                            locus: li as u16,
                            parent: NO_PARENT,
                            origin_year: 0.0,
                            effects,
                            dominance: 0.5,
                            derived: false,
                        }));
                    }
                }
                LocusKind::Neutral => {
                    for _ in 0..(k + 1) {
                        ids.push(table.push(Allele {
                            locus: li as u16,
                            parent: NO_PARENT,
                            origin_year: 0.0,
                            effects: [(NO_TRAIT, 0.0), (NO_TRAIT, 0.0)],
                            dominance: 0.5,
                            derived: false,
                        }));
                    }
                }
                LocusKind::Dmi => {
                    ids.push(table.push(Allele {
                        locus: li as u16,
                        parent: NO_PARENT,
                        origin_year: 0.0,
                        effects: [(NO_TRAIT, 0.0), (NO_TRAIT, 0.0)],
                        dominance: 0.5,
                        derived: false,
                    }));
                }
            }
            founder_alleles.push(ids);
        }

        GenomeArch {
            id,
            chrom_len,
            chrom_range,
            loci,
            trait_loci,
            neutral_loci,
            dmi_loci,
            base: spec.latent_mean,
            env_sd,
            mut_sd,
            founder_alleles,
        }
    }

    /// A random founder genome drawn from the founder allele pool.
    pub fn founder_genome(&self, rng: &mut Rng) -> Genome {
        let mut hap = [
            Vec::with_capacity(self.n_loci()),
            Vec::with_capacity(self.n_loci()),
        ];
        for h in hap.iter_mut() {
            for pool in &self.founder_alleles {
                h.push(pool[rng.below(pool.len())]);
            }
        }
        Genome { hap }
    }

    /// Meiosis: one recombinant haplotype from a diploid genome (TRD TR-G5).
    pub fn meiosis(&self, g: &Genome, rng: &mut Rng) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.n_loci());
        let mut xs: Vec<f32> = Vec::with_capacity(4);
        for (c, &(s, e)) in self.chrom_range.iter().enumerate() {
            if s == e {
                continue;
            }
            let len = self.chrom_len[c];
            xs.clear();
            for _ in 0..rng.poisson(len as f64) {
                xs.push(rng.f32() * len);
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut strand = rng.below(2);
            let mut xi = 0;
            for li in s..e {
                let pos = self.loci[li].pos;
                while xi < xs.len() && xs[xi] < pos {
                    strand ^= 1;
                    xi += 1;
                }
                out.push(g.hap[strand][li]);
            }
        }
        out
    }

    /// Applies new mutations to a gamete in place (TRD TR-G6). Returns how many occurred.
    pub fn mutate(
        &self,
        gamete: &mut [u32],
        table: &mut AlleleTable,
        rates: &MutationRates,
        year: f32,
        rng: &mut Rng,
    ) -> u32 {
        let lt = self.trait_loci.len() as f64 * rates.functional;
        let ln = self.neutral_loci.len() as f64 * rates.neutral;
        let ld = self.dmi_loci.len() as f64 * rates.dmi;
        let n = rng.poisson(lt + ln + ld);
        for _ in 0..n {
            let r = rng.f64() * (lt + ln + ld);
            let li = if r < lt {
                self.trait_loci[rng.below(self.trait_loci.len())]
            } else if r < lt + ln {
                self.neutral_loci[rng.below(self.neutral_loci.len())]
            } else {
                self.dmi_loci[rng.below(self.dmi_loci.len())]
            };
            gamete[li] = self.new_allele(gamete[li], li, table, rates, year, rng);
        }
        n as u32
    }

    pub fn new_allele(
        &self,
        parent: u32,
        locus: usize,
        table: &mut AlleleTable,
        rates: &MutationRates,
        year: f32,
        rng: &mut Rng,
    ) -> u32 {
        let p = table.get(parent).clone();
        let mut effects = p.effects;
        let mut dominance = 0.5;
        if let LocusKind::Trait(_) = self.loci[locus].kind {
            let big = if rng.chance(rates.large_effect_fraction) {
                5.0
            } else {
                1.0
            };
            for e in effects.iter_mut() {
                if e.0 != NO_TRAIT {
                    e.1 += rng.normal_f32(0.0, self.mut_sd[e.0 as usize] * big);
                }
            }
            dominance = rng.normal_f32(0.45, 0.2).clamp(0.0, 1.0);
        }
        table.push(Allele {
            locus: locus as u16,
            parent,
            origin_year: year,
            effects,
            dominance,
            derived: true,
        })
    }

    /// Genetic (latent-scale) value of every trait (TRD TR-G3).
    pub fn genetic_values(&self, table: &AlleleTable, g: &Genome) -> [f32; N_TRAITS] {
        let mut v = self.base;
        for &li in &self.trait_loci {
            let a = table.get(g.hap[0][li]);
            let b = table.get(g.hap[1][li]);
            let hbar = 0.5 * (a.dominance + b.dominance);
            add_locus(&mut v, a, b, hbar);
        }
        v
    }

    /// Expected heterozygosity proxy for one genome: fraction of heterozygous neutral loci.
    pub fn neutral_heterozygosity(&self, g: &Genome) -> f32 {
        let n = self.neutral_loci.len().max(1) as f32;
        self.neutral_loci
            .iter()
            .filter(|&&l| g.hap[0][l] != g.hap[1][l])
            .count() as f32
            / n
    }
}

fn add_locus(v: &mut [f32; N_TRAITS], a: &Allele, b: &Allele, hbar: f32) {
    let dom = 2.0 * hbar - 1.0;
    // Same trait slots in both alleles because mutations keep the locus effect structure.
    for s in 0..2 {
        let t = a.effects[s].0;
        if t == NO_TRAIT {
            continue;
        }
        let (ea, eb) = (a.effects[s].1, b.effects[s].1);
        v[t as usize] += ea + eb + dom * (ea - eb).abs();
    }
}

/// Whether two derived incompatibility alleles clash (deterministic in the world seed).
#[inline]
pub fn incompatible(seed: u64, a: u32, b: u32, p: f64) -> bool {
    let (x, y) = if a < b { (a, b) } else { (b, a) };
    hash_unit(hash4(seed, x as u64, y as u64, 0xD3A1)) < p
}

/// Hybrid viability multiplier (1 - s)^n for n incompatible pairs (TRD TR-G10).
pub fn dmi_viability(
    arch: &GenomeArch,
    table: &AlleleTable,
    g: &Genome,
    seed: u64,
    p: f64,
    s: f64,
) -> f64 {
    let mut derived: Vec<(u16, u32)> = Vec::with_capacity(8);
    for &li in &arch.dmi_loci {
        for h in 0..2 {
            let id = g.hap[h][li];
            if table.get(id).derived && !derived.iter().any(|&(_, x)| x == id) {
                derived.push((li as u16, id));
            }
        }
    }
    let mut n = 0;
    for i in 0..derived.len() {
        for j in (i + 1)..derived.len() {
            if derived[i].0 != derived[j].0 && incompatible(seed, derived[i].1, derived[j].1, p) {
                n += 1;
            }
        }
    }
    (1.0 - s).powi(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arch() -> (GenomeArch, AlleleTable) {
        let mut table = AlleleTable::default();
        let mut rng = Rng::new(1);
        let mut mean = [0f32; N_TRAITS];
        let mut sd = [0f32; N_TRAITS];
        for t in Trait::ALL {
            mean[t as usize] = t.to_latent(if t == Trait::BodyMass { 5.0 } else { 0.5 });
            sd[t as usize] = 0.2;
        }
        let spec = FounderSpec {
            latent_mean: mean,
            latent_sd: sd,
            diversity: 0.8,
        };
        let a = GenomeArch::create(0, &spec, &mut table, &mut rng);
        (a, table)
    }

    #[test]
    fn loci_sorted_and_ranges_cover_all() {
        let (a, _) = arch();
        let total: usize = a.chrom_range.iter().map(|(s, e)| e - s).sum();
        assert_eq!(total, a.n_loci());
        for w in a.loci.windows(2) {
            assert!(w[0].chrom < w[1].chrom || (w[0].chrom == w[1].chrom && w[0].pos <= w[1].pos));
        }
    }

    #[test]
    fn meiosis_draws_each_locus_from_a_parent_haplotype() {
        let (a, _) = arch();
        let mut rng = Rng::new(5);
        let g = a.founder_genome(&mut rng);
        let gam = a.meiosis(&g, &mut rng);
        assert_eq!(gam.len(), a.n_loci());
        for (i, x) in gam.iter().enumerate() {
            assert!(*x == g.hap[0][i] || *x == g.hap[1][i]);
        }
    }

    #[test]
    fn founder_genetic_values_center_on_base() {
        let (a, table) = arch();
        let mut rng = Rng::new(9);
        let n = 4000;
        let mut m = 0.0;
        for _ in 0..n {
            let g = a.founder_genome(&mut rng);
            m += a.genetic_values(&table, &g)[0];
        }
        m /= n as f32;
        assert!((m - a.base[0]).abs() < 0.03, "{m} vs {}", a.base[0]);
    }
}
