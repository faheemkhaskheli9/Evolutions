//! Species registry, automatic species detection using the biological species
//! concept (TRD §5.10), the phylogeny and generated names.

use crate::genetics::{incompatible, AlleleTable};
use crate::physiology::{mate_acceptance, year_distance};
use crate::rng::{stream, Rng};
use crate::traits::N_TRAITS;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpeciesInfo {
    pub id: u32,
    pub name: String,
    pub common_name: String,
    pub arch: u16,
    pub parent: Option<u32>,
    pub origin_year: f64,
    pub extinct_year: Option<f64>,
    pub color: [u8; 3],
    pub split_cause: Option<String>,
    /// Mean phenotype when the species arose (for "trait shift" events).
    pub origin_mean: Option<[f32; N_TRAITS]>,
    pub origin_sd: Option<[f32; N_TRAITS]>,
    pub announced_shift: [u8; N_TRAITS],
    pub pending_split: u32,
    pub peak_n: u32,
    pub last_n: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpeciesRegistry {
    pub list: Vec<SpeciesInfo>,
}

impl SpeciesRegistry {
    pub fn get(&self, id: u32) -> &SpeciesInfo {
        &self.list[id as usize]
    }

    pub fn get_mut(&mut self, id: u32) -> &mut SpeciesInfo {
        &mut self.list[id as usize]
    }

    pub fn add(
        &mut self,
        arch: u16,
        parent: Option<u32>,
        common_name: String,
        year: f64,
        color: [u8; 3],
        seed: u64,
    ) -> u32 {
        let id = self.list.len() as u32;
        let genus = parent.map(|p| {
            self.list[p as usize]
                .name
                .split(' ')
                .next()
                .unwrap_or("")
                .to_string()
        });
        let name = generate_name(seed, id as u64, genus);
        self.list.push(SpeciesInfo {
            id,
            name,
            common_name,
            arch,
            parent,
            origin_year: year,
            extinct_year: None,
            color,
            split_cause: None,
            origin_mean: None,
            origin_sd: None,
            announced_shift: [0; N_TRAITS],
            pending_split: 0,
            peak_n: 0,
            last_n: 0,
        });
        id
    }

    pub fn alive(&self) -> impl Iterator<Item = &SpeciesInfo> {
        self.list.iter().filter(|s| s.extinct_year.is_none())
    }
}

const ONSETS: [&str; 18] = [
    "b", "c", "d", "f", "g", "l", "m", "n", "p", "r", "s", "t", "v", "th", "ph", "ch", "tr", "gr",
];
const VOWELS: [&str; 7] = ["a", "e", "i", "o", "u", "ae", "y"];
const GENUS_END: [&str; 6] = ["us", "on", "ax", "odon", "ops", "erium"];
const SPECIES_END: [&str; 6] = ["is", "ensis", "ii", "a", "um", "oides"];

fn syllables(r: &mut Rng, n: usize) -> String {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(ONSETS[r.below(ONSETS.len())]);
        s.push_str(VOWELS[r.below(VOWELS.len())]);
    }
    s
}

/// Deterministic Latin-style binomial. Daughter species keep the parent genus.
pub fn generate_name(seed: u64, id: u64, genus: Option<String>) -> String {
    let mut r = Rng::keyed(seed, id, 0, stream::NAMING);
    let genus = genus.filter(|g| !g.is_empty()).unwrap_or_else(|| {
        let mut g = syllables(&mut r, 2);
        g.push_str(GENUS_END[r.below(GENUS_END.len())]);
        let mut c = g.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => g,
        }
    });
    let mut sp = syllables(&mut r, 2);
    sp.push_str(SPECIES_END[r.below(SPECIES_END.len())]);
    format!("{genus} {sp}")
}

/// Population-level summary used for species detection; built from animals
/// (Individual mode) or demes (Population mode).
#[derive(Clone, Debug)]
pub struct PopSummary {
    pub key: usize,
    pub species: u32,
    pub arch: u16,
    pub patch: u32,
    pub n: u32,
    /// Frequencies of derived alleles at incompatibility loci: (locus, allele uid, freq).
    pub dmi: Vec<(u16, u64, f32)>,
    pub ornament: f32,
    pub preference: f32,
    pub choosiness: f32,
    pub breeding_day: f32,
}

/// Expected reproductive compatibility between two populations in [0,1]:
/// hybrid viability × mutual mate acceptance × overlap of breeding seasons.
pub fn compatibility(a: &PopSummary, b: &PopSummary, seed: u64, p: f64, s: f64) -> f64 {
    if a.arch != b.arch {
        return 0.0;
    }
    let mut expected = 0.0f64;
    for &(la, xa, fa) in &a.dmi {
        for &(lb, xb, fb) in &b.dmi {
            if la != lb && xa != xb && incompatible(seed, xa, xb, p) {
                expected += (fa * fb) as f64;
            }
        }
    }
    let viability = (1.0 - s).powf(expected);
    let rel = |f: &PopSummary, m: &PopSummary| -> f64 {
        let own = mate_acceptance(f.preference, f.choosiness, f.ornament).max(1e-6);
        (mate_acceptance(f.preference, f.choosiness, m.ornament) / own).min(1.0) as f64
    };
    let mating = rel(a, b).min(rel(b, a));
    let d = year_distance(a.breeding_day, b.breeding_day) as f64;
    let timing = (-(d / 0.15).powi(2)).exp();
    viability * mating * timing
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut c = x;
        while self.0[c] != r {
            let n = self.0[c];
            self.0[c] = r;
            c = n;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.0[hi] = lo;
        }
    }
}

