//! Terrain generation (TRD TR-W2): plate-like Voronoi ranges plus fractal noise,
//! sea level chosen to hit the target land fraction.

use super::{LandShape, WorldGenParams};
use crate::noise::{fbm_cyl, ridged_cyl};
use crate::rng::{stream, Rng};

struct Plate {
    x: f64,
    y: f64,
    continental: bool,
    vx: f64,
    vy: f64,
}

/// Torus-on-x distance in normalized coordinates (x scaled by aspect).
fn dist(ax: f64, ay: f64, bx: f64, by: f64, aspect: f64) -> (f64, f64, f64) {
    let mut dx = bx - ax;
    if dx > 0.5 {
        dx -= 1.0;
    } else if dx < -0.5 {
        dx += 1.0;
    }
    let dx = dx * aspect;
    let dy = by - ay;
    ((dx * dx + dy * dy).sqrt(), dx, dy)
}

pub fn generate(gen: &WorldGenParams, seed: u64) -> Vec<f32> {
    let (w, h) = (gen.width, gen.height);
    let aspect = w as f64 / h as f64;
    let mut rng = Rng::keyed(seed, 0, 0, stream::WORLD_GEN);
    let s1 = rng.next_u64();
    let s2 = rng.next_u64();
    let s3 = rng.next_u64();
    let rough = gen.roughness.clamp(0.0, 1.0);

    let plates: Vec<Plate> = (0..gen.plates.max(2))
        .map(|_| {
            let a = rng.f64() * std::f64::consts::TAU;
            Plate {
                x: rng.f64(),
                y: 0.1 + 0.8 * rng.f64(),
                continental: rng.chance((gen.land_fraction * 1.3).min(0.9)),
                vx: a.cos(),
                vy: a.sin(),
            }
        })
        .collect();

    let mut score = vec![0f64; w * h];
    for y in 0..h {
        for x in 0..w {
            let fx = (x as f64 + 0.5) / w as f64;
            let fy = (y as f64 + 0.5) / h as f64;
            let detail = fbm_cyl(s1, fx, fy, 6.0, 6, aspect);
            let ridges = ridged_cyl(s2, fx, fy, 4.0, 5, aspect);
            let s = match gen.shape {
                LandShape::Plates => {
                    // Warp the sample point so plate edges are not straight lines.
                    let wx = fx + 0.04 * fbm_cyl(s3, fx, fy, 3.0, 3, aspect);
                    let wy = fy + 0.04 * fbm_cyl(s3 ^ 1, fx, fy, 3.0, 3, aspect);
                    let mut best = (f64::MAX, 0usize);
                    let mut second = (f64::MAX, 0usize);
                    for (i, p) in plates.iter().enumerate() {
                        let (d, _, _) = dist(wx, wy, p.x, p.y, aspect);
                        if d < best.0 {
                            second = best;
                            best = (d, i);
                        } else if d < second.0 {
                            second = (d, i);
                        }
                    }
                    let (pa, pb) = (&plates[best.1], &plates[second.1]);
                    let boundary = second.0 - best.0;
                    let (_, nx, ny) = dist(pa.x, pa.y, pb.x, pb.y, aspect);
                    let nl = (nx * nx + ny * ny).sqrt().max(1e-9);
                    let conv = ((pa.vx - pb.vx) * nx + (pa.vy - pb.vy) * ny) / nl;
                    let edge = (-(boundary / 0.035).powi(2)).exp();
                    let base = if pa.continental { 0.55 } else { 0.15 };
                    let uplift = if conv > 0.0 {
                        0.45 * conv * edge * (0.6 + 0.8 * ridges)
                    } else {
                        0.12 * conv * edge
                    };
                    base + uplift + (0.15 + 0.2 * rough) * detail
                }
                LandShape::Archipelago => {
                    let islands = fbm_cyl(s1, fx, fy, 14.0, 5, aspect);
                    islands + 0.25 * ridges + 0.1 * detail
                }
                LandShape::Supercontinent => {
                    let (d, _, _) = dist(fx, fy, 0.5, 0.5, aspect * 0.55);
                    1.0 - 1.6 * d + 0.35 * detail + 0.25 * ridges
                }
                LandShape::Flat => 1.0 + 0.02 * detail,
                LandShape::Rift => {
                    let dx = (fx - 0.5).abs();
                    let body = 1.0 - 3.2 * dx + 0.25 * detail;
                    let rift = 0.35 * (-(dx / 0.05).powi(2)).exp() * (0.6 + 0.6 * ridges);
                    body + rift
                }
                LandShape::MainlandAndIslands => {
                    let main = 1.0 - 3.0 * (fx - 0.22).abs();
                    let islands = 0.55 * fbm_cyl(s1 ^ 7, fx, fy, 16.0, 4, aspect)
                        - 0.15 * ((fx - 0.72).abs() * 4.0);
                    main.max(islands + 0.25) + 0.2 * detail
                }
            };
            score[y * w + x] = s;
        }
    }

    if gen.shape == LandShape::Flat {
        return score
            .iter()
            .map(|s| (200.0 + 20.0 * (s - 1.0)) as f32)
            .collect();
    }

    // Sea level at the quantile that yields the requested land fraction.
    let mut sorted = score.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = ((1.0 - gen.land_fraction.clamp(0.02, 1.0)) * (sorted.len() - 1) as f64) as usize;
    let sea = sorted[q];
    let max = *sorted.last().unwrap();
    let min = sorted[0];
    score
        .iter()
        .map(|&s| {
            if s >= sea && gen.land_fraction >= 0.999 {
                (5.0 + (s - min) / (max - min).max(1e-9) * gen.mountain_height_m) as f32
            } else if s >= sea {
                let t = (s - sea) / (max - sea).max(1e-9);
                (2.0 + t.powf(1.6) * gen.mountain_height_m) as f32
            } else {
                let t = (sea - s) / (sea - min).max(1e-9);
                (-50.0 - t * 4000.0) as f32
            }
        })
        .collect()
}
