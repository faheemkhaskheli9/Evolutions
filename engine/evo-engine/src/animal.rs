//! Individual organism state (Mode I).

use crate::genetics::Genome;
use crate::physiology::Derived;
use crate::traits::{Phenotype, N_TRAITS};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Sex {
    Female = 0,
    Male = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Action {
    Rest = 0,
    Graze = 1,
    Hunt = 2,
    Drink = 3,
    Flee = 4,
    SeekMate = 5,
    Disperse = 6,
    FollowMother = 7,
    Wander = 8,
    Scavenge = 9,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Rest => "Resting",
            Action::Graze => "Foraging",
            Action::Hunt => "Hunting",
            Action::Drink => "Looking for water",
            Action::Flee => "Fleeing",
            Action::SeekMate => "Seeking a mate",
            Action::Disperse => "Dispersing",
            Action::FollowMother => "Following mother",
            Action::Wander => "Wandering",
            Action::Scavenge => "Scavenging",
        }
    }

    /// Metabolic rate as a multiple of resting metabolism.
    pub fn activity(self) -> f32 {
        match self {
            Action::Rest => 1.0,
            Action::Graze | Action::Scavenge => 1.6,
            Action::Hunt | Action::Flee => 2.5,
            Action::Drink | Action::Wander | Action::FollowMother => 1.5,
            Action::SeekMate => 1.8,
            Action::Disperse => 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cause {
    Starvation,
    Dehydration,
    Predation,
    OldAge,
    Heat,
    Cold,
    Accident,
    Stillbirth,
    Catastrophe,
}

impl Cause {
    pub const ALL: [Cause; 9] = [
        Cause::Starvation,
        Cause::Dehydration,
        Cause::Predation,
        Cause::OldAge,
        Cause::Heat,
        Cause::Cold,
        Cause::Accident,
        Cause::Stillbirth,
        Cause::Catastrophe,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Cause::Starvation => "starvation",
            Cause::Dehydration => "dehydration",
            Cause::Predation => "predation",
            Cause::OldAge => "old age",
            Cause::Heat => "heat stroke",
            Cause::Cold => "cold exposure",
            Cause::Accident => "accident",
            Cause::Stillbirth => "non-viable embryo",
            Cause::Catastrophe => "catastrophe",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pregnancy {
    pub father: u64,
    pub due_minute: u64,
    /// Embryo genomes with the number of new mutations each carries.
    pub embryos: Vec<(Genome, u32)>,
    /// Total neonate mass being built, kg (energy is paid during gestation).
    pub litter_kg: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Animal {
    pub id: u64,
    pub species: u32,
    pub arch: u16,
    pub sex: Sex,
    pub x: f32,
    pub y: f32,
    /// Travel target (tile coordinates) when moving between tiles.
    pub target: Option<(f32, f32)>,
    pub birth_minute: u64,
    pub mother: u64,
    pub father: u64,
    pub generation: u32,
    pub genome: Genome,
    /// Genetic value + environmental deviation on the latent scale.
    pub latent: [f32; N_TRAITS],
    pub pheno: Phenotype,
    pub d: Derived,
    pub mass: f32,
    pub neonate_mass: f32,
    /// Energy reserve, kJ.
    pub energy: f32,
    /// Water deficit, g.
    pub water_deficit: f32,
    pub heat_damage: f32,
    pub cold_damage: f32,
    /// Developmental age in days (advances more slowly when undernourished).
    pub dev_days: f32,
    pub action: Action,
    pub fear: f32,
    pub flee_from: (f32, f32),
    pub hunt_cooldown: u16,
    pub disperse_hours: u16,
    pub disperse_dir: u8,
    pub pregnancy: Option<Pregnancy>,
    pub last_birth_minute: u64,
    pub offspring: u32,
    pub matings: u32,
    pub mutations: u32,
    pub alive: bool,
    pub cause: Option<Cause>,
}

impl Animal {
    pub fn age_days(&self, now_minute: u64, day_minutes: u64) -> f32 {
        (now_minute.saturating_sub(self.birth_minute)) as f32 / day_minutes as f32
    }

    pub fn is_mature(&self) -> bool {
        self.dev_days >= self.d.maturity_days
    }

    pub fn hunger(&self) -> f32 {
        (1.0 - self.energy / self.d.fat_cap_kj.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn thirst(&self) -> f32 {
        (self.water_deficit / self.d.water_cap_g.max(1.0)).clamp(0.0, 1.0)
    }
}
