//! Disease and host–pathogen co-evolution (PRD FR-L12, FR-G14; TRD TR-E3).
//!
//! A pathogen is introduced with the Epidemic intervention. From then on:
//!
//! * **Strains** have a heritable virulence (how fast they kill) and transmissibility,
//!   and may *escape* host resistance loci. Each infection has a small chance of
//!   producing a mutant strain. With the **virulence–transmission trade-off** a strain
//!   only transmits well when it also replicates (and so harms) a lot, so intermediate
//!   virulence evolves (Anderson & May 1982).
//! * **Hosts** carry four unlinked resistance loci. Each copy of a resistant allele lowers
//!   the chance of infection by strains that do not escape that locus, and costs a little
//!   survival. Hosts are susceptible, infected or recovered (immune for about a year).
//! * In **Individual mode** an infected animal meets the animals of its species within a
//!   tile once a day; the chance of passing the infection on grows with its sociality and
//!   saturates with crowding. In **Population mode** each deme holds the same states as
//!   counts, stepped weekly through the year with the same rates; infected migrants carry
//!   the disease to neighbouring demes. Macro mode holds disease as it is.
//!
//! Everything lives in [`Disease`] and in `health` fields that are not part of the bincode
//! state. They are saved after the climate section and count towards `state_hash()` only
//! once a pathogen has been introduced, so worlds without disease behave, save and hash
//! as before.

use crate::animal::{Animal, Cause};
use crate::individual::tiles_around;
use crate::population::{neighbour_patches, Deme};
use crate::record::EventKind;
use crate::rng::{hash4, hash_unit, stream, Rng};
use crate::sim::{Mode, Simulation};
use crate::traits::{Phenotype, Trait};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Resistance loci per host.
pub const RES_LOCI: usize = 4;
const RES_MASK: u8 = (1 << RES_LOCI) - 1;
/// Fall in susceptibility for a host homozygous for the resistant allele at one locus
/// (to strains that do not escape it).
pub const PROTECTION: f32 = 0.5;
/// Extra yearly mortality per resistant allele copy.
pub const RES_COST: f32 = 0.005;
/// Mutation rate between the resistant and susceptible alleles, per locus per gamete.
pub const RES_MUTATION: f64 = 1e-4;
/// Recovery rate, per day (infections last about 10 days).
pub const RECOVERY: f32 = 0.1;
/// Death rate from infection at virulence 1, per day.
pub const MAX_DEATH: f32 = 0.15;
/// Loss of immunity, per day.
pub const WANING: f32 = 1.0 / 365.0;
/// Neighbours at which contacts reach half their maximum (crowding saturates contacts).
pub const HALF_CONTACTS: f32 = 10.0;
/// Virulence at which a trade-off strain transmits half as well as at virulence ~1.
pub const TRADE_OFF_K: f32 = 0.15;
/// Chance that an infection founds a new strain.
pub const STRAIN_MUTATION: f64 = 0.002;
/// SD of mutational changes to virulence and transmissibility.
pub const STRAIN_MUT_SD: f32 = 0.05;
/// Transmission of a strain falls by this factor for each resistance locus it escapes.
pub const ESCAPE_COST: f32 = 0.85;
/// Tiles an infected animal reaches (its own and the 8 around it).
const CONTACT_TILES: f32 = 9.0;
/// In Population mode, the share of a deme's contacts made in a neighbouring patch with a
/// fully open border.
const NEIGHBOUR_CONTACT: f32 = 0.02;
const REACH: f32 = 1.0;
/// Population-mode steps per year (about 3.5 days each).
const STEPS: usize = 104;
const SERIES_CAP: usize = 1500;
/// Strains a deme can hold before it stops founding new ones.
const MAX_STRAINS_PER_DEME: usize = 8;
const MAX_STRAINS: usize = 4000;
const NO_STRAIN: u32 = u32::MAX;

/// A pathogen to introduce (PRD FR-E4 "plague outbreak").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathogenSpec {
    /// 0–1: death rate while infected is `MAX_DEATH × virulence` per day.
    pub virulence: f32,
    /// 0–1: transmission per contact.
    pub transmissibility: f32,
    /// Whether transmission rises with virulence (so intermediate virulence evolves).
    #[serde(default = "yes")]
    pub trade_off: bool,
    /// Share of the species infected at the start, in its most crowded places.
    #[serde(default = "default_share")]
    pub share: f32,
}

fn yes() -> bool {
    true
}

fn default_share() -> f32 {
    0.001
}

impl Default for PathogenSpec {
    fn default() -> Self {
        PathogenSpec {
            virulence: 0.3,
            transmissibility: 0.6,
            trade_off: true,
            share: default_share(),
        }
    }
}

impl PathogenSpec {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.01..=1.0).contains(&self.virulence) {
            return Err("virulence must be 0.01–1".into());
        }
        if !(0.01..=1.0).contains(&self.transmissibility) {
            return Err("transmissibility must be 0.01–1".into());
        }
        if !(self.share > 0.0 && self.share <= 1.0) {
            return Err("the share infected must be above 0 and at most 1".into());
        }
        Ok(())
    }
}

/// Infection state of a host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Sir {
    #[default]
    Susceptible,
    /// Infected with a strain (its id).
    Infected(u32),
    Recovered,
}

/// An animal's resistance genotype (one bit per locus on each haplotype) and state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Health {
    pub res: [u8; 2],
    pub sir: Sir,
}

impl Health {
    /// Resistant copies at locus `i`, as a fraction (0, 0.5 or 1).
    pub fn dosage(&self, i: usize) -> f32 {
        (((self.res[0] >> i) & 1) + ((self.res[1] >> i) & 1)) as f32 * 0.5
    }

    pub fn copies(&self) -> u32 {
        (self.res[0] & RES_MASK).count_ones() + (self.res[1] & RES_MASK).count_ones()
    }

    /// Relative chance of infection by a strain escaping the loci in `escape`.
    pub fn susceptibility(&self, escape: u8) -> f32 {
        (0..RES_LOCI)
            .filter(|i| escape >> i & 1 == 0)
            .map(|i| 1.0 - PROTECTION * self.dosage(i))
            .product()
    }
}

/// A deme's resistance allele frequencies and infection counts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DemeHealth {
    pub res: [f32; RES_LOCI],
    /// (strain, infected), sorted by strain.
    pub infected: Vec<(u32, u32)>,
    pub recovered: u32,
}

impl DemeHealth {
    pub fn infected_total(&self) -> u32 {
        self.infected.iter().map(|x| x.1).sum()
    }

    /// Mean relative chance of infection (Hardy–Weinberg, loci independent).
    pub fn susceptibility(&self, escape: u8) -> f32 {
        (0..RES_LOCI)
            .filter(|i| escape >> i & 1 == 0)
            .map(|i| 1.0 - PROTECTION * self.res[i])
            .product()
    }

    fn add(&mut self, strain: u32, k: u32) {
        if k == 0 {
            return;
        }
        match self.infected.binary_search_by_key(&strain, |x| x.0) {
            Ok(i) => self.infected[i].1 += k,
            Err(i) => self.infected.insert(i, (strain, k)),
        }
    }