/// Result of a species check: populations to relabel, and new species to create.
pub struct Split {
    pub parent: u32,
    /// Population keys that form the new species.
    pub members: Vec<usize>,
    pub cause: String,
}

/// Groups populations into reproductively compatible clusters and decides which
/// existing species have split (with hysteresis: a split must persist across
/// two consecutive checks).
pub fn detect_splits(
    pops: &[PopSummary],
    registry: &mut SpeciesRegistry,
    seed: u64,
    params: &crate::params::SimParams,
    connected: &dyn Fn(u32, u32) -> bool,
) -> Vec<Split> {
    let n = pops.len();
    let mut dsu = Dsu((0..n).collect());
    let try_union = |dsu: &mut Dsu, i: usize, j: usize| {
        // A pair already joined through other populations needs no comparison: the
        // clusters are the connected components, whichever edges are found first.
        if pops[i].arch == pops[j].arch
            && dsu.find(i) != dsu.find(j)
            && compatibility(
                &pops[i],
                &pops[j],
                seed,
                params.dmi_probability,
                params.dmi_cost,
            ) >= params.compat_threshold
        {
            dsu.union(i, j);
        }
    };
    // Populations sharing a patch first: they are the likeliest to be compatible, so
    // most later pairs are already in one cluster and are skipped.
    let mut by_patch: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, p) in pops.iter().enumerate() {
        by_patch.entry(p.patch).or_default().push(i);
    }
    for members in by_patch.values() {
        for (x, &i) in members.iter().enumerate() {
            for &j in &members[x + 1..] {
                try_union(&mut dsu, i, j);
            }
        }
    }
    for i in 0..n {
        for j in (i + 1)..n {
            try_union(&mut dsu, i, j);
        }
    }
    // species → component → (count, members)
    let mut by_species: BTreeMap<u32, BTreeMap<usize, (u32, Vec<usize>)>> = BTreeMap::new();
    for i in 0..n {
        let c = dsu.find(i);
        let e = by_species
            .entry(pops[i].species)
            .or_default()
            .entry(c)
            .or_insert((0, vec![]));
        e.0 += pops[i].n;
        e.1.push(i);
    }
    let mut splits = vec![];
    for (sp, comps) in by_species {
        let total: u32 = comps.values().map(|c| c.0).sum();
        let mut sig: Vec<(u32, Vec<usize>)> = comps
            .into_values()
            .filter(|(c, _)| *c >= 20 && *c as f32 >= 0.08 * total as f32)
            .collect();
        let info = registry.get_mut(sp);
        if sig.len() < 2 {
            info.pending_split = 0;
            continue;
        }
        info.pending_split += 1;
        if info.pending_split < 2 {
            continue;
        }
        info.pending_split = 0;
        sig.sort_by(|a, b| b.0.cmp(&a.0).then(a.1[0].cmp(&b.1[0])));
        let main: Vec<u32> = sig[0].1.iter().map(|&i| pops[i].patch).collect();
        for (_, members) in sig.into_iter().skip(1) {
            let adjacent = members
                .iter()
                .any(|&i| main.iter().any(|&q| connected(pops[i].patch, q)));
            let cause = if adjacent {
                "parapatric/sympatric divergence (reproductive isolation without a barrier)"
            } else {
                "allopatric (geographically separated populations diverged)"
            };
            splits.push(Split {
                parent: sp,
                members: members.iter().map(|&i| pops[i].key).collect(),
                cause: cause.to_string(),
            });
        }
    }
    splits
}

/// Collects derived incompatibility-allele frequencies from genomes.
pub fn dmi_freqs<'a>(
    table: &AlleleTable,
    dmi_loci: &[usize],
    genomes: impl Iterator<Item = &'a crate::genetics::Genome>,
) -> Vec<(u16, u64, f32)> {
    let mut counts: BTreeMap<(u16, u64), u32> = BTreeMap::new();
    let mut n = 0u32;
    for g in genomes {
        n += 2;
        for &l in dmi_loci {
            for h in 0..2 {
                let a = table.get(g.hap[h][l]);
                if a.derived {
                    *counts.entry((l as u16, a.uid)).or_default() += 1;
                }
            }
        }
    }
    counts
        .into_iter()
        .map(|((l, a), c)| (l, a, c as f32 / n.max(1) as f32))
        .collect()
}
