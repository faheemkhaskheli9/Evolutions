//! World: terrain, hydrology, climate, biomes and vegetation (TRD §5.1–5.3).

mod climate;
pub mod hydrology;
mod terrain;

pub use climate::Biome;

use serde::{Deserialize, Serialize};

/// Shape family used by the terrain generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandShape {
    /// Voronoi tectonic plates with convergent mountain ranges.
    Plates,
    /// Many small islands.
    Archipelago,
    /// One large central landmass.
    Supercontinent,
    /// Flat land everywhere (controlled experiments).
    Flat,
    /// Elongated north–south landmass with a rift highland.
    Rift,
    /// A mainland on one side, small islands on the other.
    MainlandAndIslands,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldGenParams {
    pub width: usize,
    pub height: usize,
    /// Latitude of the northern edge (row 0) and southern edge, degrees.
    pub lat_north: f64,
    pub lat_south: f64,
    pub shape: LandShape,
    pub land_fraction: f64,
    pub plates: usize,
    pub roughness: f64,
    pub mountain_height_m: f64,
    /// Global temperature offset, °C.
    pub temp_offset: f64,
    /// Multiplier on precipitation.
    pub precip_factor: f64,
    /// Axial tilt, degrees (Earth 23.44). Controls seasonality.
    pub axial_tilt: f64,
}

