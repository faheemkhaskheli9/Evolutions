//! Climate (TRD TR-C1–C4): temperature from latitude, lapse rate and
//! continentality; precipitation from latitude bands plus moisture advection
//! with orographic rain shadows; Whittaker-style biomes; Miami-model NPP.

use super::{Water, World};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Biome {
    Ocean = 0,
    Lake = 1,
    Ice = 2,
    Tundra = 3,
    Boreal = 4,
    ColdDesert = 5,
    TemperateGrassland = 6,
    TemperateForest = 7,
    TemperateRainforest = 8,
    HotDesert = 9,
    Savanna = 10,
    TropicalSeasonalForest = 11,
    TropicalRainforest = 12,
}

pub struct BiomeProps {
    pub name: &'static str,
    /// Fraction of NPP that is edible to ground herbivores.
    pub edible_fraction: f32,
    /// Background luminance (0 dark .. 1 bright) used for camouflage.
    pub luminance: f32,
    /// Vegetation cover that helps prey hide (0..1).
    pub cover: f32,
    /// Extra NPP fraction available as woody browse (coarse forage).
    pub browse: f32,
}

impl Biome {
    pub const ALL: [Biome; 13] = [
        Biome::Ocean,
        Biome::Lake,
        Biome::Ice,
        Biome::Tundra,
        Biome::Boreal,
        Biome::ColdDesert,
        Biome::TemperateGrassland,
        Biome::TemperateForest,
        Biome::TemperateRainforest,
        Biome::HotDesert,
        Biome::Savanna,
        Biome::TropicalSeasonalForest,
        Biome::TropicalRainforest,
    ];

    pub fn props(self) -> BiomeProps {
        let (name, edible_fraction, luminance, cover, browse) = match self {
            Biome::Ocean => ("Ocean", 0.0, 0.2, 0.0, 0.0),
            Biome::Lake => ("Lake", 0.0, 0.25, 0.0, 0.0),
            Biome::Ice => ("Ice", 0.0, 0.95, 0.0, 0.0),
            Biome::Tundra => ("Tundra", 0.45, 0.6, 0.1, 0.02),
            Biome::Boreal => ("Boreal forest", 0.15, 0.3, 0.6, 0.12),
            Biome::ColdDesert => ("Cold desert", 0.35, 0.7, 0.05, 0.05),
            Biome::TemperateGrassland => ("Temperate grassland", 0.5, 0.55, 0.15, 0.03),
            Biome::TemperateForest => ("Temperate forest", 0.2, 0.32, 0.55, 0.2),
            Biome::TemperateRainforest => ("Temperate rainforest", 0.12, 0.25, 0.7, 0.15),
            Biome::HotDesert => ("Hot desert", 0.35, 0.78, 0.03, 0.05),
            Biome::Savanna => ("Savanna", 0.45, 0.62, 0.2, 0.1),
            Biome::TropicalSeasonalForest => ("Tropical seasonal forest", 0.2, 0.32, 0.55, 0.18),
            Biome::TropicalRainforest => ("Tropical rainforest", 0.1, 0.2, 0.75, 0.12),
        };
        BiomeProps {
            name,
            edible_fraction,
            luminance,
            cover,
            browse,
        }
    }

    pub fn classify(water: Water, t: f32, p: f32) -> Biome {
        match water {
            Water::Ocean => return Biome::Ocean,
            Water::Lake => return Biome::Lake,
            _ => {}
        }
        if t < -12.0 {
            Biome::Ice
        } else if t < -4.0 {
            Biome::Tundra
        } else if t < 3.0 {
            if p < 250.0 {
                Biome::ColdDesert
            } else {
                Biome::Boreal
            }
        } else if t < 18.0 {
            if p < 250.0 {
                Biome::ColdDesert
            } else if p < 650.0 {
                Biome::TemperateGrassland
            } else if p < 1700.0 {
                Biome::TemperateForest
            } else {
                Biome::TemperateRainforest
            }
        } else if p < 300.0 {
            Biome::HotDesert
        } else if p < 1100.0 {
            Biome::Savanna
        } else if p < 2000.0 {
            Biome::TropicalSeasonalForest
        } else {
            Biome::TropicalRainforest
        }
    }
}