    /// Takes in another deme's hosts; `w` is this deme's share of the combined size.
    pub(crate) fn merge(&mut self, other: &DemeHealth, w: f32) {
        for i in 0..RES_LOCI {
            self.res[i] = self.res[i] * w + other.res[i] * (1.0 - w);
        }
        for &(s, k) in &other.infected {
            self.add(s, k);
        }
        self.recovered += other.recovered;
    }

    /// Each infected and recovered host survives with probability `keep`.
    pub(crate) fn thin(&mut self, keep: f64, rng: &mut Rng) {
        if keep >= 1.0 {
            return;
        }
        let keep = keep.max(0.0);
        for x in self.infected.iter_mut() {
            x.1 = rng.binomial(x.1 as u64, keep) as u32;
        }
        self.infected.retain(|x| x.1 > 0);
        self.recovered = rng.binomial(self.recovered as u64, keep) as u32;
    }

    /// Keeps infected and recovered hosts within the deme's size `n`.
    pub(crate) fn fit(&mut self, n: u32) {
        let i = self.infected_total();
        if i + self.recovered <= n {
            return;
        }
        if i <= n {
            self.recovered = n - i;
            return;
        }
        self.recovered = 0;
        let f = n as f64 / i as f64;
        for x in self.infected.iter_mut() {
            x.1 = (x.1 as f64 * f).floor() as u32;
        }
        self.infected.retain(|x| x.1 > 0);
    }

    /// Removes `taken` migrants from a deme of `n` (before they left) and returns them.
    pub(crate) fn split(&mut self, n: u32, taken: u32, rng: &mut Rng) -> DemeHealth {
        let mut out = DemeHealth {
            res: self.res,
            ..Default::default()
        };
        let (mut left, mut pool) = (taken as u64, n.max(taken) as u64);
        for x in self.infected.iter_mut() {
            if left == 0 || pool == 0 {
                break;
            }
            let k = rng.binomial(left, x.1 as f64 / pool as f64).min(x.1 as u64) as u32;
            pool -= x.1 as u64;
            left -= k as u64;
            x.1 -= k;
            out.add(x.0, k);
        }
        if left > 0 && pool > 0 {
            let k = rng
                .binomial(left, self.recovered as f64 / pool as f64)
                .min(self.recovered as u64) as u32;
            self.recovered -= k;
            out.recovered = k;
        }
        self.infected.retain(|x| x.1 > 0);
        out
    }
}

/// A pathogen strain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strain {
    pub id: u32,
    pub parent: u32,
    /// Id of the strain introduced, shared by all its descendants.
    pub lineage: u32,
    /// Species it was introduced into.
    pub host: u32,
    pub virulence: f32,
    pub transmissibility: f32,
    /// Resistance loci it escapes (bit i = locus i).
    pub escape: u8,
    pub trade_off: bool,
    pub origin_year: f64,
    /// Hosts infected now.
    pub infected: u32,
    pub cases: u64,
    pub deaths: u64,
}

impl Strain {
    pub fn death_rate(&self) -> f32 {
        MAX_DEATH * self.virulence
    }

    /// Transmission per contact-day with a fully susceptible host.
    pub fn beta(&self) -> f32 {
        let v = self.virulence;
        let t = if self.trade_off {
            v * (1.0 + TRADE_OFF_K) / (v + TRADE_OFF_K)
        } else {
            1.0
        };
        self.transmissibility * t * ESCAPE_COST.powi(self.escape.count_ones() as i32)
    }

    /// Share of infections that end in death.
    pub fn fatality(&self) -> f32 {
        self.death_rate() / (self.death_rate() + RECOVERY)
    }

    pub fn name(&self) -> String {
        if self.id == self.lineage {
            format!("P{}", self.lineage)
        } else {
            format!("P{}.{}", self.lineage, self.id)
        }
    }
}

/// Basic reproduction number of a strain in hosts with `near` neighbours of their
/// species, a given sociality and mean susceptibility.
pub fn r0(strain: &Strain, sociality: f32, near: f32, susceptibility: f32) -> f32 {
    let near = near.max(0.0);
    strain.beta() * (0.5 + sociality) * near / (near + HALF_CONTACTS) * susceptibility
        / (strain.death_rate() + RECOVERY)
}

/// Starting resistance allele frequencies of a species (standing variation).
pub fn standing_res(seed: u64, species: u32) -> [f32; RES_LOCI] {
    std::array::from_fn(|i| {
        0.03 + 0.12 * hash_unit(hash4(seed, species as u64, i as u64, 0x0D15_EA5E)) as f32
    })
}

fn draw_genotype(res: &[f32; RES_LOCI], rng: &mut Rng) -> [u8; 2] {
    let mut out = [0u8; 2];
    for h in out.iter_mut() {
        for (i, &p) in res.iter().enumerate() {
            if rng.f32() < p {
                *h |= 1 << i;
            }
        }
    }
    out
}

/// One gamete: a random copy at each (unlinked) locus, with mutation.
fn gamete(res: [u8; 2], rng: &mut Rng) -> u8 {
    let mut g = 0u8;
    for i in 0..RES_LOCI {
        let mut bit = (res[usize::from(rng.chance(0.5))] >> i) & 1;
        if rng.chance(RES_MUTATION) {
            bit ^= 1;
        }
        g |= bit << i;
    }
    g
}

/// One point of a species' disease chart.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiseaseSample {
    pub year: f64,
    pub n: u32,
    pub infected: u32,
    pub recovered: u32,
    /// Most hosts infected at once since the previous point.
    pub peak: u32,
    /// Resistant allele frequency per locus.
    pub res: [f32; RES_LOCI],
    /// Mean virulence and transmissibility of the strains infecting the species (−1 when none).
    pub virulence: f32,
    pub transmissibility: f32,
    /// Deaths from infection so far.
    pub deaths: u64,
}

/// A species' hosts right now.
#[derive(Clone, Debug, Default)]
pub struct HostStats {
    pub n: u64,
    pub infected: u64,
    pub recovered: u64,
    /// Sum over hosts of resistant dosage per locus.
    res: [f64; RES_LOCI],
    vir: f64,
    trans: f64,
    pub by_strain: BTreeMap<u32, u64>,
}

impl HostStats {
    pub fn res(&self) -> [f32; RES_LOCI] {
        std::array::from_fn(|i| (self.res[i] / self.n.max(1) as f64) as f32)
    }

    pub fn virulence(&self) -> f32 {
        if self.infected == 0 {
            -1.0
        } else {
            (self.vir / self.infected as f64) as f32
        }
    }

    pub fn transmissibility(&self) -> f32 {
        if self.infected == 0 {
            -1.0
        } else {
            (self.trans / self.infected as f64) as f32
        }
    }
}

