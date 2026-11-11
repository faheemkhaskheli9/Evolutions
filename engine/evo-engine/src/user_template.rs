//! User templates (PRD FR-W9, FR-P3): a world's creation settings, a name and a
//! description in a small JSON file (`.evotemplate`), so a class can start from the
//! same experiment. Not a save: a template holds only what `Simulation::new` needs.
//!
//! Format (TRD TR-DA5), version 1:
//!
//! ```json
//! {
//!   "format": "evolutions-template",
//!   "version": 1,
//!   "name": "Cold north",
//!   "description": "Hares on a continent that reaches the Arctic.",
//!   "base": "earth",
//!   "seed": 42,
//!   "realism": "Accelerated",
//!   "gen": { "width": 128, "height": 64, ... },
//!   "founders": [ { "species": "hare", "count": 400, "diversity": 0.8,
//!                   "region": null, "delay_years": 0 } ]
//! }
//! ```
//!
//! `base` is the built-in template the world is named after ("random" for a random
//! world). `gen` and `founders` are complete, so a template does not depend on the
//! built-in template's settings, which may change between versions.

use crate::params::Realism;
use crate::sim::{validate_founders, validate_gen, NewWorld, Simulation};
use crate::templates::{self, FounderGroup};
use crate::world::WorldGenParams;
use serde::{Deserialize, Serialize};

/// Marks a template file.
pub const TEMPLATE_FORMAT: &str = "evolutions-template";
/// The template format version this engine writes and reads.
pub const TEMPLATE_VERSION: u32 = 1;
/// Longest name, in characters.
pub const MAX_NAME: usize = 80;
/// Longest description, in characters.
pub const MAX_DESCRIPTION: usize = 2000;
/// Largest seed a template may hold: the largest integer JavaScript numbers keep exactly,
/// so a template survives the web app unchanged.
pub const MAX_SEED: u64 = (1 << 53) - 1;
/// Largest template file accepted, in bytes.
pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserTemplate {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub base: String,
    pub seed: u64,
    pub realism: Realism,
    pub gen: WorldGenParams,
    pub founders: Vec<FounderGroup>,
}

impl UserTemplate {
    /// A template from new-world settings: the generator settings and founders are
    /// resolved from the built-in template (or the seed, for a random world) when the
    /// settings leave them out.
    pub fn from_new_world(
        cfg: &NewWorld,
        name: &str,
        description: &str,
    ) -> Result<UserTemplate, String> {
        let base = if cfg.template == "random" {
            templates::random(cfg.seed)
        } else {
            templates::find(&cfg.template)
                .ok_or_else(|| format!("unknown template '{}'", cfg.template))?
        };
        let t = UserTemplate {
            format: TEMPLATE_FORMAT.into(),
            version: TEMPLATE_VERSION,
            name: name.trim().into(),
            description: description.trim().into(),
            base: cfg.template.clone(),
            seed: cfg.seed,
            realism: cfg
                .realism
                .unwrap_or(crate::params::SimParams::default().realism),
            gen: cfg.gen.clone().unwrap_or(base.gen),
            founders: cfg.founders.clone().unwrap_or(base.founders),
        };
        t.validate()?;
        Ok(t)
    }

    /// Reads and checks a template file. Anything malformed, from a newer version, or
    /// with settings a new world would refuse is rejected with a message saying why.
    pub fn parse(text: &str) -> Result<UserTemplate, String> {
        if text.len() > MAX_BYTES {
            return Err(format!(
                "not a template: the file is larger than {} KB",
                MAX_BYTES / 1024
            ));
        }
        let v: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| format!("not a template: the file is not valid JSON ({e})"))?;
        if v.get("format").and_then(|f| f.as_str()) != Some(TEMPLATE_FORMAT) {
            return Err("not a template: this file is not an Evolutions template".into());
        }
        match v.get("version").and_then(|n| n.as_u64()) {
            Some(n) if n == TEMPLATE_VERSION as u64 => {}
            Some(n) if n > TEMPLATE_VERSION as u64 => {
                return Err(format!(
                    "this template was made by a newer version of Evolutions (template format {n}); update the app to open it"
                ))
            }
            _ => return Err("invalid template: unknown format version".into()),
        }
        let t: UserTemplate =
            serde_json::from_value(v).map_err(|e| format!("invalid template: {e}"))?;
        t.validate()?;
        Ok(t)
    }

    /// The same checks as for new-world settings (PRD FR-W1), plus the name and seed.
    pub fn validate(&self) -> Result<(), String> {
        let bad = |m: String| Err(format!("invalid template: {m}"));
        if self.format != TEMPLATE_FORMAT || self.version != TEMPLATE_VERSION {
            return bad("unknown format or version".into());
        }
        let name = self.name.trim();
        if name.is_empty() {
            return bad("the name is empty".into());
        }
        if name.chars().count() > MAX_NAME {
            return bad(format!("the name is longer than {MAX_NAME} characters"));
        }
        if self.description.chars().count() > MAX_DESCRIPTION {
            return bad(format!(
                "the description is longer than {MAX_DESCRIPTION} characters"
            ));
        }
        if name
            .chars()
            .chain(self.description.chars())
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return bad("the name or description contains control characters".into());
        }
        if self.base != "random" && templates::find(&self.base).is_none() {
            return bad(format!("unknown base world '{}'", self.base));
        }
        if self.seed > MAX_SEED {
            return bad(format!("the seed must be at most {MAX_SEED}"));
        }
        validate_gen(&self.gen).or_else(bad)?;
        if self.founders.is_empty() {
            return bad("it has no founders".into());
        }
        validate_founders(&self.founders).or_else(bad)?;
        Ok(())
    }

    /// The settings to create the world with. The world is named after `base` and
    /// carries the template's name as its label, which is outside `state_hash()`.
    pub fn new_world(&self) -> NewWorld {
        NewWorld {
            template: self.base.clone(),
            seed: self.seed,
            gen: Some(self.gen.clone()),
            founders: Some(self.founders.clone()),
            realism: Some(self.realism),
            label: Some(self.name.trim().to_string()),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("templates serialize")
    }
}

