//! Deterministic, counter-keyed random numbers (TRD TR-D3).
//!
//! Every random stream is derived from `(world_seed, tick, entity, stream)`, so
//! the numbers an entity sees do not depend on processing order or on how many
//! draws other systems made.

use serde::{Deserialize, Serialize};

/// Named purposes for random streams. Adding a stream never shifts the others.
pub mod stream {
    pub const WORLD_GEN: u64 = 1;
    pub const FOUNDERS: u64 = 2;
    pub const DECISION: u64 = 3;
    pub const PERCEPTION: u64 = 4;
    pub const INTERACTION: u64 = 5;
    pub const MEIOSIS: u64 = 6;
    pub const MUTATION: u64 = 7;
    pub const MORTALITY: u64 = 8;
    pub const BIRTH: u64 = 9;
    pub const POPULATION: u64 = 10;
    pub const TRANSITION: u64 = 11;
    pub const NAMING: u64 = 12;
    pub const EVENTS: u64 = 13;
    pub const SPECIES: u64 = 14;
    pub const MOVEMENT: u64 = 15;
}

#[inline]
pub fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Hash several words into one well-mixed key.
#[inline]
pub fn hash4(a: u64, b: u64, c: u64, d: u64) -> u64 {
    let mut h = mix64(a ^ 0x9e37_79b9_7f4a_7c15);
    h = mix64(h ^ b.wrapping_mul(0xd6e8_feb8_6659_fd93));
    h = mix64(h ^ c.wrapping_mul(0xa076_1d64_78bd_642f));
    mix64(h ^ d.wrapping_mul(0xe703_7ed1_a0b4_28db))
}