/// The world's pathogens and its hosts' running state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Disease {
    /// Whether a pathogen has ever been introduced.
    pub active: bool,
    /// Strains with infections, and every introduced strain; sorted by id.
    pub strains: Vec<Strain>,
    pub next_strain: u32,
    pub lineages: u32,
    /// The haplotype each pregnant female received from the father, by mother.
    pub paternal: BTreeMap<u64, u8>,
    pub cases: u64,
    pub deaths: u64,
    pub species_deaths: BTreeMap<u32, u64>,
    /// Species with an epidemic reported (prevalence passed 10%, not yet back below 1%).
    pub surging: BTreeSet<u32>,
    /// (species, locus) whose resistant allele was reported as common.
    pub common: BTreeSet<(u32, u8)>,
    /// Lineages with infections.
    pub circulating: BTreeSet<u32>,
    /// Most hosts of each species infected at once since the last chart point.
    pub peak: BTreeMap<u32, u64>,
    pub series: BTreeMap<u32, Vec<DiseaseSample>>,
}

impl Disease {
    pub fn strain(&self, id: u32) -> Option<&Strain> {
        self.strains
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.strains[i])
    }

    fn strain_mut(&mut self, id: u32) -> Option<&mut Strain> {
        self.strains
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &mut self.strains[i])
    }

    /// A mutant of strain `parent`; returns its id (or `parent` when no new strain is made).
    fn mutate(&mut self, parent: u32, year: f64, rng: &mut Rng) -> u32 {
        if self.strains.len() >= MAX_STRAINS {
            return parent;
        }
        let Some(p) = self.strain(parent).cloned() else {
            return parent;
        };
        let escape = if rng.chance(0.25) {
            p.escape ^ (1 << rng.below(RES_LOCI))
        } else {
            p.escape
        };
        let id = self.next_strain;
        self.next_strain += 1;
        self.strains.push(Strain {
            id,
            parent,
            lineage: p.lineage,
            host: p.host,
            virulence: (p.virulence + rng.normal_f32(0.0, STRAIN_MUT_SD)).clamp(0.01, 1.0),
            transmissibility: (p.transmissibility + rng.normal_f32(0.0, STRAIN_MUT_SD))
                .clamp(0.01, 1.0),
            escape,
            trade_off: p.trade_off,
            origin_year: year,
            infected: 0,
            cases: 0,
            deaths: 0,
        });
        id
    }

    fn infection(&mut self, strain: u32, k: u64) {
        self.cases += k;
        if let Some(s) = self.strain_mut(strain) {
            s.cases += k;
        }
    }

    fn died(&mut self, strain: u32, species: u32, k: u64) {
        if k == 0 {
            return;
        }
        self.deaths += k;
        *self.species_deaths.entry(species).or_default() += k;
        if let Some(s) = self.strain_mut(strain) {
            s.deaths += k;
        }
    }

    fn push_sample(&mut self, species: u32, s: DiseaseSample) {
        let series = self.series.entry(species).or_default();
        series.push(s);
        if series.len() > SERIES_CAP {
            let kept: Vec<DiseaseSample> = series
                .drain(..)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, s)| s)
                .collect();
            *series = kept;
        }
    }

    /// Deaths from infection per host per year in a species between two years (from the
    /// chart series); 0 when unknown.
    pub fn hazard(&self, species: u32, from: f64, to: f64) -> f32 {
        let Some(s) = self.series.get(&species) else {
            return 0.0;
        };
        let pts: Vec<&DiseaseSample> = s
            .iter()
            .filter(|x| x.year > from + 1e-9 && x.year <= to + 1e-9)
            .collect();
        if to <= from || pts.is_empty() {
            return 0.0;
        }
        let n = pts.iter().map(|x| x.n as f64).sum::<f64>() / pts.len() as f64;
        if n <= 0.0 {
            return 0.0;
        }
        (self.deaths_between(species, from, to) as f64 / n / (to - from)) as f32
    }

    /// Deaths from infection in a species between two years (from the chart series).
    pub fn deaths_between(&self, species: u32, from: f64, to: f64) -> u64 {
        let Some(s) = self.series.get(&species) else {
            return 0;
        };
        let at = |y: f64| {
            s.iter()
                .take_while(|x| x.year <= y + 1e-9)
                .last()
                .map_or(0, |x| x.deaths)
        };
        at(to).saturating_sub(at(from))
    }
}

/// How the disease state is laid out in a save file (after the climate section).
#[derive(Serialize)]
pub(crate) struct SavedRef<'a> {
    pub disease: &'a Disease,
    pub animals: Vec<Health>,
    pub demes: Vec<Option<DemeHealth>>,
    pub frozen: Vec<Option<DemeHealth>>,
}

#[derive(Deserialize)]
pub(crate) struct Saved {
    pub disease: Disease,
    pub animals: Vec<Health>,
    pub demes: Vec<Option<DemeHealth>>,
    pub frozen: Vec<Option<DemeHealth>>,
}

/// Removes `k` random individuals from a deme's age classes.
fn remove_random(d: &mut Deme, k: u32, rng: &mut Rng) {
    let mut left = k as u64;
    let mut pool = d.total() as u64;
    for c in d.females.iter_mut().chain(d.males.iter_mut()) {
        if left == 0 || pool == 0 {
            break;
        }
        let x = rng.binomial(left, *c as f64 / pool as f64).min(*c as u64);
        pool -= *c as u64;
        *c -= x as u32;
        left -= x;
    }
}

