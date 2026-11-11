//! Undo and redo for the world editor (PRD FR-W3).
//!
//! Each edit keeps the whole simulation as it was before the stroke (as save bytes),
//! so undo restores the exact previous state, replay log included: an undone stroke
//! is gone from the log. Redo applies the stroke again, which gives the same state
//! because edits are deterministic.
//!
//! The history only holds while nothing else happens: once time runs or another
//! action is taken, the entries no longer match and are dropped.

use crate::sim::{Intervention, Simulation};
use crate::world::Stroke;

/// Most edits that can be undone.
pub const MAX_UNDO: usize = 30;

struct Undo {
    /// The state before the stroke.
    bytes: Vec<u8>,
    /// Simulated minutes and replay-log length just after the stroke.
    minutes: u64,
    log_len: usize,
}

struct Redo {
    stroke: Stroke,
    /// Simulated minutes and replay-log length just after the undo.
    minutes: u64,
    log_len: usize,
}

#[derive(Default)]
pub struct EditHistory {
    undo: Vec<Undo>,
    redo: Vec<Redo>,
}

impl Simulation {
    fn at(&self) -> (u64, usize) {
        (self.minutes, self.replay.entries.len())
    }

    /// Drops undo and redo entries that no longer match the current state.
    fn prune_history(&mut self) {
        let now = self.at();
        if self
            .edit_history
            .undo
            .last()
            .is_some_and(|u| (u.minutes, u.log_len) != now)
        {
            self.edit_history.undo.clear();
        }
        if self
            .edit_history
            .redo
            .last()
            .is_some_and(|r| (r.minutes, r.log_len) != now)
        {
            self.edit_history.redo.clear();
        }
    }

    /// Paints a brush stroke onto the world (logged in the replay log) and remembers
    /// how to undo it.
    pub fn edit_world(&mut self, stroke: Stroke) -> Result<(), String> {
        self.prune_history();
        let bytes = self.save_bytes()?;
        self.apply(Intervention::EditWorld { stroke })?;
        let (minutes, log_len) = self.at();
        let h = &mut self.edit_history;
        h.undo.push(Undo {
            bytes,
            minutes,
            log_len,
        });
        if h.undo.len() > MAX_UNDO {
            h.undo.remove(0);
        }
        h.redo.clear();
        Ok(())
    }

    /// Undoes the last edit. Returns false when there is nothing to undo.
    pub fn undo_edit(&mut self) -> Result<bool, String> {
        self.prune_history();
        let Some(u) = self.edit_history.undo.pop() else {
            return Ok(false);
        };
        let stroke = match self.replay.entries.last().map(|e| &e.action) {
            Some(crate::replay::Action::Intervention(Intervention::EditWorld { stroke })) => {
                stroke.clone()
            }
            _ => {
                self.edit_history = EditHistory::default();
                return Ok(false);
            }
        };
        let history = std::mem::take(&mut self.edit_history);
        *self = Simulation::load_bytes(&u.bytes)?;
        self.edit_history = history;
        let (minutes, log_len) = self.at();
        self.edit_history.redo.push(Redo {
            stroke,
            minutes,
            log_len,
        });
        Ok(true)
    }

    /// Redoes the last undone edit. Returns false when there is nothing to redo.
    pub fn redo_edit(&mut self) -> Result<bool, String> {
        self.prune_history();
        let Some(r) = self.edit_history.redo.pop() else {
            return Ok(false);
        };
        let redo = std::mem::take(&mut self.edit_history.redo);
        self.edit_world(r.stroke)?;
        self.edit_history.redo = redo;
        // The remaining redo entries were recorded before this edit; they still apply
        // after it, so they are moved to the new state.
        let (minutes, log_len) = self.at();
        for e in &mut self.edit_history.redo {
            e.minutes = minutes;
            e.log_len = log_len;
        }
        Ok(true)
    }

