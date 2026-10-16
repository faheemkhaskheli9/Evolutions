//! Heritable traits (TRD TR-B2). Genes act on a latent scale; each trait maps the
//! latent value to a phenotype with a fixed transform, so effects stay additive
//! on the latent scale while phenotypes stay inside biological bounds.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Trait {
    BodyMass = 0,
    Insulation,
    Appendage,
    LegLength,
    Sprint,
    Diet,
    CoatLuminance,
    Senses,
    Brain,
    Boldness,
    Sociality,
    LitterSize,
    Pace,
    Maintenance,
    Ornament,
    Preference,
    Choosiness,
    BreedingDay,
    Dispersal,
    Kidney,
    HeatTolerance,
}

pub const N_TRAITS: usize = 21;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    /// phenotype = exp(latent), clamped.
    Exp,
    /// phenotype = 1 / (1 + exp(-latent)).
    Logistic,
}

#[derive(Clone, Copy, Debug)]
pub enum TraitClass {
    Morphology,
    LifeHistory,
    Behaviour,
}

pub struct TraitInfo {
    pub key: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub transform: Transform,
    pub lo: f32,
    pub hi: f32,
    pub class: TraitClass,
    /// Loci controlling the trait.
    pub loci: usize,
}

impl Trait {
    pub const ALL: [Trait; N_TRAITS] = [
        Trait::BodyMass,
        Trait::Insulation,
        Trait::Appendage,
        Trait::LegLength,
        Trait::Sprint,
        Trait::Diet,
        Trait::CoatLuminance,
        Trait::Senses,
        Trait::Brain,
        Trait::Boldness,
        Trait::Sociality,
        Trait::LitterSize,
        Trait::Pace,
        Trait::Maintenance,
        Trait::Ornament,
        Trait::Preference,
        Trait::Choosiness,
        Trait::BreedingDay,
        Trait::Dispersal,
        Trait::Kidney,
        Trait::HeatTolerance,
    ];

    pub fn from_index(i: usize) -> Trait {
        Trait::ALL[i]
    }

    pub fn info(self) -> TraitInfo {
        use Trait::*;
        use TraitClass::*;
        use Transform::*;
        let (key, label, unit, transform, lo, hi, class, loci) = match self {
            BodyMass => (
                "body_mass",
                "Adult body mass",
                "kg",
                Exp,
                0.01,
                6000.0,
                Morphology,
                8,
            ),
            Insulation => (
                "insulation",
                "Insulation (fur/fat)",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                6,
            ),
            Appendage => (
                "appendage",
                "Appendage size (ears, tail)",
                "×",
                Exp,
                0.4,
                2.5,
                Morphology,
                5,
            ),
            LegLength => (
                "leg_length",
                "Leg length index",
                "×",
                Exp,
                0.5,
                2.0,
                Morphology,
                5,
            ),
            Sprint => (
                "sprint",
                "Muscle: sprint vs endurance",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                5,
            ),
            Diet => (
                "diet",
                "Diet (0 plant … 1 meat)",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                6,
            ),
            CoatLuminance => (
                "coat",
                "Coat luminance",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                5,
            ),
            Senses => (
                "senses",
                "Sensory acuity",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                5,
            ),
            Brain => (
                "brain",
                "Relative brain size (EQ)",
                "EQ",
                Exp,
                0.2,
                8.0,
                Morphology,
                6,
            ),
            Boldness => (
                "boldness", "Boldness", "0–1", Logistic, 0.0, 1.0, Behaviour, 4,
            ),
            Sociality => (
                "sociality",
                "Sociality",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Behaviour,
                4,
            ),
            LitterSize => (
                "litter",
                "Litter size",
                "young",
                Exp,
                1.0,
                15.0,
                LifeHistory,
                5,
            ),
            Pace => (
                "pace",
                "Slow life history (pace)",
                "×",
                Exp,
                0.3,
                8.0,
                LifeHistory,
                5,
            ),
            Maintenance => (
                "maintenance",
                "Somatic maintenance",
                "0–1",
                Logistic,
                0.0,
                1.0,
                LifeHistory,
                4,
            ),
            Ornament => (
                "ornament",
                "Ornament size",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                4,
            ),
            Preference => (
                "preference",
                "Preferred ornament",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Behaviour,
                4,
            ),
            Choosiness => (
                "choosiness",
                "Mate choosiness",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Behaviour,
                3,
            ),
            BreedingDay => (
                "breeding_day",
                "Breeding season (fraction of year)",
                "0–1",
                Logistic,
                0.0,
                1.0,
                LifeHistory,
                4,
            ),
            Dispersal => (
                "dispersal",
                "Dispersal propensity",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Behaviour,
                3,
            ),
            Kidney => (
                "kidney",
                "Kidney concentrating ability",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                4,
            ),
            HeatTolerance => (
                "heat_tolerance",
                "Evaporative cooling capacity",
                "0–1",
                Logistic,
                0.0,
                1.0,
                Morphology,
                4,
            ),
        };
        TraitInfo {
            key,
            label,
            unit,
            transform,
            lo,
            hi,
            class,
            loci,
        }
    }

    /// Narrow-sense heritability used to split founder variance (TRD TR-G4).
    pub fn heritability(self) -> f32 {
        match self.info().class {
            TraitClass::Morphology => 0.5,
            TraitClass::LifeHistory => 0.25,
            TraitClass::Behaviour => 0.3,
        }
    }

    pub fn to_phenotype(self, latent: f32) -> f32 {
        let info = self.info();
        let v = match info.transform {
            Transform::Exp => latent.exp(),
            Transform::Logistic => 1.0 / (1.0 + (-latent).exp()),
        };
        v.clamp(info.lo, info.hi)
    }

    pub fn to_latent(self, phen: f32) -> f32 {
        match self.info().transform {
            Transform::Exp => phen.max(1e-6).ln(),
            Transform::Logistic => {
                let p = phen.clamp(1e-4, 1.0 - 1e-4);
                (p / (1.0 - p)).ln()
            }
        }
    }

    /// Converts a phenotypic standard deviation around `mean` to the latent scale.
    pub fn latent_sd(self, mean: f32, phen_sd: f32) -> f32 {
        match self.info().transform {
            Transform::Exp => phen_sd / mean.max(1e-6),
            Transform::Logistic => {
                let m = mean.clamp(0.02, 0.98);
                phen_sd / (m * (1.0 - m))
            }
        }
    }
}

/// Developed phenotype values (phenotype scale), indexed by `Trait as usize`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Phenotype(pub [f32; N_TRAITS]);

impl Phenotype {
    #[inline]
    pub fn get(&self, t: Trait) -> f32 {
        self.0[t as usize]
    }

    pub fn from_latent(latent: &[f32; N_TRAITS]) -> Phenotype {
        let mut p = [0.0; N_TRAITS];
        for (i, v) in p.iter_mut().enumerate() {
            *v = Trait::from_index(i).to_phenotype(latent[i]);
        }
        Phenotype(p)
    }
}
