//! Founder species library (PRD FR-S1). Values are phenotype means; founders get
//! realistic standing variation around them.

use crate::genetics::FounderSpec;
use crate::traits::{Trait, N_TRAITS};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct SpeciesTemplate {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub color: [u8; 3],
    pub traits: Vec<(Trait, f32)>,
}

fn defaults() -> [f32; N_TRAITS] {
    let mut v = [0.5f32; N_TRAITS];
    v[Trait::BodyMass as usize] = 5.0;
    v[Trait::Appendage as usize] = 1.0;
    v[Trait::LegLength as usize] = 1.0;
    v[Trait::Brain as usize] = 1.0;
    v[Trait::LitterSize as usize] = 2.0;
    v[Trait::Pace as usize] = 1.0;
    v[Trait::Insulation as usize] = 0.45;
    v[Trait::Ornament as usize] = 0.3;
    v[Trait::Preference as usize] = 0.3;
    v[Trait::Choosiness as usize] = 0.4;
    v[Trait::BreedingDay as usize] = 0.35;
    v[Trait::Dispersal as usize] = 0.4;
    v
}

impl SpeciesTemplate {
    pub fn phenotype_means(&self) -> [f32; N_TRAITS] {
        let mut v = defaults();
        for (t, x) in &self.traits {
            v[*t as usize] = *x;
        }
        v
    }

    /// Founder specification on the latent scale with typical variation:
    /// CV ≈ 8% for log-scale traits, latent SD 0.25 for bounded (0–1) traits.
    pub fn founder_spec(&self, diversity: f32) -> FounderSpec {
        let means = self.phenotype_means();
        let mut latent_mean = [0f32; N_TRAITS];
        let mut latent_sd = [0f32; N_TRAITS];
        for t in Trait::ALL {
            let i = t as usize;
            latent_mean[i] = t.to_latent(means[i]);
            latent_sd[i] = match t.info().transform {
                crate::traits::Transform::Exp => t.latent_sd(means[i], 0.08 * means[i]),
                // Constant on the logistic scale: ±0.06 near 0.5, much narrower near the bounds.
                crate::traits::Transform::Logistic => 0.25,
            };
        }
        FounderSpec {
            latent_mean,
            latent_sd,
            diversity,
        }
    }
}