impl Simulation {
    /// A template of this world's creation settings (not its current state), so others
    /// can start the same world at year 0.
    pub fn user_template(&self, name: &str, description: &str) -> Result<UserTemplate, String> {
        let origin = self.replay.origin.as_ref().ok_or(
            "this world was saved by an older version that did not record how it was created",
        )?;
        let cfg = NewWorld {
            template: self.template_key.clone(),
            seed: self.seed,
            gen: Some(self.world.gen.clone()),
            founders: origin.founders.clone(),
            realism: Some(self.params.realism),
            label: None,
        };
        UserTemplate::from_new_world(&cfg, name, description)
    }

    /// The name to show for the world: its user template's, else the built-in template's.
    pub fn display_name(&self) -> &str {
        self.replay
            .origin
            .as_ref()
            .and_then(|o| o.label.as_deref())
            .unwrap_or(&self.template_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> NewWorld {
        NewWorld {
            template: "blank".into(),
            seed: 7,
            gen: None,
            founders: None,
            realism: None,
            label: None,
        }
    }

    #[test]
    fn round_trips_and_rebuilds_the_same_world() {
        let sim = Simulation::new(blank()).unwrap();
        let t = sim
            .user_template("Class 1", "Hares on a flat world")
            .unwrap();
        let back = UserTemplate::parse(&t.to_json()).unwrap();
        assert_eq!(back, t);
        let again = Simulation::new(back.new_world()).unwrap();
        assert_eq!(again.state_hash(), sim.state_hash());
        assert_eq!(again.display_name(), "Class 1");
        // A template of the template's world is the same template.
        assert_eq!(
            again
                .user_template("Class 1", "Hares on a flat world")
                .unwrap(),
            t
        );
    }

    #[test]
    fn rejects_bad_files() {
        let good = Simulation::new(blank())
            .unwrap()
            .user_template("T", "")
            .unwrap()
            .to_json();
        let edit = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
            f(&mut v);
            UserTemplate::parse(&v.to_string()).unwrap_err()
        };
        assert!(UserTemplate::parse("{")
            .unwrap_err()
            .contains("not valid JSON"));
        assert!(UserTemplate::parse("{}")
            .unwrap_err()
            .contains("not an Evolutions template"));
        assert!(edit(&|v| v["version"] = 2.into()).contains("newer version"));
        assert!(edit(&|v| v["name"] = "  ".into()).contains("name is empty"));
        assert!(edit(&|v| v["base"] = "mars".into()).contains("unknown base"));
        assert!(edit(&|v| v["gen"]["width"] = 2.into()).contains("world size"));
        assert!(edit(&|v| v["gen"]["lat_north"] = (-60.0).into()).contains("latitudes"));
        assert!(
            edit(&|v| v["founders"][0]["species"] = "dragon".into()).contains("unknown species")
        );
        assert!(edit(&|v| v["founders"][0]["count"] = 0.into()).contains("count"));
        assert!(edit(&|v| v["founders"][0]["diversity"] = 3.into()).contains("diversity"));
        assert!(edit(&|v| v["founders"] = serde_json::json!([])).contains("no founders"));
        assert!(edit(&|v| v["cheat"] = true.into()).contains("unknown field"));
        assert!(edit(&|v| v["seed"] = (-1).into()).contains("invalid template"));
        assert!(edit(&|v| v["realism"] = "Fast".into()).contains("invalid template"));
    }
}
