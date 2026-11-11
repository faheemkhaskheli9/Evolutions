//! World editor (PRD FR-W3): brushes that paint elevation, sea, land, lakes and climate
//! onto a world, after which hydrology, climate and biomes are recomputed.
//!
//! An edit is a [`Stroke`]: one brush dragged through a list of points. Strokes are
//! applied through `Intervention::EditWorld`, so they are recorded in the replay log
//! and a replay gives the same world.
//!
//! Painted climate (the rainfall and temperature brushes) is not stored separately:
//! before an edit the offset of each tile from the generated climate is worked out,
//! and it is added back after the climate is recomputed for the new terrain. So the
//! state format is unchanged, and painted climate survives later terrain edits.

use super::{climate, hydrology, Water, World};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Brush {
    /// Raise the ground (up to 1,500 m per stroke at full strength).
    Raise,
    /// Lower the ground; land lowered below sea level becomes sea.
    Lower,
    /// Make every tile under the brush sea (10–500 m deep by strength).
    Ocean,
    /// Make every tile under the brush land (20–300 m high by strength).
    Land,
    /// Dig an inland basin that fills as a lake (3–50 m deep by strength).
    Lake,
    /// More rainfall (up to +1,000 mm a year).
    Wetter,
    /// Less rainfall (up to −1,000 mm a year).
    Drier,
    /// Warmer (up to +10 °C).
    Warmer,
    /// Colder (up to −10 °C).
    Colder,
}

impl Brush {
    /// Brushes that set every tile under them the same way (a sharp edge), so a strait
    /// is exactly as wide as the brush.
    fn sharp(self) -> bool {
        matches!(self, Brush::Ocean | Brush::Land | Brush::Lake)
    }

    pub fn verb(self) -> &'static str {
        match self {
            Brush::Raise => "raised the ground",
            Brush::Lower => "lowered the ground",
            Brush::Ocean => "painted sea",
            Brush::Land => "painted land",
            Brush::Lake => "dug a lake",
            Brush::Wetter => "made it wetter",
            Brush::Drier => "made it drier",
            Brush::Warmer => "made it warmer",
            Brush::Colder => "made it colder",
        }
    }
}

/// One brush stroke.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub brush: Brush,
    /// Brush radius in tiles, from tile centres (0.5 paints one tile).
    pub radius: f32,
    /// Strength, 0..1.
    pub strength: f32,
    /// Brush centres along the stroke, in tile coordinates (x east, y south).
    pub points: Vec<[f32; 2]>,
}

/// Most points in one stroke.
pub const MAX_POINTS: usize = 4096;
/// Largest brush radius, in tiles.
pub const MAX_RADIUS: f32 = 32.0;
/// Highest and lowest ground the brushes make, m.
const MAX_ELEV: f32 = 8000.0;
const MIN_ELEV: f32 = -8000.0;

impl Stroke {
    pub fn validate(&self, w: usize, h: usize) -> Result<(), String> {
        if !(0.5..=MAX_RADIUS).contains(&self.radius) {
            return Err(format!("brush radius must be 0.5..{MAX_RADIUS} tiles"));
        }
        if !(0.0..=1.0).contains(&self.strength) {
            return Err("brush strength must be 0..1".into());
        }
        if self.points.is_empty() || self.points.len() > MAX_POINTS {
            return Err(format!("a stroke needs 1..{MAX_POINTS} points"));
        }
        if self.points.iter().any(|p| {
            !p[0].is_finite()
                || !p[1].is_finite()
                || p[0] < -(w as f32)
                || p[0] > 2.0 * w as f32
                || p[1] < 0.0
                || p[1] > h as f32
        }) {
            return Err("a stroke point is outside the map".into());
        }
        Ok(())
    }
}

/// What an edit changed.
#[derive(Clone, Debug, Default)]
pub struct EditResult {
    /// Tiles under the brush.
    pub tiles: usize,
    /// Tiles that were dry land (or river) and are now sea or lake.
    pub flooded: Vec<usize>,
}

