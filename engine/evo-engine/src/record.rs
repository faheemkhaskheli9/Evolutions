//! Metric time series and the event chronicle (PRD FR-O4, FR-O9).

use crate::traits::N_TRAITS;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub year: f64,
    pub n: u32,
    /// Mean phenotype per trait.
    pub mean: Vec<f32>,
    /// Phenotypic SD per trait.
    pub sd: Vec<f32>,
    /// Mean neutral-locus heterozygosity.
    pub het: f32,
}

/// Per-species series, decimated (every other point dropped) when it grows past `cap`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub series: BTreeMap<u32, Vec<Sample>>,
}

pub const SERIES_CAP: usize = 1500;

impl Metrics {
    pub fn push(&mut self, species: u32, s: Sample) {
        let v = self.series.entry(species).or_default();
        v.push(s);
        if v.len() > SERIES_CAP {
            let kept: Vec<Sample> = v
                .drain(..)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .map(|(_, s)| s)
                .collect();
            *v = kept;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    Info,
    Founders,
    Speciation,
    Extinction,
    Mode,
    Intervention,
    Crash,
    Adaptation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub year: f64,
    pub kind: EventKind,
    pub species: Option<u32>,
    pub text: String,
}

/// Running statistics accumulator for a phenotype vector.
#[derive(Clone)]
pub struct TraitStats {
    pub n: u32,
    sum: [f64; N_TRAITS],
    sq: [f64; N_TRAITS],
    pub het: f64,
}

impl Default for TraitStats {
    fn default() -> Self {
        TraitStats {
            n: 0,
            sum: [0.0; N_TRAITS],
            sq: [0.0; N_TRAITS],
            het: 0.0,
        }
    }
}

impl TraitStats {
    pub fn add(&mut self, p: &[f32; N_TRAITS], het: f32, weight: u32) {
        self.n += weight;
        for i in 0..N_TRAITS {
            let x = p[i] as f64;
            self.sum[i] += x * weight as f64;
            self.sq[i] += x * x * weight as f64;
        }
        self.het += het as f64 * weight as f64;
    }

    pub fn mean(&self) -> [f32; N_TRAITS] {
        let mut m = [0f32; N_TRAITS];
        let n = self.n.max(1) as f64;
        for (i, v) in m.iter_mut().enumerate() {
            *v = (self.sum[i] / n) as f32;
        }
        m
    }

    pub fn sd(&self) -> [f32; N_TRAITS] {
        let mut s = [0f32; N_TRAITS];
        let n = self.n.max(1) as f64;
        for (i, v) in s.iter_mut().enumerate() {
            let m = self.sum[i] / n;
            *v = ((self.sq[i] / n - m * m).max(0.0)).sqrt() as f32;
        }
        s
    }

    pub fn sample(&self, year: f64) -> Sample {
        Sample {
            year,
            n: self.n,
            mean: self.mean().to_vec(),
            sd: self.sd().to_vec(),
            het: (self.het / self.n.max(1) as f64) as f32,
        }
    }
}
