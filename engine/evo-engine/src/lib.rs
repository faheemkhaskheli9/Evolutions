//! # Evolutions engine
//!
//! A deterministic simulation of biological evolution. Organisms carry diploid
//! genomes; traits develop from genes and environment; animals eat, drink,
//! thermoregulate, flee, hunt, choose mates and reproduce; populations evolve
//! through selection, drift, mutation and gene flow; species are detected from
//! reproductive isolation.
//!
//! Two fidelity modes share one biological model (see `docs/TRD.md` §4.2):
//! **Individual** (hourly ticks, every animal simulated) and **Population**
//! (yearly steps, demes with allele frequencies) for fast-forwarding.
//!
//! ```
//! use evo_engine::{NewWorld, Simulation};
//! let mut sim = Simulation::new(NewWorld {
//!     template: "blank".into(), seed: 1, gen: None, founders: None, realism: None,
//! }).unwrap();
//! sim.run_steps(24);
//! assert!(sim.population() > 0);
//! ```

#![allow(clippy::needless_range_loop)]

pub mod animal;
pub mod api;
pub mod export;
pub mod genetics;
pub mod individual;
pub mod noise;
pub mod params;
pub mod physiology;
pub mod population;
pub mod record;
pub mod replay;
pub mod rng;
pub mod sim;
pub mod species;
pub mod species_lib;
pub mod templates;
pub mod traits;
pub mod world;

pub use params::{Realism, SimParams};
pub use replay::{Replay, ReplayScript};
pub use sim::{Intervention, Mode, NewWorld, Simulation};
pub use templates::FounderGroup;