impl World {
    /// Brush weight of every tile under the stroke, in tile order: strength times a
    /// falloff from the centre (1 everywhere for sharp brushes). Overlapping dabs take
    /// the largest weight, so a slow drag does not paint harder than a fast one.
    fn stroke_weights(&self, s: &Stroke) -> Vec<(usize, f32)> {
        let (w, h) = (self.w as i64, self.h as i64);
        let mut weight = vec![0.0f32; self.w * self.h];
        let mut hit = vec![false; self.w * self.h];
        let r = s.radius;
        for p in &s.points {
            let (px, py) = (p[0], p[1]);
            let (x0, x1) = ((px - r).floor() as i64 - 1, (px + r).ceil() as i64 + 1);
            let (y0, y1) = (
                ((py - r).floor() as i64 - 1).max(0),
                ((py + r).ceil() as i64 + 1).min(h - 1),
            );
            for y in y0..=y1 {
                for x in x0..=x1.min(x0 + w - 1) {
                    let dx = x as f32 + 0.5 - px;
                    let dy = y as f32 + 0.5 - py;
                    let d = (dx * dx + dy * dy).sqrt();
                    if d > r {
                        continue;
                    }
                    let i = (y * w + x.rem_euclid(w)) as usize;
                    let f = if s.brush.sharp() {
                        1.0
                    } else {
                        1.0 - d / (r + 0.5)
                    };
                    hit[i] = true;
                    weight[i] = weight[i].max(s.strength * f);
                }
            }
        }
        (0..self.w * self.h)
            .filter(|&i| hit[i])
            .map(|i| (i, weight[i]))
            .collect()
    }

    /// Applies a brush stroke, then recomputes hydrology, climate and biomes
    /// ([`World::recompute_after_edit`]). The caller checks the stroke first.
    pub fn apply_stroke(&mut self, s: &Stroke) -> EditResult {
        let n = self.w * self.h;
        let wet_before: Vec<bool> = (0..n).map(|i| self.is_wet(i)).collect();
        // Offsets of the current climate from the climate the terrain alone gives.
        let (mut d_temp, mut d_precip) = self.climate_offsets();
        let tiles = self.stroke_weights(s);
        let k = s.strength;
        match s.brush {
            Brush::Raise | Brush::Lower => {
                let sign = if s.brush == Brush::Raise { 1.0 } else { -1.0 };
                for &(i, wt) in &tiles {
                    self.elevation[i] =
                        (self.elevation[i] + sign * 1500.0 * wt).clamp(MIN_ELEV, MAX_ELEV);
                }
            }
            Brush::Ocean => {
                let depth = -(10.0 + 490.0 * k);
                for &(i, _) in &tiles {
                    self.elevation[i] = self.elevation[i].min(depth);
                }
            }
            Brush::Land => {
                let height = 20.0 + 280.0 * k;
                for &(i, _) in &tiles {
                    self.elevation[i] = self.elevation[i].max(height);
                }
            }
            Brush::Lake => self.dig_lake(&tiles, 3.0 + 47.0 * k),
            Brush::Wetter | Brush::Drier => {
                let sign = if s.brush == Brush::Wetter { 1.0 } else { -1.0 };
                for &(i, wt) in &tiles {
                    d_precip[i] += sign * 1000.0 * wt;
                }
            }
            Brush::Warmer | Brush::Colder => {
                let sign = if s.brush == Brush::Warmer { 1.0 } else { -1.0 };
                for &(i, wt) in &tiles {
                    d_temp[i] += sign * 10.0 * wt;
                }
            }
        }
        self.recompute_after_edit(&d_temp, &d_precip);
        // New land starts with vegetation, as a generated world does.
        for i in 0..n {
            if wet_before[i] && !self.is_wet(i) {
                self.forage[i] = self.forage_max[i] * 0.8;
                self.roughage[i] = self.roughage_max[i] * 0.8;
            }
            if self.is_wet(i) {
                self.carcass[i] = 0.0;
            }
        }
        self.vegetation_changed();
        EditResult {
            tiles: tiles.len(),
            flooded: (0..n)
                .filter(|&i| !wet_before[i] && self.is_wet(i))
                .collect(),
        }
    }