/// Uniform value in [0, 1) from a hash.
#[inline]
pub fn hash_unit(h: u64) -> f64 {
    (h >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// SplitMix64 generator started from a counter key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: mix64(seed) }
    }

    pub fn keyed(seed: u64, tick: u64, entity: u64, stream: u64) -> Self {
        Rng {
            state: hash4(seed, tick, entity, stream),
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix64(self.state)
    }

    #[inline]
    pub fn f64(&mut self) -> f64 {
        hash_unit(self.next_u64())
    }

    #[inline]
    pub fn f32(&mut self) -> f32 {
        self.f64() as f32
    }

    #[inline]
    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    /// Uniform integer in [0, n).
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }

    #[inline]
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Standard normal via Box–Muller.
    pub fn normal(&mut self) -> f64 {
        let u1 = self.f64().max(1e-300);
        let u2 = self.f64();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Standard normal via Marsaglia's polar method: no trigonometry, so cheaper than
    /// `normal` where many draws are needed (binomial and Poisson approximations).
    fn normal_polar(&mut self) -> f64 {
        loop {
            let u = 2.0 * self.f64() - 1.0;
            let v = 2.0 * self.f64() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return u * (-2.0 * s.ln() / s).sqrt();
            }
        }
    }

    pub fn normal_f32(&mut self, mean: f32, sd: f32) -> f32 {
        mean + sd * self.normal() as f32
    }

    /// Poisson-distributed count.
    pub fn poisson(&mut self, lambda: f64) -> u64 {
        if lambda <= 0.0 {
            return 0;
        }
        if lambda < 30.0 {
            // Inversion: one uniform draw, walking up the cumulative distribution.
            let mut f = (-lambda).exp();
            let mut u = self.f64();
            let mut k = 0u64;
            while u >= f {
                u -= f;
                k += 1;
                f *= lambda / k as f64;
                if f <= 0.0 {
                    break;
                }
            }
            return k;
        }
        // Normal approximation with continuity correction.
        let x = lambda + lambda.sqrt() * self.normal_polar() + 0.5;
        x.max(0.0).floor() as u64
    }

    /// Binomial(n, p). Exact (by inversion, one uniform draw) for n ≤ 64; Poisson
    /// approximation for small expected counts; normal approximation otherwise.
    pub fn binomial(&mut self, n: u64, p: f64) -> u64 {
        if n == 0 || p <= 0.0 {
            return 0;
        }
        if p >= 1.0 {
            return n;
        }
        if p > 0.5 {
            return n - self.binomial(n, 1.0 - p);
        }
        let mean = n as f64 * p;
        if n <= 64 {
            // f = P(X = k), from P(X = 0) = qⁿ via f(k) = f(k-1)·(n-k+1)/k·p/q.
            let q = 1.0 - p;
            let s = p / q;
            let a = (n + 1) as f64 * s;
            let mut f = q.powi(n as i32);
            let mut u = self.f64();
            let mut k = 0u64;
            while u >= f && k < n {
                u -= f;
                k += 1;
                f *= a / k as f64 - s;
            }
            return k;
        }
        if mean < 12.0 {
            // Poisson approximation (p ≤ 0.5 and small expected count).
            return self.poisson(mean).min(n);
        }
        let sd = (mean * (1.0 - p)).sqrt();
        let x = (mean + sd * self.normal_polar() + 0.5).floor();
        x.clamp(0.0, n as f64) as u64
    }

    /// Index chosen with probability proportional to `weights`.
    pub fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return self.below(weights.len());
        }
        let mut r = self.f64() * total;
        for (i, w) in weights.iter().enumerate() {
            if r < *w {
                return i;
            }
            r -= *w;
        }
        weights.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_streams_are_reproducible_and_distinct() {
        let a: Vec<u64> = (0..4).map(|_| Rng::keyed(1, 2, 3, 4).next_u64()).collect();
        assert!(a.iter().all(|x| *x == a[0]));
        assert_ne!(
            Rng::keyed(1, 2, 3, 4).next_u64(),
            Rng::keyed(1, 2, 3, 5).next_u64()
        );
    }

    #[test]
    fn binomial_mean_and_variance() {
        let mut r = Rng::new(7);
        for &(n, p) in &[
            (50u64, 0.3f64),
            (10, 0.05),
            (64, 0.5),
            (3, 0.9),
            (1000, 0.01),
            (5000, 0.4),
        ] {
            let k = 20000;
            let xs: Vec<f64> = (0..k).map(|_| r.binomial(n, p) as f64).collect();
            let m = xs.iter().sum::<f64>() / k as f64;
            let v = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / k as f64;
            let em = n as f64 * p;
            let ev = em * (1.0 - p);
            assert!((m - em).abs() < 0.05 * em.max(1.0), "mean {m} vs {em}");
            assert!((v - ev).abs() < 0.1 * ev.max(1.0), "var {v} vs {ev}");
        }
    }

    #[test]
    fn polar_normal_moments() {
        let mut r = Rng::new(9);
        let k = 100_000;
        let xs: Vec<f64> = (0..k).map(|_| r.normal_polar()).collect();
        let m = xs.iter().sum::<f64>() / k as f64;
        let v = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / k as f64;
        let tail = xs.iter().filter(|x| x.abs() > 1.96).count() as f64 / k as f64;
        assert!(m.abs() < 0.02 && (v - 1.0).abs() < 0.02, "{m} {v}");
        assert!((tail - 0.05).abs() < 0.004, "{tail}");
    }

    #[test]
    fn poisson_mean() {
        let mut r = Rng::new(3);
        for &l in &[0.5f64, 4.0, 25.0, 80.0] {
            let k = 20000;
            let m = (0..k).map(|_| r.poisson(l) as f64).sum::<f64>() / k as f64;
            assert!((m - l).abs() < 0.05 * l.max(1.0));
        }
    }

    #[test]
    fn small_binomial_matches_pmf() {
        let mut r = Rng::new(11);
        let (n, p) = (12u64, 0.2f64);
        let draws = 200_000;
        let mut counts = [0u32; 13];
        for _ in 0..draws {
            counts[r.binomial(n, p) as usize] += 1;
        }
        let mut pmf = (1.0 - p).powi(n as i32);
        for (k, &c) in counts.iter().enumerate() {
            let got = c as f64 / draws as f64;
            assert!((got - pmf).abs() < 0.004, "P(X={k}) {got} vs {pmf}");
            pmf *= (n - k as u64) as f64 / (k + 1) as f64 * p / (1.0 - p);
        }
    }
}