/// Miami model (Lieth 1975): NPP in g dry matter / m² / yr from annual mean
/// temperature (°C) and precipitation (mm).
pub fn miami_npp(t: f32, p: f32) -> f32 {
    let nt = 3000.0 / (1.0 + (1.315 - 0.119 * t).exp());
    let np = 3000.0 * (1.0 - (-0.000664 * p).exp());
    nt.min(np).max(0.0)
}

/// Zonal precipitation baseline (mm/yr): wet ITCZ, dry subtropics, wet mid-latitudes.
fn zonal_precip(lat: f64) -> f64 {
    let a = lat.abs();
    250.0 + 1900.0 * (-(a / 11.0).powi(2)).exp() + 850.0 * (-((a - 50.0) / 13.0).powi(2)).exp()
        - 180.0 * (-((a - 27.0) / 7.0).powi(2)).exp()
}

pub fn compute_static(world: &mut World) {
    let (w, h) = (world.w, world.h);
    let n = w * h;

    // Distance to the ocean (BFS), km.
    let mut dist = vec![u32::MAX; n];
    let mut q = VecDeque::new();
    for i in 0..n {
        if world.water[i] == Water::Ocean {
            dist[i] = 0;
            q.push_back(i);
        }
    }
    while let Some(i) = q.pop_front() {
        for d in 0..8 {
            if let Some(j) = world.neighbour(i, d) {
                if dist[j] == u32::MAX {
                    dist[j] = dist[i] + 1;
                    q.push_back(j);
                }
            }
        }
    }
    for i in 0..n {
        world.continentality[i] = if dist[i] == u32::MAX {
            1500.0
        } else {
            (dist[i] as f64 * world.tile_km).min(3000.0) as f32
        };
    }

    // Temperature.
    let tilt = world.gen.axial_tilt / 23.44;
    for y in 0..h {
        let lat = world.lat[y] as f64;
        let lr = lat.to_radians();
        let sea_level_t = -17.0 + 45.0 * lr.cos() + world.gen.temp_offset;
        for x in 0..w {
            let i = y * w + x;
            let elev = world.elevation[i].max(0.0) as f64;
            let cont = (world.continentality[i] as f64 / 1200.0).min(1.0);
            world.temp_base[i] = (sea_level_t - 0.0065 * elev) as f32;
            let amp = (1.5 + 17.0 * lr.sin().abs()) * tilt * (0.5 + 0.6 * cont);
            world.temp_amp[i] = (amp * lat.signum()) as f32;
        }
    }

    // Precipitation: zonal baseline times moisture carried by the prevailing wind.
    let decay_km = 1700.0;
    for y in 0..h {
        let lat = world.lat[y] as f64;
        let a = lat.abs();
        let dx: i64 = if (30.0..60.0).contains(&a) { 1 } else { -1 };
        let base = zonal_precip(lat);
        let start = (0..w).find(|&x| world.water[y * w + x] == Water::Ocean);
        let mut m = if start.is_some() { 1.0 } else { 0.7 };
        let x0 = start.unwrap_or(0) as i64;
        let mut prev_elev = 0.0f64;
        for step in 0..(2 * w) as i64 {
            let x = (x0 + dx * step).rem_euclid(w as i64) as usize;
            let i = y * w + x;
            if world.water[i] == Water::Ocean {
                m = 1.0;
                prev_elev = 0.0;
                if step >= w as i64 {
                    world.precip[i] = base as f32;
                }
                continue;
            }
            let elev = world.elevation[i].max(0.0) as f64;
            let rise = ((elev - prev_elev) / 1000.0).max(0.0);
            let oro = (1.0 + 2.5 * rise).min(3.0);
            if step >= w as i64 || start.is_none() {
                world.precip[i] = (base * (0.2 + 0.8 * m) * oro * world.gen.precip_factor) as f32;
            }
            m *= (-world.tile_km / decay_km).exp() * (1.0 - (0.5 * rise).min(0.6));
            m = m.max(0.05);
            prev_elev = elev;
        }
    }
}