impl Default for WorldGenParams {
    fn default() -> Self {
        WorldGenParams {
            width: 128,
            height: 64,
            lat_north: 62.0,
            lat_south: -62.0,
            shape: LandShape::Plates,
            land_fraction: 0.4,
            plates: 10,
            roughness: 0.5,
            mountain_height_m: 4000.0,
            temp_offset: 0.0,
            precip_factor: 1.0,
            axial_tilt: 23.44,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Water {
    None = 0,
    Ocean = 1,
    Lake = 2,
    River = 3,
}

/// Direction codes for the 8-neighbourhood; `HERE` means "already at target".
pub const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
pub const HERE: u8 = 8;
/// Share of edible NPP that is high-quality forage.
pub const HQ_SHARE: f32 = 0.35;
pub const NOWHERE: u8 = 9;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct World {
    pub gen: WorldGenParams,
    pub w: usize,
    pub h: usize,
    /// Approximate north–south tile size in km.
    pub tile_km: f64,
    pub lat: Vec<f32>,
    /// Elevation above sea level, metres (negative = sea floor).
    pub elevation: Vec<f32>,
    pub water: Vec<Water>,
    pub river_flow: Vec<f32>,
    /// Probability per hour of searching that an animal finds drinkable water inside the tile.
    pub water_access: Vec<f32>,
    /// Direction toward the nearest tile with good water access, and the distance in tiles.
    pub water_dir: Vec<u8>,
    pub water_dist: Vec<u16>,
    /// Distance to the ocean, km.
    pub continentality: Vec<f32>,
    /// Annual mean temperature without the global offset, °C.
    pub temp_base: Vec<f32>,
    /// Seasonal half-amplitude, °C (signed: negative in the southern hemisphere).
    pub temp_amp: Vec<f32>,
    /// Annual precipitation, mm.
    pub precip: Vec<f32>,
    pub biome: Vec<Biome>,
    /// Net primary productivity, g dry matter / m² / yr (Miami model).
    pub npp: Vec<f32>,
    /// Standing high-quality forage (forbs, seeds, fresh shoots), g / m².
    pub forage: Vec<f32>,
    pub forage_max: Vec<f32>,
    /// Standing coarse forage (mature grass, woody browse), g / m². Only large
    /// guts digest it well (Jarman–Bell principle).
    pub roughage: Vec<f32>,
    pub roughage_max: Vec<f32>,
    /// Carcass energy lying in the tile, kJ.
    pub carcass: Vec<f32>,
    /// Global temperature offset applied by events and interventions, °C.
    pub global_offset: f32,
    pub has_ocean: bool,
    /// Changes whenever the climate-derived fields are recomputed, so caches built
    /// from them (Population-mode patch contexts) know when they are stale.
    #[serde(skip)]
    pub revision: u64,
}

impl World {
    pub fn generate(gen: WorldGenParams, seed: u64) -> World {
        let (w, h) = (gen.width, gen.height);
        let n = w * h;
        let elevation = terrain::generate(&gen, seed);
        let lat: Vec<f32> = (0..h)
            .map(|y| {
                (gen.lat_north - (y as f64 + 0.5) / h as f64 * (gen.lat_north - gen.lat_south))
                    as f32
            })
            .collect();
        let tile_km = ((gen.lat_north - gen.lat_south).abs() / h as f64 * 111.32).max(1.0);
        let water: Vec<Water> = elevation
            .iter()
            .map(|&e| if e < 0.0 { Water::Ocean } else { Water::None })
            .collect();
        let has_ocean = water.contains(&Water::Ocean);
        let mut world = World {
            gen,
            w,
            h,
            tile_km,
            lat,
            elevation,
            water,
            river_flow: vec![0.0; n],
            water_access: vec![0.0; n],
            water_dir: vec![NOWHERE; n],
            water_dist: vec![u16::MAX; n],
            continentality: vec![0.0; n],
            temp_base: vec![0.0; n],
            temp_amp: vec![0.0; n],
            precip: vec![0.0; n],
            biome: vec![Biome::Ocean; n],
            npp: vec![0.0; n],
            forage: vec![0.0; n],
            forage_max: vec![0.0; n],
            roughage: vec![0.0; n],
            roughage_max: vec![0.0; n],
            carcass: vec![0.0; n],
            global_offset: 0.0,
            has_ocean,
            revision: 0,
        };
        climate::compute_static(&mut world);
        hydrology::compute(&mut world);
        world.refresh_biomes();
        for i in 0..n {
            world.forage[i] = world.forage_max[i] * 0.8;
            world.roughage[i] = world.roughage_max[i] * 0.8;
        }
        world
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    /// Tile containing a continuous position (x wraps, y clamps).
    #[inline]
    pub fn tile_of(&self, x: f32, y: f32) -> usize {
        let tx = (x.floor() as i64).rem_euclid(self.w as i64) as usize;
        let ty = (y.floor() as i64).clamp(0, self.h as i64 - 1) as usize;
        ty * self.w + tx
    }

    #[inline]
    pub fn wrap_x(&self, x: f32) -> f32 {
        x.rem_euclid(self.w as f32)
    }

    /// Neighbour tile in direction `d`, or None off the poles.
    #[inline]
    pub fn neighbour(&self, i: usize, d: usize) -> Option<usize> {
        let (x, y) = ((i % self.w) as i32, (i / self.w) as i32);
        let (dx, dy) = DIRS[d];
        let ny = y + dy;
        if ny < 0 || ny >= self.h as i32 {
            return None;
        }
        let nx = (x + dx).rem_euclid(self.w as i32);
        Some(ny as usize * self.w + nx as usize)
    }

    #[inline]
    pub fn is_land(&self, i: usize) -> bool {
        self.water[i] != Water::Ocean
    }

    pub fn land_tiles(&self) -> usize {
        (0..self.w * self.h).filter(|&i| self.is_land(i)).count()
    }

    /// Mean annual temperature including the global offset.
    #[inline]
    pub fn temp_mean(&self, i: usize) -> f32 {
        self.temp_base[i] + self.global_offset
    }

    /// Air temperature at a given day of year and hour of day.
    pub fn temp_at(&self, i: usize, day_of_year: f64, hour: f64) -> f32 {
        let season = (std::f64::consts::TAU * (day_of_year - 200.0) / 365.25).cos() as f32;
        let dry = 1.0 - (self.precip[i] / 1500.0).clamp(0.0, 1.0);
        let diurnal =
            (3.0 + 6.0 * dry) * (std::f64::consts::TAU * (hour - 15.0) / 24.0).cos() as f32;
        self.temp_mean(i) + self.temp_amp[i] * season + diurnal
    }

    /// Temperature from precomputed seasonal and diurnal cosines (hot path).
    #[inline]
    pub fn temp_fast(&self, i: usize, season_cos: f32, diurnal_cos: f32) -> f32 {
        let dry = 1.0 - (self.precip[i] / 1500.0).clamp(0.0, 1.0);
        self.temp_mean(i) + self.temp_amp[i] * season_cos + (3.0 + 6.0 * dry) * diurnal_cos
    }

    /// Seasonal cosine for a day of year: +1 at the warmest day, −1 at the coldest.
    #[inline]
    pub fn season_cos(day_of_year: f64) -> f32 {
        (std::f64::consts::TAU * (day_of_year - 200.0) / 365.25).cos() as f32
    }

    /// Mean temperature of the given day (no diurnal swing).
    pub fn temp_day(&self, i: usize, day_of_year: f64) -> f32 {
        self.temp_season(i, Self::season_cos(day_of_year))
    }

    /// Mean daily temperature from a precomputed [`World::season_cos`] (hot path).
    #[inline]
    pub fn temp_season(&self, i: usize, season_cos: f32) -> f32 {
        self.temp_mean(i) + self.temp_amp[i] * season_cos
    }

    /// Background luminance used for camouflage; snow makes it bright.
    pub fn background_luminance(&self, i: usize, day_temp: f32) -> f32 {
        if day_temp < -2.0 && self.precip[i] > 150.0 {
            0.9
        } else {
            self.biome[i].props().luminance
        }
    }

    /// Recompute biome, NPP and forage capacity after a climate change.
    pub fn refresh_biomes(&mut self) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        self.revision = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        for i in 0..self.w * self.h {
            let t = self.temp_mean(i);
            let p = self.precip[i];
            self.biome[i] = Biome::classify(self.water[i], t, p);
            self.npp[i] = if self.is_land(i) && self.water[i] != Water::Lake {
                climate::miami_npp(t, p)
            } else {
                0.0
            };
            self.forage_max[i] = self.forage_production(i) * 0.6;
            self.roughage_max[i] = self.roughage_production(i) * 0.8;
            self.forage[i] = self.forage[i].min(self.forage_max[i]);
            self.roughage[i] = self.roughage[i].min(self.roughage_max[i]);
        }
    }

    /// Annual high-quality forage production, g / m² / yr.
    #[inline]
    pub fn forage_production(&self, i: usize) -> f32 {
        self.npp[i] * self.biome[i].props().edible_fraction * HQ_SHARE
    }

    /// Annual coarse forage production, g / m² / yr.
    #[inline]
    pub fn roughage_production(&self, i: usize) -> f32 {
        let p = self.biome[i].props();
        self.npp[i] * (p.edible_fraction * (1.0 - HQ_SHARE) + p.browse)
    }

    /// Logistic regrowth of both forage pools over `days` at a given day of year.
    fn grow(&mut self, i: usize, season_cos: f32, days: f32) {
        let t = self.temp_season(i, season_cos);
        let season = (t / 10.0).clamp(0.0, 1.0);
        for (stock, max, prod) in [
            (
                &mut self.forage,
                &self.forage_max,
                self.npp[i] * self.biome[i].props().edible_fraction * HQ_SHARE,
            ),
            (&mut self.roughage, &self.roughage_max, {
                let p = self.biome[i].props();
                self.npp[i] * (p.edible_fraction * (1.0 - HQ_SHARE) + p.browse)
            }),
        ] {
            if max[i] <= 0.0 {
                continue;
            }
            let b = stock[i];
            let daily = prod / 365.0 * 2.0 * season;
            stock[i] =
                (b + days * (daily * (1.0 - b / max[i]).max(0.0) - b * 0.002)).clamp(0.0, max[i]);
        }
    }

    /// Daily plant growth and carcass decay.
    pub fn grow_vegetation_day(&mut self, day_of_year: f64, carcass_decay: f32) {
        let sc = Self::season_cos(day_of_year);
        for i in 0..self.w * self.h {
            self.grow(i, sc, 1.0);
            self.carcass[i] *= 1.0 - carcass_decay;
        }
    }

    /// One year of vegetation in monthly steps (Population mode, display only; demes
    /// draw on annual production, so coarse steps cost accuracy only on the map).
    pub fn grow_vegetation_year(&mut self) {
        for month in 0..12 {
            let sc = Self::season_cos(month as f64 * 30.44 + 15.0);
            for i in 0..self.w * self.h {
                self.grow(i, sc, 30.44);
            }
        }
        for c in self.carcass.iter_mut() {
            *c = 0.0;
        }
    }

    /// Recompute the water-direction field (multi-source BFS from tiles with good access).
    pub fn rebuild_water_field(&mut self) {
        hydrology::water_field(self);
    }

    /// Apply a global climate shift (intervention) and refresh derived fields.
    pub fn shift_climate(&mut self, delta_c: f32) {
        self.global_offset += delta_c;
        self.refresh_biomes();
    }
}
