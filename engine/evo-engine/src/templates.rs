//! World templates (PRD §7.1.1) and the random-world generator (FR-W1).

use crate::rng::{stream, Rng};
use crate::world::{LandShape, WorldGenParams};
use serde::{Deserialize, Serialize};

/// Where founders of one species are released, in fractions of the map.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FounderGroup {
    pub species: String,
    pub count: u32,
    /// 0 = bottlenecked, 1 = highly diverse.
    pub diversity: f32,
    /// Optional release region (fx, fy, radius as fraction of width). None = most suitable area.
    pub region: Option<(f32, f32, f32)>,
    /// Years after world creation at which to release (predators wait for prey to establish).
    #[serde(default)]
    pub delay_years: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldTemplate {
    pub key: String,
    pub name: String,
    pub description: String,
    pub gen: WorldGenParams,
    pub founders: Vec<FounderGroup>,
}

fn g(species: &str, count: u32, diversity: f32) -> FounderGroup {
    FounderGroup {
        species: species.into(),
        count,
        diversity,
        region: None,
        delay_years: 0.0,
    }
}

/// A predator group released after prey have had time to establish.
fn later(species: &str, count: u32, years: f32) -> FounderGroup {
    FounderGroup {
        delay_years: years,
        ..g(species, count, 0.8)
    }
}

fn gr(species: &str, count: u32, diversity: f32, fx: f32, fy: f32, r: f32) -> FounderGroup {
    FounderGroup {
        species: species.into(),
        count,
        diversity,
        region: Some((fx, fy, r)),
        delay_years: 0.0,
    }
}

pub fn templates() -> Vec<WorldTemplate> {
    let base = WorldGenParams::default();
    vec![
        WorldTemplate {
            key: "earth".into(),
            name: "Earth-like Continent".into(),
            description: "Temperate default: continents, mountain ranges, rivers and a balanced grazer–predator community.".into(),
            gen: WorldGenParams { lat_north: 65.0, lat_south: -10.0, land_fraction: 0.45, ..base.clone() },
            founders: vec![g("hare", 400, 0.8), g("deer", 150, 0.8), later("wolf", 60, 40.0)],
        },
        WorldTemplate {
            key: "archipelago".into(),
            name: "Archipelago".into(),
            description: "Galápagos-like islands near the equator: isolation and adaptive radiation.".into(),
            gen: WorldGenParams {
                shape: LandShape::Archipelago, lat_north: 8.0, lat_south: -8.0,
                land_fraction: 0.2, precip_factor: 0.7, ..base.clone()
            },
            founders: vec![g("hare", 300, 0.6)],
        },
        WorldTemplate {
            key: "pangaea".into(),
            name: "Pangaea".into(),
            description: "One supercontinent with a dry interior.".into(),
            gen: WorldGenParams { shape: LandShape::Supercontinent, land_fraction: 0.35, ..base.clone() },
            founders: vec![g("hare", 400, 0.8), g("deer", 150, 0.8), later("wolf", 50, 40.0), later("cat", 40, 40.0)],
        },
        WorldTemplate {
            key: "ice_age".into(),
            name: "Ice Age Eurasia".into(),
            description: "Cold steppe and tundra; strong selection for cold tolerance.".into(),
            gen: WorldGenParams {
                lat_north: 75.0, lat_south: 30.0, temp_offset: -6.0, land_fraction: 0.6, ..base.clone()
            },
            founders: vec![g("hare", 400, 0.8), g("deer", 150, 0.8), later("wolf", 60, 40.0)],
        },
        WorldTemplate {
            key: "rift".into(),
            name: "East African Rift".into(),
            description: "Equatorial savanna and highlands: hominins, apes, grazers and big cats.".into(),
            gen: WorldGenParams {
                shape: LandShape::Rift, lat_north: 15.0, lat_south: -15.0, land_fraction: 0.5,
                precip_factor: 0.8, ..base.clone()
            },
            founders: vec![g("hominin", 120, 0.8), g("ape", 80, 0.8), g("deer", 80, 0.8), later("cat", 40, 40.0)],
        },
        WorldTemplate {
            key: "island_dwarfism".into(),
            name: "Island Dwarfism Lab".into(),
            description: "Mainland with predators, offshore islands without. Watch island deer.".into(),
            gen: WorldGenParams {
                shape: LandShape::MainlandAndIslands, lat_north: 30.0, lat_south: 5.0,
                land_fraction: 0.35, ..base.clone()
            },
            founders: vec![
                gr("deer", 60, 0.8, 0.2, 0.5, 0.12),
                FounderGroup { delay_years: 40.0, ..gr("wolf", 40, 0.8, 0.2, 0.5, 0.12) },
                gr("deer", 30, 0.8, 0.75, 0.5, 0.25),
            ],
        },
        WorldTemplate {
            key: "desert".into(),
            name: "Desert & Oasis".into(),
            description: "Hot, dry world where water decides who lives.".into(),
            gen: WorldGenParams {
                lat_north: 40.0, lat_south: 5.0, precip_factor: 0.35, temp_offset: 3.0,
                land_fraction: 0.55, ..base.clone()
            },
            founders: vec![g("hare", 300, 0.8), g("deer", 150, 0.8), later("cat", 40, 40.0)],
        },
        WorldTemplate {
            key: "blank".into(),
            name: "Blank Slate".into(),
            description: "Flat, uniform world for controlled experiments and teaching.".into(),
            gen: WorldGenParams {
                width: 64, height: 32, shape: LandShape::Flat, lat_north: 46.0, lat_south: 44.0,
                land_fraction: 1.0, axial_tilt: 23.44, ..base
            },
            founders: vec![g("hare", 300, 0.8)],
        },
    ]
}

pub fn find(key: &str) -> Option<WorldTemplate> {
    templates().into_iter().find(|t| t.key == key)
}

/// A random but plausible world from a seed (FR-W1).
pub fn random(seed: u64) -> WorldTemplate {
    let mut r = Rng::keyed(seed, 0, 0, stream::WORLD_GEN ^ 0xABCD);
    let shapes = [
        LandShape::Plates,
        LandShape::Plates,
        LandShape::Supercontinent,
        LandShape::Archipelago,
    ];
    let shape = shapes[r.below(shapes.len())];
    let centre = r.range_f32(-20.0, 45.0) as f64;
    let span = r.range_f32(50.0, 110.0) as f64;
    let gen = WorldGenParams {
        shape,
        lat_north: (centre + span / 2.0).min(80.0),
        lat_south: (centre - span / 2.0).max(-80.0),
        land_fraction: if shape == LandShape::Archipelago {
            r.range_f32(0.15, 0.25) as f64
        } else {
            r.range_f32(0.3, 0.6) as f64
        },
        plates: 6 + r.below(10),
        roughness: r.f64(),
        temp_offset: r.range_f32(-4.0, 4.0) as f64,
        precip_factor: r.range_f32(0.5, 1.4) as f64,
        axial_tilt: r.range_f32(10.0, 35.0) as f64,
        ..WorldGenParams::default()
    };
    let mut founders = vec![g("hare", 400, 0.8), g("deer", 150, 0.8)];
    founders.push(if r.chance(0.5) {
        later("wolf", 60, 40.0)
    } else {
        later("cat", 40, 40.0)
    });
    if r.chance(0.3) {
        founders.push(g("mouse", 300, 0.8));
    }
    WorldTemplate {
        key: "random".into(),
        name: "Random World".into(),
        description: format!("Generated from seed {seed}."),
        gen,
        founders,
    }
}
