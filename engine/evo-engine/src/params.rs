//! Model parameters (TRD TR-S2). Every coefficient lives here with its unit and,
//! where it comes from the literature, its source. Defaults are starting points
//! that the validation suite checks.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Realism {
    /// Literature mutation rates. Evolution is slow, as in nature.
    Strict,
    /// Mutation rates multiplied (see `mutation_multiplier`) so change is visible sooner.
    Accelerated,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimParams {
    // ---- Time ---------------------------------------------------------------
    /// Length of one Individual-mode tick in sim-minutes.
    pub tick_minutes: u64,
    pub day_minutes: u64,
    pub year_days: f64,

    // ---- Genetics (TRD §5.4) --------------------------------------------------
    pub realism: Realism,
    /// Per functional locus per gamete (Strict). Typical per-gene rates are 1e-5–1e-6.
    pub mutation_rate: f64,
    /// Applied to all rates when `realism == Accelerated`.
    pub mutation_multiplier: f64,
    /// Neutral marker loci mutate faster (microsatellite-like) to act as a clock.
    pub neutral_rate_factor: f64,
    /// Mutation rate of incompatibility loci relative to functional loci. Lower rates let
    /// each new incompatibility allele sweep within a connected population before the next
    /// arises (with accelerated rates, competing alleles otherwise purge each other).
    pub dmi_rate_factor: f64,
    /// Fraction of new mutations with large effect.
    pub large_effect_fraction: f64,
    /// Probability a pair of derived alleles at two incompatibility loci is incompatible (Orr 1995).
    pub dmi_probability: f64,
    /// Fitness (viability) loss per incompatible pair.
    pub dmi_cost: f64,

    // ---- Ecology / population scale -----------------------------------------
    /// Effective foraging habitat represented per tile (m²). Tiles are tens of km wide for
    /// climate; this sets how many simulated animals a tile supports (population scale).
    pub habitat_m2_per_tile: f64,
    /// Energy density of dry plant forage (kJ/g).
    pub forage_kj_per_g: f64,
    /// Energy density of meat / carcass (kJ/g wet mass).
    pub meat_kj_per_g: f64,
    /// Daily fractional decay of carcass energy.
    pub carcass_decay_per_day: f64,
    /// Baseline extrinsic mortality (per year) not otherwise modelled (accidents).
    pub background_mortality: f64,

    // ---- Species detection (TRD §5.10) ---------------------------------------
    pub species_check_years: f64,
    /// Reproductive compatibility below this separates species.
    pub compat_threshold: f64,
    /// Patch edge in tiles; patches are the demes for Population mode and species detection.
    pub patch_size: usize,

    // ---- Fidelity (TRD §4.2) --------------------------------------------------
    /// Maximum animals simulated individually; above this the engine uses Population mode.
    pub individual_cap: usize,
    /// Surrogate sample size per deme per Population-mode step (TRD TR-MP2).
    pub surrogate_samples: usize,
}

impl Default for SimParams {
    fn default() -> Self {
        SimParams {
            tick_minutes: 60,
            day_minutes: 24 * 60,
            year_days: 365.25,
            realism: Realism::Accelerated,
            mutation_rate: 1e-5,
            mutation_multiplier: 20.0,
            neutral_rate_factor: 5.0,
            dmi_rate_factor: 0.1,
            large_effect_fraction: 0.05,
            dmi_probability: 0.1,
            dmi_cost: 0.35,
            habitat_m2_per_tile: 30000.0,
            forage_kj_per_g: 18.0,
            meat_kj_per_g: 7.0,
            carcass_decay_per_day: 0.15,
            background_mortality: 0.02,
            species_check_years: 20.0,
            compat_threshold: 0.5,
            patch_size: 8,
            individual_cap: 25_000,
            surrogate_samples: 48,
        }
    }
}

impl SimParams {
    pub fn effective_mutation_rate(&self) -> f64 {
        match self.realism {
            Realism::Strict => self.mutation_rate,
            Realism::Accelerated => self.mutation_rate * self.mutation_multiplier,
        }
    }

    pub fn ticks_per_day(&self) -> u64 {
        self.day_minutes / self.tick_minutes
    }

    pub fn minutes_per_year(&self) -> u64 {
        (self.year_days * self.day_minutes as f64).round() as u64
    }
}