    /// Sea or lake: no animal can stand there.
    fn is_wet(&self, i: usize) -> bool {
        matches!(self.water[i], Water::Ocean | Water::Lake)
    }

    /// Per-tile temperature and rainfall offsets from the climate the terrain gives.
    /// Recomputes the static climate in place; the caller recomputes it again after
    /// the edit and adds the offsets back.
    fn climate_offsets(&mut self) -> (Vec<f32>, Vec<f32>) {
        let (t, p) = (self.temp_base.clone(), self.precip.clone());
        climate::compute_static(self);
        let dt = t.iter().zip(&self.temp_base).map(|(a, b)| a - b).collect();
        let dp = p.iter().zip(&self.precip).map(|(a, b)| a - b).collect();
        (dt, dp)
    }

    /// Lowers the land tiles under the brush into a basin `depth` metres below its
    /// lowest rim, so the hydrology fills it as a lake. Rims lower than the basin
    /// needs are raised a little. A basin open to the sea drains, so lakes must be
    /// painted inland.
    fn dig_lake(&mut self, tiles: &[(usize, f32)], depth: f32) {
        let n = self.w * self.h;
        let mut inside = vec![false; n];
        for &(i, _) in tiles {
            if self.elevation[i] >= 0.0 {
                inside[i] = true;
            }
        }
        let mut rim = vec![];
        for i in (0..n).filter(|&i| inside[i]) {
            for d in 0..8 {
                if let Some(j) = self.neighbour(i, d) {
                    if !inside[j] && !rim.contains(&j) {
                        rim.push(j);
                    }
                }
            }
        }
        if rim.is_empty() && !inside.contains(&true) {
            return;
        }
        rim.sort_unstable();
        // Raise low land on the rim so the basin can be `depth` deep and stay above sea level.
        let need = depth + 1.0;
        for &j in &rim {
            if self.elevation[j] >= 0.0 && self.elevation[j] < need {
                self.elevation[j] = need;
            }
        }
        let low_rim = rim
            .iter()
            .map(|&j| self.elevation[j].max(0.0))
            .fold(f32::INFINITY, f32::min);
        let floor = if low_rim.is_finite() {
            (low_rim - depth).max(0.5)
        } else {
            0.5
        };
        for i in (0..n).filter(|&i| inside[i]) {
            self.elevation[i] = self.elevation[i].min(floor);
        }
    }

