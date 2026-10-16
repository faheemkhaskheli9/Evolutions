//! Hash-based value noise. The world wraps east–west, so 2D maps are sampled on
//! a cylinder in 3D to avoid a seam.

use crate::rng::{hash4, hash_unit};

#[inline]
fn smooth(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lattice(seed: u64, x: i64, y: i64, z: i64) -> f64 {
    hash_unit(hash4(seed, x as u64, y as u64, z as u64)) * 2.0 - 1.0
}

/// 3D value noise in [-1, 1].
pub fn value3(seed: u64, x: f64, y: f64, z: f64) -> f64 {
    let (xi, yi, zi) = (x.floor() as i64, y.floor() as i64, z.floor() as i64);
    let (xf, yf, zf) = (x - xi as f64, y - yi as f64, z - zi as f64);
    let (u, v, w) = (smooth(xf), smooth(yf), smooth(zf));
    let mut acc = [0.0; 8];
    for (i, a) in acc.iter_mut().enumerate() {
        let dx = (i & 1) as i64;
        let dy = ((i >> 1) & 1) as i64;
        let dz = ((i >> 2) & 1) as i64;
        *a = lattice(seed, xi + dx, yi + dy, zi + dz);
    }
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(acc[0], acc[1], u);
    let x10 = lerp(acc[2], acc[3], u);
    let x01 = lerp(acc[4], acc[5], u);
    let x11 = lerp(acc[6], acc[7], u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w)
}

/// Fractal Brownian motion on a cylinder. `fx` is in [0,1) around the world,
/// `fy` in [0,1] pole to pole. `freq` is the base number of features around.
pub fn fbm_cyl(seed: u64, fx: f64, fy: f64, freq: f64, octaves: u32, aspect: f64) -> f64 {
    let ang = fx * std::f64::consts::TAU;
    let r = freq / std::f64::consts::TAU;
    let (cx, cz) = (ang.cos() * r, ang.sin() * r);
    let cy = fy * freq / aspect;
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut f = 1.0;
    for o in 0..octaves {
        sum += amp * value3(seed.wrapping_add(o as u64 * 7919), cx * f, cy * f, cz * f);
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

/// Ridged multifractal noise in [0,1] (sharp crests, for mountain ranges).
pub fn ridged_cyl(seed: u64, fx: f64, fy: f64, freq: f64, octaves: u32, aspect: f64) -> f64 {
    let ang = fx * std::f64::consts::TAU;
    let r = freq / std::f64::consts::TAU;
    let (cx, cz) = (ang.cos() * r, ang.sin() * r);
    let cy = fy * freq / aspect;
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut f = 1.0;
    for o in 0..octaves {
        let n = 1.0
            - value3(
                seed.wrapping_add(o as u64 * 104_729),
                cx * f,
                cy * f,
                cz * f,
            )
            .abs();
        sum += amp * n * n;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}