impl Simulation {
    /// Saved form of the disease state.
    pub(crate) fn disease_saved(&self) -> SavedRef<'_> {
        let on = self.disease.active;
        SavedRef {
            disease: &self.disease,
            animals: if on {
                self.animals.iter().map(|a| a.health).collect()
            } else {
                vec![]
            },
            demes: if on {
                self.demes.iter().map(|d| d.health.clone()).collect()
            } else {
                vec![]
            },
            frozen: if on {
                self.frozen.iter().map(|d| d.health.clone()).collect()
            } else {
                vec![]
            },
        }
    }

    pub(crate) fn restore_disease(&mut self, s: Saved) -> Result<(), String> {
        if s.disease.active {
            if s.animals.len() != self.animals.len()
                || s.demes.len() != self.demes.len()
                || s.frozen.len() != self.frozen.len()
            {
                return Err("disease records do not match the organisms".into());
            }
            for (a, h) in self.animals.iter_mut().zip(s.animals) {
                a.health = h;
            }
            for (d, h) in self.demes.iter_mut().zip(s.demes) {
                d.health = h;
            }
            for (d, h) in self.frozen.iter_mut().zip(s.frozen) {
                d.health = h;
            }
        }
        self.disease = s.disease;
        Ok(())
    }

    /// Bytes added to `state_hash()` once a pathogen has been introduced.
    pub(crate) fn disease_hash_bytes(&self) -> Vec<u8> {
        if !self.disease.active {
            return vec![];
        }
        let d = Disease {
            series: BTreeMap::new(),
            ..self.disease.clone()
        };
        let mut s = self.disease_saved();
        s.disease = &d;
        bincode::serialize(&s).unwrap_or_default()
    }

    /// A founder's or materialized animal's resistance genotype.
    fn fresh_health(&self, res: &[f32; RES_LOCI], entity: u64) -> Health {
        let mut rng = Rng::keyed(self.seed, self.minutes, entity, stream::DISEASE);
        Health {
            res: draw_genotype(res, &mut rng),
            sir: Sir::Susceptible,
        }
    }

    /// Gives every host a resistance genotype from its species' standing variation.
    fn activate_disease(&mut self) {
        if self.disease.active {
            return;
        }
        self.disease.active = true;
        self.disease.next_strain = self.disease.next_strain.max(1);
        for i in 0..self.animals.len() {
            let a = &self.animals[i];
            let h = self.fresh_health(&standing_res(self.seed, a.species), a.id);
            self.animals[i].health = h;
        }
        let seed = self.seed;
        for d in self.demes.iter_mut().chain(self.frozen.iter_mut()) {
            d.health = Some(DemeHealth {
                res: standing_res(seed, d.species),
                ..Default::default()
            });
        }
    }

    /// Sets up the resistance genotypes of newly made founders.
    pub(crate) fn founder_health(&self, animals: &mut [Animal]) {
        if !self.disease.active {
            return;
        }
        for a in animals.iter_mut() {
            a.health = self.fresh_health(&standing_res(self.seed, a.species), a.id);
        }
    }

    /// Records the father's haplotype for a female's pregnancy.
    pub(crate) fn note_conception(&mut self, mother: usize, father: usize, rng_key: u64) {
        if !self.disease.active {
            return;
        }
        let mut rng = Rng::keyed(
            self.seed,
            rng_key,
            self.animals[mother].id,
            stream::DISEASE ^ 0x11,
        );
        let g = gamete(self.animals[father].health.res, &mut rng);
        self.disease.paternal.insert(self.animals[mother].id, g);
    }

    /// A newborn's resistance genotype, from its mother and the recorded father.
    pub(crate) fn newborn_health(
        &mut self,
        mother: Health,
        mother_id: u64,
        child_id: u64,
    ) -> Health {
        if !self.disease.active {
            return Health::default();
        }
        let mut rng = Rng::keyed(self.seed, self.minutes, child_id, stream::DISEASE ^ 0x22);
        let from_father = self
            .disease
            .paternal
            .get(&mother_id)
            .copied()
            .unwrap_or_else(|| gamete(mother.res, &mut rng));
        Health {
            res: [gamete(mother.res, &mut rng), from_father],
            sir: Sir::Susceptible,
        }
    }

    /// Forgets a mother's record once her litter is born.
    pub(crate) fn litter_born(&mut self, mother_id: u64) {
        if self.disease.active {
            self.disease.paternal.remove(&mother_id);
        }
    }

    /// Mode I → P: the hosts of a new deme.
    pub(crate) fn deme_health_from(&self, members: &[Animal]) -> Option<DemeHealth> {
        if !self.disease.active {
            return None;
        }
        let mut h = DemeHealth::default();
        let n = members.len().max(1) as f32;
        for a in members {
            for i in 0..RES_LOCI {
                h.res[i] += a.health.dosage(i) / n;
            }
            match a.health.sir {
                Sir::Infected(s) => h.add(s, 1),
                Sir::Recovered => h.recovered += 1,
                Sir::Susceptible => {}
            }
        }
        Some(h)
    }

    /// Mode P → I: gives the animals made from a deme (`first..` in `animals`) exactly
    /// its infected and recovered counts, and genotypes drawn from its frequencies.
    pub(crate) fn materialize_health(&mut self, d: &Deme, first: usize, key: u64) {
        let Some(h) = d.health.as_ref() else {
            return;
        };
        let n = self.animals.len() - first;
        let mut rng = Rng::keyed(self.seed, self.minutes, key, stream::DISEASE ^ 0x33);
        let mut order: Vec<usize> = (first..self.animals.len()).collect();
        // Fisher–Yates shuffle.
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i + 1));
        }
        let mut states = Vec::with_capacity(n);
        for &(s, k) in &h.infected {
            states.extend(std::iter::repeat_n(Sir::Infected(s), k as usize));
        }
        states.extend(std::iter::repeat_n(Sir::Recovered, h.recovered as usize));
        for (x, &i) in order.iter().enumerate() {
            let res = draw_genotype(&h.res, &mut rng);
            self.animals[i].health = Health {
                res,
                sir: states.get(x).copied().unwrap_or_default(),
            };
        }
    }

    /// Releases a pathogen into a species (the Epidemic intervention).
    pub(crate) fn outbreak(&mut self, species: u32, spec: &PathogenSpec) -> Result<(), String> {
        spec.validate()?;
        let present = match self.mode {
            Mode::Individual => {
                self.animals.iter().any(|a| a.species == species)
                    || self.frozen.iter().any(|d| d.species == species)
            }
            _ => self.demes.iter().any(|d| d.species == species),
        };
        if !present {
            return Err("that species has no living members".into());
        }
        self.activate_disease();
        let year = self.year();
        self.disease.lineages += 1;
        let id = self.disease.next_strain;
        self.disease.next_strain += 1;
        self.disease.strains.push(Strain {
            id,
            parent: NO_STRAIN,
            lineage: id,
            host: species,
            virulence: spec.virulence,
            transmissibility: spec.transmissibility,
            escape: 0,
            trade_off: spec.trade_off,
            origin_year: year,
            infected: 0,
            cases: 0,
            deaths: 0,
        });
        // It starts where the species is most crowded: patches are seeded in order of
        // their hosts until the share of the species is infected.
        let mut hosts: BTreeMap<u32, u64> = BTreeMap::new();
        for a in self
            .animals
            .iter()
            .filter(|a| a.alive && a.species == species)
        {
            *hosts.entry(self.patch_of_tile_xy(a.x, a.y)).or_default() += 1;
        }
        for d in self.demes.iter().chain(&self.frozen) {
            if d.species == species {
                *hosts.entry(d.patch).or_default() += d.total() as u64;
            }
        }
        let mut order: Vec<(u32, u64)> = hosts.into_iter().filter(|x| x.1 > 0).collect();
        order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let patch = order
            .first()
            .map(|x| x.0)
            .ok_or("that species has no living members")?;
        let total: u64 = order.iter().map(|x| x.1).sum();
        let mut left = ((spec.share as f64 * total as f64).round() as u64).max(1);
        let mut infected = 0u64;
        let mut rng = Rng::keyed(self.seed, self.minutes, id as u64, stream::DISEASE ^ 0x44);
        for &(p, _) in &order {
            if left == 0 {
                break;
            }
            // Susceptible animals in the patch, in random order.
            let mut free: Vec<usize> = (0..self.animals.len())
                .filter(|&i| {
                    let a = &self.animals[i];
                    a.alive
                        && a.species == species
                        && a.health.sir == Sir::Susceptible
                        && self.patch_of_tile_xy(a.x, a.y) == p
                })
                .collect();
            for i in (1..free.len()).rev() {
                free.swap(i, rng.below(i + 1));
            }
            for &i in free.iter().take(left as usize) {
                self.animals[i].health.sir = Sir::Infected(id);
                infected += 1;
                left -= 1;
            }
            for d in self.demes.iter_mut().chain(self.frozen.iter_mut()) {
                if d.species != species || d.patch != p || left == 0 {
                    continue;
                }
                let n = d.total();
                let h = d.health.get_or_insert_with(Default::default);
                let k = (n.saturating_sub(h.infected_total() + h.recovered) as u64).min(left);
                h.add(id, k as u32);
                infected += k;
                left -= k;
            }
        }
        let ps = self.params.patch_size;
        let pw = self.patches_w();
        let centre = ((patch as usize / pw) * ps + ps / 2).min(self.world.h - 1) * self.world.w
            + ((patch as usize % pw) * ps + ps / 2).min(self.world.w - 1);
        let place = crate::nature::place(&self.world, centre);
        self.disease.infection(id, infected);
        self.disease.circulating.insert(id);
        let strain = self.disease.strain(id).cloned().expect("strain just added");
        let r0 = self.r0_estimate(species, &strain);
        let name = self.species.get(species).common_name.clone();
        self.event(
            EventKind::Disease,
            Some(species),
            format!(
                "Outbreak of {} in {name} around {place}: {infected} infected (virulence {:.2}, transmissibility {:.2}{}, R₀ ≈ {:.1}).",
                strain.name(),
                strain.virulence,
                strain.transmissibility,
                if strain.trade_off { ", with the trade-off" } else { "" },
                r0
            ),
        );
        self.disease_census();
        Ok(())
    }

    /// Mean basic reproduction number of a strain over a species' hosts.
    pub fn r0_estimate(&self, species: u32, strain: &Strain) -> f32 {
        strain.beta() / (strain.death_rate() + RECOVERY) * self.r0_contacts(species, strain.escape)
    }

    /// The host side of R₀: the mean over a species' hosts of contacts per day
    /// ((0.5 + sociality) · n / (n + 10), with `n` neighbours of its species) times
    /// susceptibility to strains escaping `escape`.
    pub fn r0_contacts(&self, species: u32, escape: u8) -> f32 {
        let (mut sum, mut n) = (0f64, 0f64);
        let mut per_tile: HashMap<usize, u32> = HashMap::new();
        for a in self
            .animals
            .iter()
            .filter(|a| a.alive && a.species == species)
        {
            *per_tile.entry(self.world.tile_of(a.x, a.y)).or_default() += 1;
        }
        let contacts = |soc: f32, near: f32| {
            let near = near.max(0.0);
            (0.5 + soc) * near / (near + HALF_CONTACTS)
        };
        for a in self
            .animals
            .iter()
            .filter(|a| a.alive && a.species == species)
        {
            let near: u32 = tiles_around(&self.world, a.x, a.y, REACH)
                .iter()
                .map(|t| per_tile.get(t).copied().unwrap_or(0))
                .sum();
            sum += (contacts(a.pheno.get(Trait::Sociality), near.saturating_sub(1) as f32)
                * a.health.susceptibility(escape)) as f64;
            n += 1.0;
        }
        for d in self.counted_demes().iter().filter(|d| d.species == species) {
            let total = d.total();
            if total == 0 {
                continue;
            }
            let tiles = self.patch_land_tiles(d.patch).max(1);
            let soc =
                Phenotype::from_latent(&d.mean_latent(&self.archs[d.arch as usize], &self.alleles))
                    .get(Trait::Sociality);
            let susc = d.health.as_ref().map_or(1.0, |h| h.susceptibility(escape));
            let near = total as f32 / tiles as f32 * CONTACT_TILES - 1.0;
            sum += (contacts(soc, near) * susc) as f64 * total as f64;
            n += total as f64;
        }
        if n > 0.0 {
            (sum / n) as f32
        } else {
            0.0
        }
    }

    /// One day of disease in Individual mode: transmission among neighbours, recovery,
    /// death, loss of immunity and the cost of resistance. Uses this tick's grid.
    pub(crate) fn disease_day(&mut self, day: u64) {
        if !self.disease.active {
            return;
        }
        let year = self.year();
        let n = self.animals.len();
        let mut marked = vec![false; n];
        let mut infections: Vec<(usize, u32)> = vec![];
        let mut outcomes: Vec<(usize, Option<Cause>, u32)> = vec![];
        let mut wane: Vec<usize> = vec![];
        for i in 0..n {
            let a = &self.animals[i];
            if !a.alive {
                continue;
            }
            let mut rng = Rng::keyed(self.seed, day, a.id, stream::DISEASE);
            match a.health.sir {
                Sir::Infected(s) => {
                    let Some(st) = self.disease.strain(s) else {
                        outcomes.push((i, None, s));
                        continue;
                    };
                    let near = tiles_around(&self.world, a.x, a.y, REACH);
                    let mut count = 0u32;
                    for &t in near.iter() {
                        for &j in self.grid.cell(t) {
                            let b = &self.animals[j as usize];
                            if j as usize != i && b.alive && b.species == a.species {
                                count += 1;
                            }
                        }
                    }
                    if count > 0 {
                        let base = st.beta() * (0.5 + a.pheno.get(Trait::Sociality))
                            / (count as f32 + HALF_CONTACTS);
                        for &t in near.iter() {
                            for &j in self.grid.cell(t) {
                                let j = j as usize;
                                let b = &self.animals[j];
                                if j == i
                                    || !b.alive
                                    || b.species != a.species
                                    || b.health.sir != Sir::Susceptible
                                    || marked[j]
                                {
                                    continue;
                                }
                                let rate = base * b.health.susceptibility(st.escape);
                                if rng.chance(1.0 - (-rate as f64).exp()) {
                                    marked[j] = true;
                                    infections.push((j, s));
                                }
                            }
                        }
                    }
                    let alpha = st.death_rate();
                    let out = alpha + RECOVERY;
                    if rng.chance(1.0 - (-out as f64).exp()) {
                        let dies = rng.f32() * out < alpha;
                        outcomes.push((i, dies.then_some(Cause::Disease), s));
                    }
                }
                Sir::Recovered => {
                    if rng.chance(1.0 - (-WANING as f64).exp()) {
                        wane.push(i);
                    }
                }
                Sir::Susceptible => {}
            }
            let c = self.animals[i].health.copies();
            if c > 0 && rng.chance((RES_COST * c as f32 / 365.0) as f64) {
                outcomes.push((i, Some(Cause::Accident), NO_STRAIN));
            }
        }
        for i in wane {
            self.animals[i].health.sir = Sir::Susceptible;
        }
        for (i, cause, s) in outcomes {
            match cause {
                Some(Cause::Disease) => {
                    let sp = self.animals[i].species;
                    self.disease.died(s, sp, 1);
                    self.kill(i, Cause::Disease);
                }
                Some(c) => self.kill(i, c),
                None => self.animals[i].health.sir = Sir::Recovered,
            }
        }
        let pregnant: BTreeSet<u64> = self
            .animals
            .iter()
            .filter(|a| a.alive && a.pregnancy.is_some())
            .map(|a| a.id)
            .collect();
        self.disease.paternal.retain(|id, _| pregnant.contains(id));
        for (j, s) in infections {
            if !self.animals[j].alive {
                continue;
            }
            let mut rng = Rng::keyed(self.seed, day, self.animals[j].id, stream::DISEASE ^ 0x55);
            let s = if rng.chance(STRAIN_MUTATION) {
                self.disease.mutate(s, year, &mut rng)
            } else {
                s
            };
            self.animals[j].health.sir = Sir::Infected(s);
            self.disease.infection(s, 1);
        }
        self.disease_census();
    }

    /// Thins a deme's infected and recovered hosts after a Population-mode year of
    /// demography (`n_before` hosts, `births` of them newborn now).
    pub(crate) fn thin_after_demography(
        seed: u64,
        tick: u64,
        key: u64,
        deme: &mut Deme,
        n_before: u32,
        births: u32,
    ) {
        let total = deme.total();
        let Some(h) = deme.health.as_mut() else {
            return;
        };
        if n_before == 0 {
            *h = DemeHealth {
                res: h.res,
                ..Default::default()
            };
            return;
        }
        let survivors = total.saturating_sub(births);
        let mut rng = Rng::keyed(seed, tick, key, stream::DISEASE ^ 0x66);
        h.thin(survivors as f64 / n_before as f64, &mut rng);
        h.fit(total);
    }

    /// One Population-mode year of disease, in weekly steps taken by all demes together
    /// (so epidemics travel between neighbouring patches), then selection, drift and
    /// mutation at the resistance loci.
    pub(crate) fn disease_year(&mut self) {
        if !self.disease.active {
            return;
        }
        let ctxs = self.patch_contexts();
        let (pw, ph) = (self.patches_w() as i64, self.patches_h() as i64);
        let year = self.year();
        let tick = self.minutes;
        let seed = self.seed;
        let dt = 365.0 / STEPS as f64;
        self.disease.paternal.clear();
        let mut demes = std::mem::take(&mut self.demes);
        let nd = demes.len();
        for d in demes.iter_mut() {
            if d.health.is_none() {
                d.health = Some(DemeHealth {
                    res: standing_res(seed, d.species),
                    ..Default::default()
                });
            }
        }
        let mut rngs: Vec<Rng> = demes
            .iter()
            .map(|d| {
                Rng::keyed(
                    seed,
                    tick,
                    ((d.species as u64) << 32) | d.patch as u64,
                    stream::DISEASE,
                )
            })
            .collect();
        // Floods, eruptions and other losses since the last year may have shrunk demes.
        for d in demes.iter_mut() {
            let n = d.total();
            if let Some(h) = d.health.as_mut() {
                h.fit(n);
            }
        }
        let n0: Vec<u32> = demes.iter().map(|d| d.total()).collect();
        // This year's newborns join the hosts gradually, as births spread over the year.
        let born: Vec<u32> = demes.iter().map(|d| d.females[0] + d.males[0]).collect();
        let mut dead: Vec<BTreeMap<u32, u64>> = vec![BTreeMap::new(); nd];
        let any = demes
            .iter()
            .any(|d| d.health.as_ref().is_some_and(|h| !h.infected.is_empty()));
        if any {
            let where_is: HashMap<(u32, u32), usize> = demes
                .iter()
                .enumerate()
                .map(|(i, d)| ((d.species, d.patch), i))
                .collect();
            // Contacts per infected per day at full susceptibility, before crowding.
            let soc: Vec<f32> = demes
                .iter()
                .map(|d| {
                    0.5 + Phenotype::from_latent(
                        &d.mean_latent(&self.archs[d.arch as usize], &self.alleles),
                    )
                    .get(Trait::Sociality)
                })
                .collect();
            let tiles: Vec<f32> = demes
                .iter()
                .map(|d| ctxs[d.patch as usize].land.len().max(1) as f32)
                .collect();
            // Neighbouring demes of the same species, with the share of contacts made there.
            let nbrs: Vec<Vec<(usize, f32)>> = demes
                .iter()
                .map(|d| {
                    let (targets, weights) = neighbour_patches(&ctxs, d.patch, pw, ph);
                    targets
                        .iter()
                        .zip(weights)
                        .filter_map(|(t, w)| {
                            where_is
                                .get(&(d.species, *t))
                                .map(|&j| (j, NEIGHBOUR_CONTACT * w as f32))
                        })
                        .collect()
                })
                .collect();
            let mut weekly: BTreeMap<u32, u64> = BTreeMap::new();
            for step in 0..STEPS {
                let unborn =
                    |k: usize| (born[k] as u64 * (STEPS - 1 - step) as u64 / STEPS as u64) as u32;
                // Infected share by strain in each deme at the start of the week.
                let prev: Vec<Vec<(u32, f32)>> = demes
                    .iter()
                    .enumerate()
                    .map(|(k, d)| {
                        let n = d.total().saturating_sub(unborn(k)).max(1) as f32;
                        d.health
                            .as_ref()
                            .map(|h| h.infected.iter().map(|&(s, k)| (s, k as f32 / n)).collect())
                            .unwrap_or_default()
                    })
                    .collect();
                if prev.iter().all(|p| p.is_empty()) {
                    break;
                }
                weekly.clear();
                for k in 0..nd {
                    let all = demes[k].total();
                    let n = all.saturating_sub(unborn(k));
                    if n == 0 {
                        continue;
                    }
                    // Force of infection by strain: own hosts and neighbours'.
                    let mut force: BTreeMap<u32, f32> = BTreeMap::new();
                    let away: f32 = nbrs[k].iter().map(|x| x.1).sum();
                    for &(s, p) in &prev[k] {
                        *force.entry(s).or_default() += (1.0 - away) * p;
                    }
                    for &(j, w) in &nbrs[k] {
                        for &(s, p) in &prev[j] {
                            *force.entry(s).or_default() += w * p;
                        }
                    }
                    if force.is_empty() {
                        continue;
                    }
                    let near = (n as f32 / tiles[k] * CONTACT_TILES - 1.0).max(0.0);
                    let contact = soc[k] * near / (near + HALF_CONTACTS);
                    let d = &mut demes[k];
                    let rng = &mut rngs[k];
                    let h = d.health.as_mut().expect("deme has hosts");
                    let lam: Vec<(u32, f64)> = force
                        .iter()
                        .filter_map(|(&s, &p)| {
                            self.disease.strain(s).map(|st| {
                                (
                                    s,
                                    (st.beta() * contact * h.susceptibility(st.escape) * p) as f64,
                                )
                            })
                        })
                        .collect();
                    let total: f64 = lam.iter().map(|x| x.1).sum();
                    let sus = n.saturating_sub(h.infected_total() + h.recovered) as u64;
                    let new = if total > 0.0 {
                        rng.binomial(sus, 1.0 - (-total * dt).exp())
                    } else {
                        0
                    };
                    // Split the new infections among strains.
                    let mut new_by: Vec<(u32, u64)> = vec![];
                    let (mut left, mut rest) = (new, total);
                    for (x, &(s, l)) in lam.iter().enumerate() {
                        if left == 0 || rest <= 0.0 {
                            break;
                        }
                        let c = if x + 1 == lam.len() {
                            left
                        } else {
                            rng.binomial(left, (l / rest).min(1.0))
                        };
                        if c > 0 {
                            new_by.push((s, c));
                        }
                        left -= c;
                        rest -= l;
                    }
                    // Recovery and death of those already infected.
                    let mut recovered = 0u32;
                    let mut deaths = 0u32;
                    for e in h.infected.iter_mut() {
                        let alpha = self.disease.strain(e.0).map_or(0.0, |st| st.death_rate());
                        let out = (alpha + RECOVERY) as f64;
                        let leave = rng.binomial(e.1 as u64, 1.0 - (-out * dt).exp());
                        let died = rng.binomial(leave, alpha as f64 / out);
                        recovered += (leave - died) as u32;
                        deaths += died as u32;
                        if died > 0 {
                            *dead[k].entry(e.0).or_default() += died;
                        }
                        e.1 -= leave as u32;
                    }
                    let waned =
                        rng.binomial(h.recovered as u64, 1.0 - (-(WANING as f64) * dt).exp());
                    h.recovered = h.recovered - waned as u32 + recovered;
                    // New infections, a few of them by new strains.
                    for &(s, c) in &new_by {
                        self.disease.infection(s, c);
                        let mut c = c as u32;
                        if h.infected.len() < MAX_STRAINS_PER_DEME
                            && rng.chance(1.0 - (1.0 - STRAIN_MUTATION).powf(c as f64))
                        {
                            let m = self.disease.mutate(s, year, rng);
                            if m != s {
                                h.add(m, 1);
                                c -= 1;
                            }
                        }
                        h.add(s, c);
                    }
                    h.infected.retain(|x| x.1 > 0);
                    if deaths > 0 {
                        remove_random(d, deaths, rng);
                        let n = d.total();
                        d.health.as_mut().expect("deme has hosts").fit(n);
                    }
                    let i = d.health.as_ref().map_or(0, |h| h.infected_total());
                    *weekly.entry(d.species).or_default() += i as u64;
                }
                for (&sp, &i) in &weekly {
                    let p = self.disease.peak.entry(sp).or_default();
                    *p = (*p).max(i);
                }
            }
        }
        for (k, d) in demes.iter_mut().enumerate() {
            if n0[k] == 0 {
                continue;
            }
            for (&s, &c) in &dead[k] {
                self.disease.died(s, d.species, c);
            }
            let rng = &mut rngs[k];
            let n = d.total();
            let h = d.health.as_mut().expect("deme has hosts");
            // Selection at the resistance loci: deaths fall on susceptible genotypes...
            for i in 0..RES_LOCI {
                let p = h.res[i];
                let killed: u64 = dead[k]
                    .iter()
                    .filter(|(s, _)| {
                        self.disease
                            .strain(**s)
                            .is_none_or(|st| st.escape >> i & 1 == 0)
                    })
                    .map(|(_, c)| *c)
                    .sum();
                let dd = (killed as f32 / n0[k] as f32).min(1.0);
                let sbar = 1.0 - PROTECTION * p;
                let s_r = 1.0 - PROTECTION * (1.0 + p) / 2.0;
                let s_s = 1.0 - PROTECTION * p / 2.0;
                let mut w_r = 1.0 - dd * s_r / sbar;
                let mut w_s = 1.0 - dd * s_s / sbar;
                // ...and resistance has a cost.
                w_r *= 1.0 - RES_COST * (1.0 + p);
                w_s *= 1.0 - RES_COST * p;
                let wbar = p * w_r + (1.0 - p) * w_s;
                let mut p = if wbar > 0.0 { p * w_r / wbar } else { p };
                // Drift and mutation.
                let copies = 2 * n.clamp(1, 50_000) as u64;
                p = rng.binomial(copies, p.clamp(0.0, 1.0) as f64) as f32 / copies as f32;
                p += RES_MUTATION as f32 * (1.0 - 2.0 * p);
                h.res[i] = p.clamp(0.0, 1.0);
            }
            // The cost of resistance kills a few.
            let copies: f32 = 2.0 * h.res.iter().sum::<f32>();
            let cost = rng.binomial(n as u64, (RES_COST * copies) as f64) as u32;
            if cost > 0 {
                remove_random(d, cost, rng);
                let n = d.total();
                d.health.as_mut().expect("deme has hosts").fit(n);
            }
        }
        demes.retain(|d| d.total() > 0);
        self.demes = demes;
        self.disease_census();
    }

    /// After a Macro-mode step: every deme has hosts' records, within its size.
    pub(crate) fn disease_after_macro(&mut self) {
        if !self.disease.active {
            return;
        }
        let seed = self.seed;
        for d in self.demes.iter_mut() {
            let n = d.total();
            let h = d.health.get_or_insert_with(|| DemeHealth {
                res: standing_res(seed, d.species),
                ..Default::default()
            });
            h.fit(n);
        }
        self.disease_census();
    }

    /// Land tiles in a patch.
    pub(crate) fn patch_land_tiles(&self, patch: u32) -> usize {
        let ps = self.params.patch_size;
        let pw = self.patches_w();
        let (px, py) = (patch as usize % pw, patch as usize / pw);
        (py * ps..((py + 1) * ps).min(self.world.h))
            .flat_map(|y| (px * ps..((px + 1) * ps).min(self.world.w)).map(move |x| (x, y)))
            .filter(|&(x, y)| self.world.is_land(y * self.world.w + x))
            .count()
    }

    /// Hosts per species right now (animals, demes and held demes).
    pub fn host_stats(&self) -> BTreeMap<u32, HostStats> {
        let mut out: BTreeMap<u32, HostStats> = BTreeMap::new();
        let vir = |s: u32| {
            self.disease.strain(s).map_or((0.0, 0.0), |st| {
                (st.virulence as f64, st.transmissibility as f64)
            })
        };
        for a in &self.animals {
            if !a.alive {
                continue;
            }
            let e = out.entry(a.species).or_default();
            e.n += 1;
            for i in 0..RES_LOCI {
                e.res[i] += a.health.dosage(i) as f64;
            }
            match a.health.sir {
                Sir::Infected(s) => {
                    e.infected += 1;
                    let (v, t) = vir(s);
                    e.vir += v;
                    e.trans += t;
                    *e.by_strain.entry(s).or_default() += 1;
                }
                Sir::Recovered => e.recovered += 1,
                Sir::Susceptible => {}
            }
        }
        for d in self.demes.iter().chain(&self.frozen) {
            let n = d.total() as u64;
            let e = out.entry(d.species).or_default();
            e.n += n;
            let Some(h) = d.health.as_ref() else {
                continue;
            };
            for i in 0..RES_LOCI {
                e.res[i] += h.res[i] as f64 * n as f64;
            }
            for &(s, k) in &h.infected {
                e.infected += k as u64;
                let (v, t) = vir(s);
                e.vir += v * k as f64;
                e.trans += t * k as f64;
                *e.by_strain.entry(s).or_default() += k as u64;
            }
            e.recovered += h.recovered as u64;
        }
        out
    }

    /// Counts each strain's infections, drops strains that have died out (keeping the
    /// introduced ones) and reports epidemics, lineages dying out and resistance.
    fn disease_census(&mut self) {
        let stats = self.host_stats();
        let mut per: HashMap<u32, u64> = HashMap::new();
        for st in stats.values() {
            for (&s, &k) in &st.by_strain {
                *per.entry(s).or_default() += k;
            }
        }
        for s in self.disease.strains.iter_mut() {
            s.infected = per.get(&s.id).copied().unwrap_or(0) as u32;
        }
        self.disease
            .strains
            .retain(|s| s.infected > 0 || s.id == s.lineage);
        let mut by_lineage: BTreeMap<u32, u64> = BTreeMap::new();
        for s in &self.disease.strains {
            *by_lineage.entry(s.lineage).or_default() += s.infected as u64;
        }
        let mut notes: Vec<(Option<u32>, String)> = vec![];
        for l in self.disease.circulating.clone() {
            if by_lineage.get(&l).copied().unwrap_or(0) == 0 {
                self.disease.circulating.remove(&l);
                let (name, host) = self
                    .disease
                    .strain(l)
                    .map(|s| (s.name(), s.host))
                    .unwrap_or_else(|| (format!("P{l}"), u32::MAX));
                let host = (host != u32::MAX).then_some(host);
                notes.push((host, format!("Pathogen {name} died out.")));
            }
        }
        for (&sp, st) in &stats {
            let peak = self.disease.peak.entry(sp).or_default();
            *peak = (*peak).max(st.infected);
            let peak = *peak;
            if st.n == 0 {
                continue;
            }
            let prev = peak.min(st.n) as f64 / st.n as f64;
            let name = self.species.get(sp).common_name.clone();
            if prev >= 0.1 && st.n >= 20 && !self.disease.surging.contains(&sp) {
                self.disease.surging.insert(sp);
                let top = st
                    .by_strain
                    .iter()
                    .max_by_key(|(_, k)| **k)
                    .and_then(|(s, _)| self.disease.strain(*s))
                    .map(|s| s.name())
                    .unwrap_or_else(|| "a pathogen".into());
                notes.push((
                    Some(sp),
                    format!("Epidemic: {top} infects {:.0}% of {name}.", 100.0 * prev),
                ));
            } else if prev < 0.01 {
                self.disease.surging.remove(&sp);
            }
            let res = st.res();
            for (i, &p) in res.iter().enumerate() {
                let key = (sp, i as u8);
                if p >= 0.5 && !self.disease.common.contains(&key) {
                    self.disease.common.insert(key);
                    notes.push((
                        Some(sp),
                        format!(
                            "Resistance allele R{} is now carried by most {name} genomes ({:.0}%).",
                            i + 1,
                            100.0 * p
                        ),
                    ));
                } else if p < 0.3 {
                    self.disease.common.remove(&key);
                }
            }
        }
        for (sp, text) in notes {
            self.event(EventKind::Disease, sp, text);
        }
    }

    /// Records a point of each species' disease chart (with the other metrics).
    pub(crate) fn record_disease(&mut self, year: f64) {
        if !self.disease.active {
            return;
        }
        for (sp, st) in self.host_stats() {
            if st.n == 0 {
                continue;
            }
            let deaths = self.disease.species_deaths.get(&sp).copied().unwrap_or(0);
            self.disease.push_sample(
                sp,
                DiseaseSample {
                    year,
                    n: st.n.min(u32::MAX as u64) as u32,
                    infected: st.infected.min(u32::MAX as u64) as u32,
                    recovered: st.recovered.min(u32::MAX as u64) as u32,
                    peak: self
                        .disease
                        .peak
                        .get(&sp)
                        .copied()
                        .unwrap_or(0)
                        .max(st.infected)
                        .min(u32::MAX as u64) as u32,
                    res: st.res(),
                    virulence: st.virulence(),
                    transmissibility: st.transmissibility(),
                    deaths,
                },
            );
        }
        self.disease.peak.clear();
    }

    /// Share of each tile's hosts that are infected (by patch); −1 where nobody lives.
    pub fn disease_layer(&self) -> Vec<f32> {
        let n = self.world.w * self.world.h;
        let mut out = vec![-1f32; n];
        let mut acc: HashMap<u32, (f64, f64)> = HashMap::new();
        for a in self.animals.iter().filter(|a| a.alive) {
            let e = acc.entry(self.patch_of_tile_xy(a.x, a.y)).or_default();
            e.1 += 1.0;
            if matches!(a.health.sir, Sir::Infected(_)) {
                e.0 += 1.0;
            }
        }
        for d in self.counted_demes() {
            let e = acc.entry(d.patch).or_default();
            e.1 += d.total() as f64;
            e.0 += d.health.as_ref().map_or(0, |h| h.infected_total()) as f64;
        }
        for (i, v) in out.iter_mut().enumerate() {
            if !self.world.is_land(i) {
                continue;
            }
            if let Some(&(k, t)) = acc.get(&self.patch_of_tile(i)) {
                if t > 0.0 {
                    *v = (k / t) as f32;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strain(v: f32, t: f32, trade_off: bool) -> Strain {
        Strain {
            id: 1,
            parent: NO_STRAIN,
            lineage: 1,
            host: 0,
            virulence: v,
            transmissibility: t,
            escape: 0,
            trade_off,
            origin_year: 0.0,
            infected: 0,
            cases: 0,
            deaths: 0,
        }
    }

    #[test]
    fn the_trade_off_makes_intermediate_virulence_best() {
        let r = |v: f32| r0(&strain(v, 1.0, true), 0.5, 1000.0, 1.0);
        let best = (1..100)
            .map(|k| k as f32 / 100.0)
            .max_by(|a, b| r(*a).total_cmp(&r(*b)))
            .unwrap();
        // v* = sqrt(k·γ / α_max).
        let expect = (TRADE_OFF_K * RECOVERY / MAX_DEATH).sqrt();
        assert!((best - expect).abs() < 0.02, "{best} vs {expect}");
        // Without it the mildest strain spreads best.
        let r = |v: f32| r0(&strain(v, 1.0, false), 0.5, 1000.0, 1.0);
        assert!(r(0.05) > r(0.3) && r(0.3) > r(0.9));
    }

    #[test]
    fn resistance_lowers_susceptibility_unless_escaped() {
        let h = Health {
            res: [0b0011, 0b0001],
            sir: Sir::Susceptible,
        };
        assert_eq!(h.copies(), 3);
        assert_eq!(h.dosage(0), 1.0);
        assert_eq!(h.dosage(1), 0.5);
        assert!((h.susceptibility(0) - 0.5 * 0.75).abs() < 1e-6);
        assert!((h.susceptibility(0b0001) - 0.75).abs() < 1e-6);
        assert_eq!(Health::default().susceptibility(0), 1.0);
    }

    #[test]
    fn deme_health_splits_and_merges_without_losing_hosts() {
        let mut h = DemeHealth {
            res: [0.2; RES_LOCI],
            infected: vec![(1, 30), (4, 10)],
            recovered: 20,
        };
        let mut rng = Rng::new(3);
        let moved = h.split(100, 40, &mut rng);
        assert_eq!(
            moved.infected_total() + h.infected_total(),
            40,
            "infected conserved"
        );
        assert_eq!(moved.recovered + h.recovered, 20);
        assert!(moved.infected_total() + moved.recovered <= 40);
        let mut back = h.clone();
        back.merge(&moved, 0.6);
        assert_eq!(back.infected_total(), 40);
        assert_eq!(back.recovered, 20);
        let mut small = back.clone();
        small.fit(25);
        assert!(small.infected_total() + small.recovered <= 25);
    }

    #[test]
    fn gametes_mendelise() {
        let mut rng = Rng::new(9);
        let het = [0b1111, 0b0000];
        let mut ones = 0;
        for _ in 0..4000 {
            ones += gamete(het, &mut rng).count_ones();
        }
        let f = ones as f64 / 16000.0;
        assert!((f - 0.5).abs() < 0.02, "{f}");
    }
}
