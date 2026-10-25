//! Data export (PRD FR-O11): per-species time series and the Chronicle as CSV, and
//! the world's settings as JSON. The CLI and the browser app both write these, so the
//! files are identical whichever produced them.

use crate::record::EventKind;
use crate::sim::{Simulation, MODEL_VERSION};
use crate::templates;
use crate::traits::Trait;
use serde_json::{json, Value};

/// Quotes a CSV field when it contains a comma, quote or line break (RFC 4180).
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl Simulation {
    /// File name for a species' time series, e.g. `species_3_Ovis_borealis.csv`.
    pub fn species_csv_name(&self, species: u32) -> String {
        let s = self.species.get(species);
        format!("species_{}_{}.csv", s.id, s.name.replace(' ', "_"))
    }

    /// One species' metrics over time: year, population, mean neutral heterozygosity,
    /// then the mean of each trait, then each trait's SD (`<trait>_sd`).
    pub fn species_csv(&self, species: u32) -> String {
        let mut out = String::from("year,n,het");
        for t in Trait::ALL {
            out.push_str(&format!(",{}", t.info().key));
        }
        for t in Trait::ALL {
            out.push_str(&format!(",{}_sd", t.info().key));
        }
        out.push('\n');
        for smp in self
            .metrics
            .series
            .get(&species)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
        {
            out.push_str(&format!("{:.4},{},{:.4}", smp.year, smp.n, smp.het));
            for m in &smp.mean {
                out.push_str(&format!(",{m:.5}"));
            }
            for s in &smp.sd {
                out.push_str(&format!(",{s:.5}"));
            }
            out.push('\n');
        }
        out
    }

    /// One species' population-genetics measures over time (PRD FR-O7): year, expected
    /// heterozygosity of the species (`ht`) and within patches (`hs`), `fst`, Nₑ from the
    /// adult sex ratio (`ne`) and from the loss of heterozygosity (`ne_h`, empty when it
    /// cannot be estimated), the generation time used for it, and segregating alleles.
    pub fn popgen_csv(&self, species: u32) -> String {
        let mut out = String::from("year,ht,hs,fst,ne,ne_h,generation_years,segregating_alleles\n");
        for g in self
            .popgen
            .series
            .get(&species)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
        {
            let ne_h = if g.ne_h >= 0.0 {
                format!("{:.1}", g.ne_h)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "{:.4},{:.4},{:.4},{:.4},{:.1},{},{:.2},{}\n",
                g.year, g.ht, g.hs, g.fst, g.ne, ne_h, g.generation, g.segregating
            ));
        }
        out
    }

    /// Every species' time series as (file name, contents), in species order: the
    /// metrics file, then the population-genetics file (`species_<id>_<name>_popgen.csv`).
    pub fn species_csv_files(&self) -> Vec<(String, String)> {
        self.species
            .list
            .iter()
            .flat_map(|s| {
                let name = self.species_csv_name(s.id);
                let popgen = name.replace(".csv", "_popgen.csv");
                [
                    (name, self.species_csv(s.id)),
                    (popgen, self.popgen_csv(s.id)),
                ]
            })
            .collect()
    }

    /// The Chronicle: year, kind, species id (empty for world events), species name, text.
    pub fn chronicle_csv(&self) -> String {
        let mut out = String::from("year,kind,species,species_name,text\n");
        for e in &self.events {
            let kind = match e.kind {
                EventKind::Info => "Info",
                EventKind::Founders => "Founders",
                EventKind::Speciation => "Speciation",
                EventKind::Extinction => "Extinction",
                EventKind::Mode => "Mode",
                EventKind::Intervention => "Intervention",
                EventKind::Crash => "Crash",
                EventKind::Adaptation => "Adaptation",
            };
            let (id, name) = match e.species {
                Some(id) if (id as usize) < self.species.list.len() => {
                    (id.to_string(), self.species.get(id).name.clone())
                }
                Some(id) => (id.to_string(), String::new()),
                None => (String::new(), String::new()),
            };
            out.push_str(&format!(
                "{:.4},{},{},{},{}\n",
                e.year,
                kind,
                id,
                csv_field(&name),
                csv_field(&e.text)
            ));
        }
        out
    }

    /// The settings the world was created from: template, seed, realism, generator
    /// settings and founders. `founders` is null for worlds saved before replay logs
    /// existed, which did not record them.
    pub fn settings_json(&self) -> Value {
        let origin = self.replay.origin.as_ref();
        json!({
            "format": "evolutions-settings",
            "model_version": MODEL_VERSION,
            "template": self.template_key,
            "template_name": self.template_name,
            "seed": self.seed,
            "realism": self.params.realism,
            "gen": self.world.gen,
            "founders": origin.map(|o| match &o.founders {
                Some(f) => json!(f),
                None => {
                    let tmpl = if o.template == "random" {
                        Some(templates::random(o.seed))
                    } else {
                        templates::find(&o.template)
                    };
                    tmpl.map(|t| json!(t.founders)).unwrap_or(Value::Null)
                }
            }),
            "year": self.year(),
        })
    }
}