    /// Recomputes everything derived from the terrain after an edit: sea from
    /// elevation, distance to the sea, temperature and rainfall (plus the painted
    /// offsets), lakes, rivers, the water field, biomes and plant growth. On an
    /// unedited world with zero offsets it reproduces the generated world exactly.
    pub fn recompute_after_edit(&mut self, d_temp: &[f32], d_precip: &[f32]) {
        let n = self.w * self.h;
        for i in 0..n {
            self.water[i] = if self.elevation[i] < 0.0 {
                Water::Ocean
            } else {
                Water::None
            };
        }
        self.has_ocean = self.water.contains(&Water::Ocean);
        climate::compute_static(self);
        for i in 0..n {
            self.temp_base[i] += d_temp[i];
            self.precip[i] = (self.precip[i] + d_precip[i]).max(0.0);
        }
        self.river_flow.iter_mut().for_each(|f| *f = 0.0);
        hydrology::compute(self);
        self.refresh_biomes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{LandShape, WorldGenParams};

    fn flat() -> World {
        World::generate(
            WorldGenParams {
                width: 48,
                height: 24,
                shape: LandShape::Flat,
                ..Default::default()
            },
            3,
        )
    }

    fn stroke(brush: Brush, radius: f32, points: &[[f32; 2]]) -> Stroke {
        Stroke {
            brush,
            radius,
            strength: 1.0,
            points: points.to_vec(),
        }
    }

    #[test]
    fn recompute_without_edits_changes_nothing() {
        let mut w = flat();
        let before = bincode::serialize(&w).unwrap();
        let zero = vec![0.0; w.w * w.h];
        w.recompute_after_edit(&zero, &zero);
        assert_eq!(bincode::serialize(&w).unwrap(), before);
        let mut p = World::generate(WorldGenParams::default(), 9);
        let before = bincode::serialize(&p).unwrap();
        let zero = vec![0.0; p.w * p.h];
        p.recompute_after_edit(&zero, &zero);
        assert_eq!(bincode::serialize(&p).unwrap(), before);
    }

    #[test]
    fn sea_brush_paints_exactly_the_tiles_under_it() {
        let mut w = flat();
        let land = w.land_tiles();
        let r = w.apply_stroke(&stroke(Brush::Ocean, 0.5, &[[10.5, 5.5]]));
        assert_eq!(r.tiles, 1);
        assert_eq!(w.water[w.idx(10, 5)], Water::Ocean);
        assert_eq!(w.land_tiles(), land - 1);
        assert_eq!(r.flooded, vec![w.idx(10, 5)]);
    }

    #[test]
    fn lake_brush_makes_a_lake_and_land_brush_removes_it() {
        let mut w = flat();
        w.apply_stroke(&stroke(Brush::Lake, 1.5, &[[20.0, 12.0]]));
        let lakes = (0..w.w * w.h)
            .filter(|&i| w.water[i] == Water::Lake)
            .count();
        assert!(lakes >= 4, "{lakes} lake tiles");
        assert!(w.water_access[w.idx(19, 11)] > 0.9);
        w.apply_stroke(&stroke(Brush::Land, 3.0, &[[20.0, 12.0]]));
        assert!(!w.water.contains(&Water::Lake));
    }

    #[test]
    fn painted_climate_survives_later_terrain_edits() {
        let mut w = flat();
        let i = w.idx(30, 12);
        let (t0, p0) = (w.temp_base[i], w.precip[i]);
        w.apply_stroke(&stroke(Brush::Colder, 0.5, &[[30.5, 12.5]]));
        w.apply_stroke(&stroke(Brush::Wetter, 0.5, &[[30.5, 12.5]]));
        assert!((w.temp_base[i] - (t0 - 10.0)).abs() < 0.01);
        assert!((w.precip[i] - (p0 + 1000.0)).abs() < 0.5);
        // A sea painted far away leaves the painted offsets in place.
        w.apply_stroke(&stroke(Brush::Ocean, 0.5, &[[2.5, 2.5]]));
        assert!((w.temp_base[i] - (t0 - 10.0)).abs() < 1.0);
        assert!(w.precip[i] > p0 + 900.0);
    }

    #[test]
    fn brush_wraps_east_west() {
        let mut w = flat();
        let r = w.apply_stroke(&stroke(Brush::Ocean, 1.0, &[[0.0, 10.5]]));
        assert_eq!(w.water[w.idx(47, 10)], Water::Ocean);
        assert_eq!(w.water[w.idx(0, 10)], Water::Ocean);
        assert_eq!(r.tiles, 2);
    }

    #[test]
    fn rejects_bad_strokes() {
        let w = flat();
        let ok = stroke(Brush::Raise, 2.0, &[[1.0, 1.0]]);
        assert!(ok.validate(w.w, w.h).is_ok());
        let bad = |f: &dyn Fn(&mut Stroke)| {
            let mut s = ok.clone();
            f(&mut s);
            s.validate(w.w, w.h).unwrap_err()
        };
        assert!(bad(&|s| s.radius = 0.0).contains("radius"));
        assert!(bad(&|s| s.strength = 2.0).contains("strength"));
        assert!(bad(&|s| s.points.clear()).contains("points"));
        assert!(bad(&|s| s.points[0] = [f32::NAN, 0.0]).contains("outside"));
        assert!(bad(&|s| s.points[0] = [0.0, 99.0]).contains("outside"));
    }
}