    /// How many edits can be undone and redone now.
    pub fn edit_history_len(&self) -> (usize, usize) {
        let now = self.at();
        let ok = |m: u64, l: usize| (m, l) == now;
        (
            match self.edit_history.undo.last() {
                Some(u) if ok(u.minutes, u.log_len) => self.edit_history.undo.len(),
                _ => 0,
            },
            match self.edit_history.redo.last() {
                Some(r) if ok(r.minutes, r.log_len) => self.edit_history.redo.len(),
                _ => 0,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::population::border_connectivity;
    use crate::replay::Replay;
    use crate::sim::NewWorld;
    use crate::user_template::UserTemplate;
    use crate::world::{Brush, Water, World, WorldGenParams};

    fn blank() -> Simulation {
        Simulation::new(NewWorld {
            template: "blank".into(),
            seed: 11,
            gen: None,
            founders: None,
            realism: None,
            label: None,
            edits: vec![],
        })
        .unwrap()
    }

    fn stroke(brush: Brush, radius: f32, points: Vec<[f32; 2]>) -> Stroke {
        Stroke {
            brush,
            radius,
            strength: 1.0,
            points,
        }
    }

    /// Sea over every tile of the patch column `px` (8 tiles wide), pole to pole.
    fn strait(sim: &Simulation, px: usize) -> Stroke {
        let ps = sim.params.patch_size;
        let mut points = vec![];
        for y in 0..sim.world.h {
            for x in px * ps..(px + 1) * ps {
                points.push([x as f32 + 0.5, y as f32 + 0.5]);
            }
        }
        stroke(Brush::Ocean, 0.5, points)
    }

    #[test]
    fn a_painted_strait_cuts_land_connectivity() {
        let mut sim = blank();
        let ps = sim.params.patch_size;
        for py in 0..sim.patches_h() {
            assert_eq!(border_connectivity(&sim.world, ps, 2, py, 1, 0), 1.0);
        }
        let land = sim.world.land_tiles();
        sim.edit_world(strait(&sim, 3)).unwrap();
        assert_eq!(sim.world.land_tiles(), land - ps * sim.world.h);
        for py in 0..sim.patches_h() {
            assert_eq!(border_connectivity(&sim.world, ps, 2, py, 1, 0), 0.0);
            assert_eq!(border_connectivity(&sim.world, ps, 4, py, -1, 0), 0.0);
            assert_eq!(border_connectivity(&sim.world, ps, 1, py, 1, 0), 1.0);
        }
        // Hares standing in the new sea drowned.
        assert!(sim
            .animals
            .iter()
            .all(|a| sim.world.is_land(sim.world.tile_of(a.x, a.y))));
        assert!(sim.events.last().unwrap().text.contains("painted sea"));
    }

    #[test]
    fn rivers_reroute_into_a_painted_strait() {
        let mut w = World::generate(WorldGenParams::default(), 5);
        let rivers = |w: &World| -> Vec<usize> {
            (0..w.w * w.h)
                .filter(|&i| w.water[i] == Water::River)
                .collect()
        };
        let before = rivers(&w);
        assert!(!before.is_empty());
        // A sea one tile wide, pole to pole, through the river with the most water.
        let big = *before
            .iter()
            .max_by(|&&a, &&b| w.river_flow[a].total_cmp(&w.river_flow[b]))
            .unwrap();
        let x = (big % w.w) as f32 + 0.5;
        let points = (0..w.h).map(|y| [x, y as f32 + 0.5]).collect();
        let r = w.apply_stroke(&stroke(Brush::Ocean, 0.5, points));
        assert!(!r.flooded.is_empty());
        let after = rivers(&w);
        assert_ne!(after, before);
        let x = big % w.w;
        assert!(after.iter().all(|&i| i % w.w != x));
        assert_eq!(w.water[big], Water::Ocean);
        // Land beside the strait now drains into it.
        let (_, downstream, _) = crate::world::hydrology::priority_flood(&w);
        assert!((0..w.w * w.h).any(|i| w.is_land(i)
            && downstream[i] != usize::MAX
            && downstream[i] % w.w == x
            && w.river_flow[i] > 0.0));
    }

    #[test]
    fn undo_restores_the_exact_previous_world() {
        let mut sim = blank();
        sim.run_steps(30);
        let h0 = sim.state_hash();
        let log0 = sim.replay.entries.len();
        sim.edit_world(strait(&sim, 3)).unwrap();
        let h1 = sim.state_hash();
        sim.edit_world(stroke(Brush::Raise, 3.0, vec![[10.0, 10.0], [12.0, 11.0]]))
            .unwrap();
        let h2 = sim.state_hash();
        assert_ne!(h1, h0);
        assert_ne!(h2, h1);
        assert_eq!(sim.edit_history_len(), (2, 0));
        assert!(sim.undo_edit().unwrap());
        assert_eq!(sim.state_hash(), h1);
        assert!(sim.undo_edit().unwrap());
        assert_eq!(sim.state_hash(), h0);
        assert_eq!(sim.replay.entries.len(), log0);
        assert!(!sim.undo_edit().unwrap());
        assert_eq!(sim.edit_history_len(), (0, 2));
        assert!(sim.redo_edit().unwrap());
        assert_eq!(sim.state_hash(), h1);
        assert!(sim.redo_edit().unwrap());
        assert_eq!(sim.state_hash(), h2);
        assert!(!sim.redo_edit().unwrap());
        // Once time runs, the edits can no longer be undone.
        sim.run_steps(1);
        assert_eq!(sim.edit_history_len(), (0, 0));
        assert!(!sim.undo_edit().unwrap());
    }

    #[test]
    fn a_replay_with_edits_gives_the_same_state() {
        let mut sim = blank();
        sim.edit_world(stroke(Brush::Lake, 2.0, vec![[40.0, 16.0]]))
            .unwrap();
        sim.run_steps(48);
        sim.edit_world(strait(&sim, 3)).unwrap();
        sim.edit_world(stroke(Brush::Colder, 6.0, vec![[50.0, 10.0], [55.0, 20.0]]))
            .unwrap();
        // An undone stroke is not part of the run.
        sim.edit_world(stroke(Brush::Raise, 4.0, vec![[10.0, 10.0]]))
            .unwrap();
        sim.undo_edit().unwrap();
        sim.run_steps(48);
        let r = Replay::run(sim.replay_script().unwrap()).unwrap();
        assert!(r.matches());
        assert_eq!(r.sim.state_hash(), sim.state_hash());
        // And the save keeps the edited world.
        let back = Simulation::load_bytes(&sim.save_bytes().unwrap()).unwrap();
        assert_eq!(back.state_hash(), sim.state_hash());
    }

    #[test]
    fn a_template_keeps_the_edits_painted_at_year_0() {
        let mut sim = blank();
        sim.edit_world(strait(&sim, 3)).unwrap();
        sim.edit_world(stroke(Brush::Lake, 2.0, vec![[40.0, 16.0]]))
            .unwrap();
        let t = sim.user_template("Strait", "").unwrap();
        assert_eq!(t.version, 2);
        assert_eq!(t.edits.len(), 2);
        let back = UserTemplate::parse(&t.to_json()).unwrap();
        let again = Simulation::new(back.new_world()).unwrap();
        assert_eq!(again.state_hash(), sim.state_hash());
        // Edits made after time ran are history, not part of the template.
        sim.run_steps(5);
        sim.edit_world(stroke(Brush::Raise, 2.0, vec![[10.0, 10.0]]))
            .unwrap();
        assert_eq!(sim.user_template("Strait", "").unwrap().edits.len(), 2);
        // A template of the template's world is the same template.
        assert_eq!(again.user_template("Strait", "").unwrap(), t);
        // Without edits a template is still format 1.
        assert_eq!(blank().user_template("Plain", "").unwrap().version, 1);
    }
}
