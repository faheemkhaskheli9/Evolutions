//! Species designer (PRD FR-S2, FR-S3): a founder species defined by its trait
//! targets, and the biological checks a design must pass.
//!
//! A design is turned into a starting genome the same way a library species is
//! (`species_lib::founder_spec`), so a design made of a library species' values
//! behaves exactly like that species.
//!
//! The checks use the engine's own physiology (`physiology::derive`,
//! `population::behavioural_thermal`), so what the designer reports is what the
//! simulation will do:
//!
//! - **Energy budget**: field metabolism (2.5 × resting, Nagy 1987; resting includes
//!   the maintenance cost of brain, insulation and other costly traits) plus breeding
//!   must fit in what the gut can process (`max_intake_kj_day`).
//! - **Breeding**: growing a litter during gestation must fit in the energy left
//!   over, and an animal must raise enough young in its lifetime to replace itself.
//! - **Brain cost** (Aiello & Wheeler 1995): the share of resting energy the brain uses.
//! - **Thermal limits**: the air temperatures it can live in without heat or cold
//!   stress, and that its energy budget can pay to keep warm in.
//! - **Diet** (Jarman–Bell): small plant-eaters cannot use coarse forage.
//!
//! Errors make a design impossible (it is rejected); warnings and notes are shown.

use crate::physiology::{derive, roughage_quality, Derived, TISSUE_KJ_PER_KG};
use crate::species_lib;
use crate::traits::{Phenotype, Trait, Transform, N_TRAITS};
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most designs a world or template may hold.
pub const MAX_DESIGNS: usize = 32;
/// Field metabolic rate as a multiple of resting metabolism (Nagy 1987: 2–4×).
pub const FMR_FACTOR: f32 = 2.5;
/// Energy margin below which a design is warned that it will likely starve.
pub const LEAN_MARGIN: f32 = 1.1;

fn one() -> f32 {
    1.0
}

/// A user-designed founder species.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeciesDesign {
    /// Identifier used by founder groups; must not be a library species' key.
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub color: [u8; 3],
    /// Adult phenotype means by trait key (see the catalog). Traits left out take the
    /// library's defaults.
    pub traits: BTreeMap<String, f32>,
    /// Standing variation among founders relative to a library species' (1 = the same).
    #[serde(default = "one")]
    pub variation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Level {
    /// The design is impossible and is rejected.
    Error,
    /// Possible, but it will likely struggle.
    Warning,
    /// Worth knowing.
    Note,
}

#[derive(Clone, Debug, Serialize)]
pub struct Problem {
    pub level: Level,
    /// Which check: "design", "trait", "energy", "breeding", "brain", "thermal", "diet", "world".
    pub check: &'static str,
    /// The trait concerned, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_key: Option<&'static str>,
    pub message: String,
}

/// Values derived from a design, shown in the designer's preview.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Preview {
    pub mass_kg: f32,
    /// Resting metabolic rate, W, and the share of it the brain uses.
    pub bmr_w: f32,
    pub brain_share: f32,
    /// Daily energy: field metabolism, breeding (averaged over the year) and the most
    /// the gut can process, kJ/day.
    pub fmr_kj_day: f32,
    pub breeding_kj_day: f32,
    pub intake_kj_day: f32,
    /// intake / (field metabolism + breeding); below 1 the budget cannot balance.
    pub energy_margin: f32,
    pub top_speed_kmh: f32,
    pub cruise_kmh: f32,
    /// Water lost per day at rest, litres.
    pub water_l_day: f32,
    pub gestation_days: f32,
    pub weaning_days: f32,
    pub maturity_days: f32,
    /// Median age at death from ageing alone, years.
    pub lifespan_years: f32,
    pub newborn_kg: f32,
    pub litters_per_year: f32,
    /// Young born over a lifetime of breeding.
    pub lifetime_young: f32,
    /// Air temperatures it can live in without heat or cold stress, °C. The search
    /// spans −80 to +70 °C, so −80 means "−80 or colder".
    pub t_min_c: f32,
    pub t_max_c: f32,
    pub plant_efficiency: f32,
    pub meat_efficiency: f32,
    /// Fraction of a plant-eater's efficiency kept on coarse forage (Jarman–Bell).
    pub roughage_use: f32,
    pub perception_km: f32,
    /// Share of this world's land within its temperature limits all year, when checked
    /// against a world.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub habitable_land: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignReport {
    /// True when there are no errors.
    pub ok: bool,
    pub problems: Vec<Problem>,
    pub preview: Preview,
}

