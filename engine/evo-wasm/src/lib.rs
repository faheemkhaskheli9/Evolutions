//! WebAssembly bindings: the browser app drives the engine from a Web Worker
//! through this API (System Design §7). Queries return JSON strings or typed arrays.

use evo_engine::{Intervention, Mode, NewWorld, Simulation};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Engine {
    sim: Simulation,
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl Engine {
    /// Creates a world from a JSON `NewWorld` config.
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<Engine, JsError> {
        let cfg: NewWorld = serde_json::from_str(config_json).map_err(err)?;
        Ok(Engine {
            sim: Simulation::new(cfg).map_err(err)?,
        })
    }

    /// Restores a world from save-file bytes.
    pub fn load(bytes: &[u8]) -> Result<Engine, JsError> {
        Ok(Engine {
            sim: Simulation::load_bytes(bytes).map_err(err)?,
        })
    }

    pub fn save(&self) -> Result<Vec<u8>, JsError> {
        self.sim.save_bytes().map_err(err)
    }

    /// Templates, species library and trait metadata.
    pub fn catalog() -> String {
        Simulation::catalog().to_string()
    }

    pub fn step(&mut self, n: u32) {
        self.sim.run_steps(n as u64);
    }

    pub fn step_minutes(&self) -> f64 {
        self.sim.step_minutes() as f64
    }

    pub fn year(&self) -> f64 {
        self.sim.year()
    }

    pub fn population(&self) -> f64 {
        self.sim.population() as f64
    }

    pub fn individual_cap(&self) -> u32 {
        self.sim.params.individual_cap as u32
    }

    pub fn mode(&self) -> String {
        match self.sim.mode {
            Mode::Individual => "Individual".into(),
            Mode::Population => "Population".into(),
        }
    }

    pub fn set_mode(&mut self, mode: &str) -> Result<(), JsError> {
        let m = match mode {
            "Individual" => Mode::Individual,
            "Population" => Mode::Population,
            _ => return Err(JsError::new("mode must be Individual or Population")),
        };
        self.sim.set_mode(m).map_err(err)
    }

    /// Simulates only a rectangle of tiles individually. Returns how many animals that is.
    pub fn focus(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) -> Result<f64, JsError> {
        self.sim
            .focus_region(x0, y0, x1, y1)
            .map(|n| n as f64)
            .map_err(err)
    }

    /// Applies an intervention given as JSON, e.g. `{"kind":"ClimateShift","delta_c":-3}`.
    pub fn apply(&mut self, json: &str) -> Result<(), JsError> {
        let iv: Intervention = serde_json::from_str(json).map_err(err)?;
        self.sim.apply(iv).map_err(err)
    }

    pub fn status(&self) -> String {
        self.sim.status().to_string()
    }

    pub fn world_info(&self) -> String {
        self.sim.world_info().to_string()
    }

    pub fn species(&self) -> String {
        self.sim.species_json().to_string()
    }

    pub fn metrics(&self, species: u32) -> String {
        self.sim.metrics_json(species).to_string()
    }

    pub fn trait_table(&self, species: u32) -> String {
        self.sim.trait_table(species).to_string()
    }

    pub fn events(&self, since: u32) -> String {
        self.sim.events_json(since as usize).to_string()
    }

    pub fn newick(&self) -> String {
        self.sim.newick()
    }

    pub fn inspect(&self, id: f64) -> String {
        self.sim.inspect(id as u64).to_string()
    }

    /// Nearest animal id within `radius` tiles, or -1.
    pub fn pick(&self, x: f32, y: f32, radius: f32) -> f64 {
        self.sim
            .pick(x, y, radius)
            .map(|id| id as f64)
            .unwrap_or(-1.0)
    }

    pub fn layer(&self, kind: &str) -> Vec<f32> {
        self.sim.layer(kind)
    }

    /// Organisms per tile; `species < 0` means all species.
    pub fn density(&self, species: i32) -> Vec<f32> {
        self.sim.density(if species < 0 {
            None
        } else {
            Some(species as u32)
        })
    }

    /// Most abundant species per tile from demes (u32::MAX = none).
    pub fn dominant(&self) -> Vec<u32> {
        self.sim.dominant_species()
    }

    pub fn positions(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(self.sim.animals.len() * 2);
        for a in &self.sim.animals {
            v.push(a.x);
            v.push(a.y);
        }
        v
    }

    pub fn frame_species(&self) -> Vec<u32> {
        self.sim.animals.iter().map(|a| a.species).collect()
    }

    pub fn frame_actions(&self) -> Vec<u8> {
        self.sim.animals.iter().map(|a| a.action as u8).collect()
    }

    /// Animal ids, in the same order as `positions` (f64 holds ids exactly up to 2^53).
    pub fn frame_ids(&self) -> Vec<f64> {
        self.sim.animals.iter().map(|a| a.id as f64).collect()
    }

    /// Body shape per animal for drawing, four values each: current mass (kg), leg
    /// length index, ornament size (0–1) and coat luminance (0–1).
    pub fn frame_shape(&self) -> Vec<f32> {
        use evo_engine::traits::Trait;
        let mut v = Vec::with_capacity(self.sim.animals.len() * 4);
        for a in &self.sim.animals {
            v.push(a.mass);
            v.push(a.pheno.get(Trait::LegLength));
            v.push(a.pheno.get(Trait::Ornament));
            v.push(a.pheno.get(Trait::CoatLuminance));
        }
        v
    }

    /// Latest Population-mode vital rates per deme (for the "why" panel).
    pub fn diagnostics(&self) -> String {
        serde_json::to_string(
            &self
                .sim
                .diagnostics
                .iter()
                .map(|(sp, patch, n, r)| serde_json::json!({"species": sp, "patch": patch, "n": n, "rates": r}))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default()
    }
}