pub fn library() -> Vec<SpeciesTemplate> {
    use Trait::*;
    vec![
        SpeciesTemplate {
            key: "hare",
            name: "Steppe hare",
            description: "Small, fast-breeding grazer (rabbit-like).",
            color: [222, 184, 135],
            traits: vec![
                (BodyMass, 2.0),
                (Insulation, 0.45),
                (Appendage, 1.25),
                (LegLength, 1.1),
                (Sprint, 0.75),
                (Diet, 0.03),
                (CoatLuminance, 0.55),
                (Senses, 0.65),
                (Brain, 0.8),
                (Boldness, 0.3),
                (Sociality, 0.35),
                (LitterSize, 4.5),
                (Pace, 0.55),
                (Maintenance, 0.3),
                (Ornament, 0.2),
                (Preference, 0.2),
                (Choosiness, 0.3),
                (BreedingDay, 0.3),
                (Dispersal, 0.45),
                (Kidney, 0.5),
                (HeatTolerance, 0.4),
            ],
        },
        SpeciesTemplate {
            key: "deer",
            name: "Plains deer",
            description: "Medium-large herding grazer with antler-like ornaments.",
            color: [160, 110, 60],
            traits: vec![
                (BodyMass, 60.0),
                (Insulation, 0.45),
                (LegLength, 1.3),
                (Sprint, 0.55),
                (Diet, 0.03),
                (CoatLuminance, 0.45),
                (Senses, 0.7),
                (Boldness, 0.35),
                (Sociality, 0.7),
                (LitterSize, 1.3),
                (Pace, 1.1),
                (Maintenance, 0.5),
                (Ornament, 0.5),
                (Preference, 0.5),
                (Choosiness, 0.5),
                (BreedingDay, 0.75),
                (Dispersal, 0.35),
                (HeatTolerance, 0.45),
            ],
        },
        SpeciesTemplate {
            key: "wolf",
            name: "Pack wolf",
            description: "Social endurance hunter.",
            color: [120, 120, 130],
            traits: vec![
                (BodyMass, 35.0),
                (Insulation, 0.5),
                (LegLength, 1.2),
                (Sprint, 0.4),
                (Diet, 0.95),
                (CoatLuminance, 0.5),
                (Senses, 0.8),
                (Brain, 1.5),
                (Boldness, 0.7),
                (Sociality, 0.85),
                (LitterSize, 5.0),
                (Pace, 0.9),
                (Maintenance, 0.5),
                (Ornament, 0.2),
                (Preference, 0.2),
                (BreedingDay, 0.12),
                (Dispersal, 0.5),
                (Kidney, 0.55),
                (HeatTolerance, 0.45),
            ],
        },
        SpeciesTemplate {
            key: "cat",
            name: "Ambush cat",
            description: "Solitary sprint-and-pounce predator.",
            color: [214, 160, 40],
            traits: vec![
                (BodyMass, 50.0),
                (Insulation, 0.35),
                (Sprint, 0.85),
                (Diet, 0.98),
                (CoatLuminance, 0.6),
                (Senses, 0.8),
                (Brain, 1.4),
                (Boldness, 0.8),
                (Sociality, 0.1),
                (LitterSize, 2.5),
                (Pace, 1.0),
                (Maintenance, 0.5),
                (BreedingDay, 0.4),
                (Dispersal, 0.55),
                (HeatTolerance, 0.55),
            ],
        },
        SpeciesTemplate {
            key: "mouse",
            name: "Field mouse",
            description: "Tiny seed-eater with a very fast life history.",
            color: [150, 140, 120],
            traits: vec![
                (BodyMass, 0.05),
                (Insulation, 0.4),
                (Appendage, 1.2),
                (Sprint, 0.6),
                (Diet, 0.15),
                (CoatLuminance, 0.45),
                (Senses, 0.55),
                (Brain, 0.7),
                (Sociality, 0.3),
                (LitterSize, 6.0),
                (Pace, 0.35),
                (Maintenance, 0.2),
                (BreedingDay, 0.4),
                (Dispersal, 0.5),
            ],
        },
        SpeciesTemplate {
            key: "hominin",
            name: "Early hominin",
            description: "Bipedal, social, large-brained omnivore that sweats to keep cool.",
            color: [190, 120, 100],
            traits: vec![
                (BodyMass, 40.0),
                (Insulation, 0.25),
                (LegLength, 1.1),
                (Sprint, 0.3),
                (Diet, 0.35),
                (CoatLuminance, 0.4),
                (Senses, 0.6),
                (Brain, 3.0),
                (Boldness, 0.6),
                (Sociality, 0.9),
                (LitterSize, 1.0),
                (Pace, 4.0),
                (Maintenance, 0.7),
                (Ornament, 0.3),
                (Preference, 0.3),
                (Choosiness, 0.6),
                (BreedingDay, 0.5),
                (Dispersal, 0.4),
                (HeatTolerance, 0.8),
            ],
        },
        SpeciesTemplate {
            key: "ape",
            name: "Forest ape",
            description: "Large, mostly plant-eating, slow-breeding forest primate.",
            color: [90, 70, 60],
            traits: vec![
                (BodyMass, 50.0),
                (Insulation, 0.45),
                (Sprint, 0.5),
                (Diet, 0.15),
                (CoatLuminance, 0.25),
                (Senses, 0.6),
                (Brain, 2.0),
                (Sociality, 0.7),
                (LitterSize, 1.0),
                (Pace, 3.0),
                (Maintenance, 0.6),
                (BreedingDay, 0.5),
                (Dispersal, 0.3),
                (HeatTolerance, 0.35),
            ],
        },
    ]
}

pub fn find(key: &str) -> Option<SpeciesTemplate> {
    library().into_iter().find(|s| s.key == key)
}