impl DesignReport {
    /// The first error, as a sentence.
    pub fn error(&self) -> Option<String> {
        self.problems
            .iter()
            .find(|p| p.level == Level::Error)
            .map(|p| p.message.clone())
    }
}

fn trait_by_key(key: &str) -> Option<Trait> {
    Trait::ALL.into_iter().find(|t| t.info().key == key)
}

fn key_ok(key: &str) -> bool {
    (1..=40).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

impl SpeciesDesign {
    /// A design with the values of the library species `key`.
    pub fn from_library(key: &str) -> Option<SpeciesDesign> {
        let s = species_lib::find(key)?;
        let means = s.phenotype_means();
        Some(SpeciesDesign {
            key: format!("my-{key}"),
            name: s.name.to_string(),
            description: s.description.to_string(),
            color: s.color,
            traits: Trait::ALL
                .iter()
                .map(|t| (t.info().key.to_string(), means[*t as usize]))
                .collect(),
            variation: 1.0,
        })
    }

    /// Adult phenotype means (unchecked; `report` checks them).
    pub fn means(&self) -> [f32; N_TRAITS] {
        let mut v = species_lib::default_means();
        for (k, x) in &self.traits {
            if let Some(t) = trait_by_key(k) {
                v[t as usize] = *x;
            }
        }
        v
    }

    /// Checks the design without a world.
    pub fn validate(&self) -> Result<(), String> {
        match self.report(None).error() {
            Some(e) => Err(format!("species '{}': {e}", self.name)),
            None => Ok(()),
        }
    }

    /// Runs every check and works out the preview values. With a world, also says how
    /// much of it the species could live in.
    pub fn report(&self, world: Option<&World>) -> DesignReport {
        let mut problems = vec![];
        let mut err = |check, trait_key, message: String| {
            problems.push(Problem {
                level: Level::Error,
                check,
                trait_key,
                message,
            })
        };
        if !key_ok(&self.key) {
            err(
                "design",
                None,
                "the key must be 1–40 lowercase letters, digits, '-' or '_'".into(),
            );
        } else if species_lib::find(&self.key).is_some() {
            err(
                "design",
                None,
                format!("'{}' is a library species' key", self.key),
            );
        }
        let name_len = self.name.trim().chars().count();
        if name_len == 0 || name_len > 60 {
            err("design", None, "the name must be 1–60 characters".into());
        }
        if self.description.chars().count() > 500 {
            err(
                "design",
                None,
                "the description must be at most 500 characters".into(),
            );
        }
        if !(self.variation.is_finite() && (0.1..=3.0).contains(&self.variation)) {
            err(
                "design",
                None,
                "founder variation must be between 0.1 and 3".into(),
            );
        }
        for (k, x) in &self.traits {
            match trait_by_key(k) {
                None => err("trait", None, format!("unknown trait '{k}'")),
                Some(t) => {
                    let i = t.info();
                    // Bounded traits must stay strictly inside 0–1: the genome cannot reach the ends.
                    let (lo, hi) = match i.transform {
                        Transform::Exp => (i.lo, i.hi),
                        Transform::Logistic => (0.01, 0.99),
                    };
                    if !(x.is_finite() && *x >= lo && *x <= hi) {
                        err(
                            "trait",
                            Some(i.key),
                            format!("{} must be between {lo} and {hi} {}", i.label, i.unit),
                        );
                    }
                }
            }
        }
        if !problems.is_empty() {
            return DesignReport {
                ok: false,
                problems,
                preview: Preview::default(),
            };
        }
        let means = self.means();
        let (preview, mut found) = check(&Phenotype(means), world);
        problems.append(&mut found);
        DesignReport {
            ok: !problems.iter().any(|p| p.level == Level::Error),
            problems,
            preview,
        }
    }
}

/// Breeding energy per year, kJ, and litters per year: the same as Population mode
/// (`population::annual`).
fn breeding(p: &Phenotype, d: &Derived) -> (f32, f32) {
    let cycle = d.gestation_days + d.weaning_days;
    let litters = if cycle < 300.0 {
        (90.0 / cycle).clamp(1.0, 2.5)
    } else {
        365.0 / cycle
    };
    let litter = p.get(Trait::LitterSize);
    let kj = litters * litter * d.neonate_kg * TISSUE_KJ_PER_KG / 0.5
        + litters * 0.5 * d.bmr_w * 86.4 * d.weaning_days;
    (kj, litters)
}

/// Coldest and hottest air temperatures without cold or heat stress (with the same
/// sheltering and shade-seeking as the simulation), °C. The coldest is also limited
/// by energy: metabolism plus keeping warm must fit in `budget_w`.
fn thermal_range(d: &Derived, budget_w: f32) -> (f32, f32) {
    let ok_cold = |ta: f32| {
        let (heat, th) = crate::population::behavioural_thermal(d, ta);
        th.cold_stress <= 0.0 && heat + th.extra_w <= budget_w
    };
    let ok_hot = |ta: f32| {
        let (_, th) = crate::population::behavioural_thermal(d, ta);
        th.heat_stress <= 0.0
    };
    let mut t_min = f32::NAN;
    let mut t_max = f32::NAN;
    // 0.5 °C steps from −80 to +70 °C.
    for k in 0..=300 {
        let ta = -80.0 + 0.5 * k as f32;
        if t_min.is_nan() && ok_cold(ta) && ok_hot(ta) {
            t_min = ta;
        }
        if ok_cold(ta) && ok_hot(ta) {
            t_max = ta;
        }
    }
    (t_min, t_max)
}

fn round(x: f32, digits: i32) -> f32 {
    let f = 10f32.powi(digits);
    (x * f).round() / f
}

fn check(p: &Phenotype, world: Option<&World>) -> (Preview, Vec<Problem>) {
    let mass = p.get(Trait::BodyMass);
    let d = derive(p, mass);
    let mut out = vec![];
    let mut add = |level, check, trait_key: Option<Trait>, message: String| {
        out.push(Problem {
            level,
            check,
            trait_key: trait_key.map(|t| t.info().key),
            message,
        })
    };
    use Level::*;

    // Energy budget.
    let bmr_kj = d.bmr_w * 86.4;
    let fmr = FMR_FACTOR * bmr_kj;
    let (breed_kj, litters) = breeding(p, &d);
    let breed_day = breed_kj / 365.25;
    let intake = d.max_intake_kj_day;
    let margin = intake / (fmr + breed_day);
    let brain_w =
        crate::physiology::kleiber_w(mass) * 0.045 * (p.get(Trait::Brain) - 1.0).max(-0.8);
    let brain_share = (brain_w / d.bmr_w).max(0.0);
    if margin < 1.0 {
        add(
            Error,
            "energy",
            None,
            format!(
                "Impossible energy budget: it needs about {:.0} kJ a day (field metabolism {:.0} + breeding {:.0}) \
                 but its gut can process at most {:.0} kJ. Lower its brain size, somatic maintenance, \
                 insulation or other costly traits, or its litter size.",
                fmr + breed_day,
                fmr,
                breed_day,
                intake
            ),
        );
    } else if margin < LEAN_MARGIN {
        add(
            Warning,
            "energy",
            None,
            format!(
                "Little energy to spare ({:.0}% above its needs): it will likely starve \
                 whenever food is short.",
                100.0 * (margin - 1.0)
            ),
        );
    }

    // Brain cost (Aiello & Wheeler 1995).
    if brain_share > 0.12 {
        add(
            Warning,
            "brain",
            Some(Trait::Brain),
            format!(
                "Its brain uses {:.0}% of its resting energy: it needs rich food to pay for it.",
                100.0 * brain_share
            ),
        );
    } else if brain_share > 0.05 {
        add(
            Note,
            "brain",
            Some(Trait::Brain),
            format!(
                "Its brain uses {:.0}% of its resting energy.",
                100.0 * brain_share
            ),
        );
    }

    // Breeding: growing a litter during gestation, and replacing itself.
    let litter = p.get(Trait::LitterSize);
    let litter_kg = litter * d.neonate_kg;
    let fetal_kj_day = litter_kg * TISSUE_KJ_PER_KG / 0.5 / d.gestation_days.max(1.0);
    let spare = (intake - fmr).max(0.0);
    if margin >= 1.0 && fetal_kj_day > spare {
        add(
            Error,
            "breeding",
            Some(Trait::LitterSize),
            format!(
                "Gestation too short for its litter: growing {:.1} kg of young in {:.0} days needs {:.0} kJ a day, \
                 more than the {:.0} kJ it has to spare. Give it fewer young or a slower life history.",
                litter_kg, d.gestation_days, fetal_kj_day, spare
            ),
        );
    }
    let typical_gestation = 30.0 * mass.powf(0.25);
    let g_ratio = d.gestation_days / typical_gestation;
    if !(0.5..=2.0).contains(&g_ratio) {
        add(
            Note,
            "breeding",
            Some(Trait::Pace),
            format!(
                "Gestation of {:.0} days is unusually {} for a {} animal (typical about {:.0} days).",
                d.gestation_days,
                if g_ratio < 1.0 { "short" } else { "long" },
                fmt_mass(mass),
                typical_gestation
            ),
        );
    }
    // Median age at death from ageing alone: S(t) = exp(−b/g (e^{gt} − 1)) = ½.
    let (b, g) = (d.gompertz_b, d.gompertz_g);
    let lifespan = (1.0 + g * std::f32::consts::LN_2 / b.max(1e-6)).ln() / g.max(1e-6);
    let first_young = (d.maturity_days + d.gestation_days) / 365.25;
    let breeding_years = (lifespan - first_young).max(0.0);
    let lifetime_young = litters * litter * breeding_years;
    if breeding_years <= 0.0 {
        add(
            Error,
            "breeding",
            Some(Trait::Pace),
            format!(
                "It dies of old age (median {:.1} years) before its first young are born ({:.1} years).",
                lifespan, first_young
            ),
        );
    } else if lifetime_young < 2.0 {
        add(
            Error,
            "breeding",
            Some(Trait::Pace),
            format!(
                "It cannot replace itself: a female has only about {:.1} young in her lifetime.",
                lifetime_young
            ),
        );
    } else if lifetime_young < 4.0 {
        add(
            Warning,
            "breeding",
            Some(Trait::LitterSize),
            format!(
                "Slow to recover: a female has only about {:.1} young in her lifetime, so losses are hard to make up.",
                lifetime_young
            ),
        );
    }

    // Thermal limits: metabolism and keeping warm must fit in what is left after breeding.
    let budget_w = ((intake - breed_day) / 86.4).max(0.0);
    let (t_min, t_max) = thermal_range(&d, budget_w);
    if t_min.is_nan() || t_max - t_min < 1.0 {
        add(
            Error,
            "thermal",
            Some(Trait::Insulation),
            "It cannot keep its body temperature at any air temperature.".into(),
        );
    } else if t_max - t_min < 15.0 {
        add(
            Warning,
            "thermal",
            Some(Trait::Insulation),
            format!(
                "Narrow comfort range ({:.0} to {:.0} °C): seasons will stress it.",
                t_min, t_max
            ),
        );
    }

    // Diet.
    let diet = p.get(Trait::Diet);
    let q = roughage_quality(mass);
    if diet < 0.5 && q < 0.1 {
        add(
            Note,
            "diet",
            Some(Trait::BodyMass),
            format!(
                "Too small to digest coarse forage (Jarman–Bell): as a {} plant-eater it lives on high-quality plants only.",
                fmt_mass(mass)
            ),
        );
    }
    if (0.3..=0.7).contains(&diet) {
        add(
            Note,
            "diet",
            Some(Trait::Diet),
            format!(
                "Mixed diet: it digests plants ({:.0}%) and meat ({:.0}%) less well than a specialist.",
                100.0 * d.plant_eff,
                100.0 * d.meat_eff
            ),
        );
    }

    // The world.
    let habitable = world.and_then(|w| {
        if t_min.is_nan() {
            return Some(0.0);
        }
        let mut land = 0usize;
        let mut ok = 0usize;
        for i in 0..w.w * w.h {
            if !w.is_land(i) {
                continue;
            }
            land += 1;
            let cold = w.temp_day(i, 15.0).min(w.temp_day(i, 196.0));
            let hot = w.temp_day(i, 15.0).max(w.temp_day(i, 196.0));
            if cold - 6.0 >= t_min && hot + 6.0 <= t_max {
                ok += 1;
            }
        }
        (land > 0).then(|| ok as f32 / land as f32)
    });
    if let Some(h) = habitable {
        if h == 0.0 {
            add(
                Warning,
                "world",
                None,
                format!(
                    "No land in this world stays within its limits ({:.0} to {:.0} °C) all year: \
                     it will likely die out here.",
                    t_min, t_max
                ),
            );
        } else if h < 0.1 {
            add(
                Warning,
                "world",
                None,
                format!(
                    "Only {:.0}% of this world's land stays within its limits ({:.0} to {:.0} °C) all year.",
                    100.0 * h,
                    t_min,
                    t_max
                ),
            );
        }
    }

    let preview = Preview {
        mass_kg: mass,
        bmr_w: round(d.bmr_w, 3),
        brain_share: round(brain_share, 3),
        fmr_kj_day: round(fmr, 1),
        breeding_kj_day: round(breed_day, 1),
        intake_kj_day: round(intake, 1),
        energy_margin: round(margin, 3),
        top_speed_kmh: round(d.vmax_kmh, 1),
        cruise_kmh: round(d.cruise_kmh, 1),
        water_l_day: round(d.water_loss_g_day / 1000.0, 3),
        gestation_days: round(d.gestation_days, 1),
        weaning_days: round(d.weaning_days, 1),
        maturity_days: round(d.maturity_days, 0),
        lifespan_years: round(lifespan, 1),
        newborn_kg: round(d.neonate_kg, 4),
        litters_per_year: round(litters, 2),
        lifetime_young: round(lifetime_young, 1),
        t_min_c: if t_min.is_nan() { 0.0 } else { t_min },
        t_max_c: if t_max.is_nan() { 0.0 } else { t_max },
        plant_efficiency: round(d.plant_eff, 3),
        meat_efficiency: round(d.meat_eff, 3),
        roughage_use: round(q, 3),
        perception_km: round(d.perception_km, 2),
        habitable_land: habitable.map(|h| round(h, 3)),
    };
    (preview, out)
}

fn fmt_mass(kg: f32) -> String {
    if kg < 1.0 {
        format!("{:.0} g", kg * 1000.0)
    } else {
        format!("{:.0} kg", kg)
    }
}

/// Checks a list of designs: each passes, keys are unique.
pub fn validate_designs(designs: &[SpeciesDesign]) -> Result<(), String> {
    if designs.len() > MAX_DESIGNS {
        return Err(format!("at most {MAX_DESIGNS} designed species"));
    }
    for (i, d) in designs.iter().enumerate() {
        d.validate()?;
        if designs[..i].iter().any(|e| e.key == d.key) {
            return Err(format!("two designed species share the key '{}'", d.key));
        }
    }
    Ok(())
}

/// A species founders can be released as: a library species or a design.
pub struct FounderSpecies {
    pub name: String,
    pub color: [u8; 3],
    pub means: [f32; N_TRAITS],
    pub variation: f32,
}

impl crate::sim::Simulation {
    /// The library species or design with this key.
    pub fn founder_species(&self, key: &str) -> Option<FounderSpecies> {
        if let Some(s) = species_lib::find(key) {
            return Some(FounderSpecies {
                name: s.name.to_string(),
                color: s.color,
                means: s.phenotype_means(),
                variation: 1.0,
            });
        }
        self.designs
            .iter()
            .find(|d| d.key == key)
            .map(|d| FounderSpecies {
                name: d.name.clone(),
                color: d.color,
                means: d.means(),
                variation: d.variation,
            })
    }

    /// A species' name from its key (the key itself when unknown).
    pub fn species_name(&self, key: &str) -> String {
        self.founder_species(key)
            .map(|s| s.name)
            .unwrap_or_else(|| key.to_string())
    }

    /// Adds a design to the world. Returns false when the world already has the same
    /// design; a different design with the same key is an error.
    pub(crate) fn register_design(&mut self, d: SpeciesDesign) -> Result<bool, String> {
        d.validate()?;
        if let Some(old) = self.designs.iter().find(|e| e.key == d.key) {
            return if *old == d {
                Ok(false)
            } else {
                Err(format!(
                    "this world already has a different species with the key '{}'",
                    d.key
                ))
            };
        }
        if self.designs.len() >= MAX_DESIGNS {
            return Err(format!("at most {MAX_DESIGNS} designed species"));
        }
        self.designs.push(d);
        Ok(true)
    }

    /// Rebuilds `designs` from the replay log after loading a save.
    pub(crate) fn rebuild_designs(&mut self) -> Result<(), String> {
        let mut all: Vec<SpeciesDesign> = self
            .replay
            .origin
            .as_ref()
            .map(|o| o.designs.clone())
            .unwrap_or_default();
        all.extend(self.replay.entries.iter().filter_map(|e| match &e.action {
            crate::replay::Action::Intervention(crate::sim::Intervention::AddFounders {
                design: Some(d),
                ..
            }) => Some(d.clone()),
            _ => None,
        }));
        self.designs.clear();
        for d in all {
            self.register_design(d)?;
        }
        Ok(())
    }

    /// Checks a design against this world.
    pub fn design_report(&self, d: &SpeciesDesign) -> DesignReport {
        let mut r = d.report(Some(&self.world));
        if let Some(old) = self.designs.iter().find(|e| e.key == d.key) {
            if old != d {
                r.ok = false;
                r.problems.insert(
                    0,
                    Problem {
                        level: Level::Error,
                        check: "design",
                        trait_key: None,
                        message: format!(
                            "This world already has a different species with the key '{}': rename it.",
                            d.key
                        ),
                    },
                );
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design(changes: &[(&str, f32)]) -> SpeciesDesign {
        let mut d = SpeciesDesign::from_library("hare").unwrap();
        for (k, v) in changes {
            d.traits.insert(k.to_string(), *v);
        }
        d
    }

    #[test]
    fn every_library_species_passes_its_checks() {
        for s in species_lib::library() {
            let r = SpeciesDesign::from_library(s.key).unwrap().report(None);
            assert!(r.ok, "{}: {:?}", s.key, r.problems);
            assert!(
                !r.problems.iter().any(|p| p.level == Level::Warning),
                "{}: {:?}",
                s.key,
                r.problems
            );
            assert!(r.preview.energy_margin >= LEAN_MARGIN);
        }
    }

    #[test]
    fn an_impossible_energy_budget_is_rejected_with_a_reason() {
        let d = design(&[
            ("brain", 8.0),
            ("maintenance", 0.99),
            ("insulation", 0.99),
            ("ornament", 0.99),
            ("senses", 0.99),
            ("sprint", 0.99),
            ("kidney", 0.99),
            ("heat_tolerance", 0.99),
        ]);
        let r = d.report(None);
        assert!(!r.ok);
        let e = r.error().unwrap();
        assert!(e.starts_with("Impossible energy budget"), "{e}");
        assert!(r.preview.energy_margin < 1.0);
        assert!(d
            .validate()
            .unwrap_err()
            .contains("Impossible energy budget"));
    }

    #[test]
    fn a_slow_giant_with_one_young_cannot_replace_itself() {
        let r = design(&[("body_mass", 5000.0), ("pace", 8.0), ("litter", 1.0)]).report(None);
        assert!(!r.ok);
        assert!(r
            .problems
            .iter()
            .any(|p| p.check == "breeding" && p.level == Level::Error));
    }

    #[test]
    fn insulation_moves_the_comfort_range() {
        let thin = design(&[("insulation", 0.05)]).report(None).preview;
        let thick = design(&[("insulation", 0.95)]).report(None).preview;
        assert!(thick.t_min_c < thin.t_min_c - 10.0, "{thin:?} {thick:?}");
    }

    #[test]
    fn out_of_range_values_and_bad_keys_are_errors() {
        assert!(!design(&[("body_mass", 0.0)]).report(None).ok);
        assert!(!design(&[("diet", 1.0)]).report(None).ok);
        assert!(!design(&[("wings", 1.0)]).report(None).ok);
        let mut d = design(&[]);
        d.key = "hare".into();
        assert!(!d.report(None).ok);
        d.key = "Bad Key".into();
        assert!(!d.report(None).ok);
        let mut d = design(&[]);
        d.variation = 0.0;
        assert!(!d.report(None).ok);
        assert!(validate_designs(&[design(&[]), design(&[])]).is_err());
    }

    #[test]
    fn a_tropical_species_warns_in_a_cold_world() {
        let gen = crate::world::WorldGenParams {
            width: 64,
            height: 32,
            temp_offset: -25.0,
            ..Default::default()
        };
        let w = World::generate(gen, 3);
        let r = design(&[("insulation", 0.02), ("body_mass", 0.3)]).report(Some(&w));
        assert!(
            r.problems.iter().any(|p| p.check == "world"),
            "{:?}",
            r.problems
        );
        assert!(r.preview.habitable_land.unwrap() < 0.1);
    }

    use crate::replay::Replay;
    use crate::sim::{Intervention, NewWorld, Simulation};
    use crate::templates::FounderGroup;
    use crate::user_template::UserTemplate;

    fn group(species: &str, count: u32) -> FounderGroup {
        FounderGroup {
            species: species.into(),
            count,
            diversity: 0.8,
            region: None,
            delay_years: 0.0,
        }
    }

    fn world(
        founders: Vec<FounderGroup>,
        designs: Vec<SpeciesDesign>,
    ) -> Result<Simulation, String> {
        Simulation::new(NewWorld {
            template: "blank".into(),
            seed: 21,
            gen: None,
            founders: Some(founders),
            realism: None,
            label: None,
            edits: vec![],
            designs,
        })
    }

    /// A hare made in the designer from the library's values.
    fn hare_copy() -> SpeciesDesign {
        let mut d = SpeciesDesign::from_library("hare").unwrap();
        d.key = "my-hare".into();
        d
    }

    #[test]
    fn a_design_with_library_values_behaves_like_the_library_species() {
        let mut lib = world(vec![group("hare", 300)], vec![]).unwrap();
        let mut des = world(vec![group("my-hare", 300)], vec![hare_copy()]).unwrap();
        for _ in 0..4 {
            lib.run_steps(24);
            des.run_steps(24);
            assert_eq!(lib.population(), des.population());
        }
        // Population mode too.
        lib.set_mode(crate::sim::Mode::Population).unwrap();
        des.set_mode(crate::sim::Mode::Population).unwrap();
        lib.run_steps(5);
        des.run_steps(5);
        assert_eq!(lib.population(), des.population());
        let (a, b) = (
            lib.species_json().to_string(),
            des.species_json().to_string(),
        );
        assert_eq!(a, b);
        // The states differ only in the world knowing the design.
        des.designs.clear();
        assert_eq!(lib.state_hash(), des.state_hash());
    }

    #[test]
    fn an_impossible_design_is_refused_everywhere() {
        let mut bad = hare_copy();
        bad.traits.insert("brain".into(), 8.0);
        bad.traits.insert("maintenance".into(), 0.99);
        bad.traits.insert("insulation".into(), 0.99);
        bad.traits.insert("ornament".into(), 0.99);
        bad.traits.insert("senses".into(), 0.99);
        bad.traits.insert("sprint".into(), 0.99);
        bad.traits.insert("kidney".into(), 0.99);
        bad.traits.insert("heat_tolerance".into(), 0.99);
        let e = world(vec![group("my-hare", 50)], vec![bad.clone()])
            .err()
            .unwrap();
        assert!(e.contains("Impossible energy budget"), "{e}");
        // Unknown designs are refused too.
        assert!(world(vec![group("my-hare", 50)], vec![]).is_err());
        let mut sim = world(vec![group("hare", 50)], vec![]).unwrap();
        let h = sim.state_hash();
        let r = sim.apply(Intervention::AddFounders {
            group: group("my-hare", 20),
            design: Some(bad),
        });
        assert!(r.unwrap_err().contains("Impossible energy budget"));
        assert!(sim.designs.is_empty());
        assert_eq!(sim.state_hash(), h);
        assert!(sim.replay.entries.is_empty());
    }

    #[test]
    fn designs_survive_saves_replays_and_templates() {
        let mut cold = SpeciesDesign::from_library("hare").unwrap();
        cold.key = "snow-grazer".into();
        cold.name = "Snow grazer".into();
        cold.traits.insert("insulation".into(), 0.85);
        cold.traits.insert("body_mass".into(), 4.0);
        cold.variation = 1.5;
        let mut later = FounderGroup {
            delay_years: 0.5,
            ..group("snow-grazer", 40)
        };
        later.diversity = 0.3;
        let mut sim = world(vec![group("snow-grazer", 200), later], vec![cold.clone()]).unwrap();
        assert_eq!(sim.species.list[0].common_name, "Snow grazer");
        assert!(sim
            .events
            .iter()
            .any(|e| e.text.contains("40 Snow grazer will be released")));
        // A design added later, through an intervention.
        let mut pred = SpeciesDesign::from_library("wolf").unwrap();
        pred.key = "lean-wolf".into();
        pred.name = "Lean wolf".into();
        pred.traits.insert("body_mass".into(), 20.0);
        sim.run_steps(24);
        sim.apply(Intervention::AddFounders {
            group: group("lean-wolf", 10),
            design: Some(pred.clone()),
        })
        .unwrap();
        // The same design again is fine; a different one under the same key is not.
        let mut other = pred.clone();
        other.name = "Other".into();
        assert!(sim
            .apply(Intervention::AddFounders {
                group: group("lean-wolf", 5),
                design: Some(other),
            })
            .is_err());
        sim.apply(Intervention::AddFounders {
            group: group("lean-wolf", 5),
            design: None,
        })
        .unwrap();
        assert_eq!(sim.designs.len(), 2);
        sim.run_steps(24);

        // Save and load.
        let back = Simulation::load_bytes(&sim.save_bytes().unwrap()).unwrap();
        assert_eq!(back.designs, sim.designs);
        assert_eq!(back.state_hash(), sim.state_hash());

        // The scheduled release of the design still happens after loading.
        let mut back = back;
        back.set_mode(crate::sim::Mode::Population).unwrap();
        back.run_steps(1);
        assert!(back
            .events
            .iter()
            .any(|e| e.text.starts_with("40 founders of Snow grazer")));

        // Replay.
        let r = Replay::run(sim.replay_script().unwrap()).unwrap();
        assert!(r.matches());
        assert_eq!(r.sim.designs, sim.designs);

        // Template: only the creation settings' designs, as format 3.
        let t = sim.user_template("Snow", "").unwrap();
        assert_eq!(t.version, 3);
        assert_eq!(t.designs, vec![cold.clone()]);
        let parsed = UserTemplate::parse(&t.to_json()).unwrap();
        let again = Simulation::new(parsed.new_world()).unwrap();
        let fresh = world(vec![group("snow-grazer", 200), later_group()], vec![cold]).unwrap();
        assert_eq!(again.state_hash(), fresh.state_hash());
        // A format-2 template cannot hold designs.
        let mut v: serde_json::Value = serde_json::from_str(&t.to_json()).unwrap();
        v["version"] = 2.into();
        assert!(UserTemplate::parse(&v.to_string())
            .unwrap_err()
            .contains("format 3"));
    }

    fn later_group() -> FounderGroup {
        FounderGroup {
            delay_years: 0.5,
            diversity: 0.3,
            ..group("snow-grazer", 40)
        }
    }

    #[test]
    fn variation_scales_founder_diversity() {
        let mut narrow = hare_copy();
        narrow.variation = 0.2;
        let mut wide = hare_copy();
        wide.variation = 2.5;
        let sd = |d: SpeciesDesign| {
            let sim = world(vec![group("my-hare", 400)], vec![d]).unwrap();
            let m: Vec<f32> = sim
                .animals
                .iter()
                .map(|a| a.pheno.get(Trait::Sprint))
                .collect();
            let mean = m.iter().sum::<f32>() / m.len() as f32;
            (m.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / m.len() as f32).sqrt()
        };
        assert!(sd(wide) > 4.0 * sd(narrow));
    }
}
