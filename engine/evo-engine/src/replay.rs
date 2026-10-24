//! Replay log (PRD FR-P2): every intervention, mode switch and focus region is
//! recorded with its simulated time, so a world created from the same settings
//! replays identically. The log travels in the save file and can be exported as a
//! JSON replay script.

use crate::sim::{Intervention, Mode, NewWorld, Simulation, MODEL_VERSION};
use serde::{Deserialize, Serialize};

/// Marks a JSON replay script.
pub const REPLAY_FORMAT: &str = "evolutions-replay";

/// A user action that changes the world between steps.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Intervention(Intervention),
    Mode(Mode),
    /// Focus region in tile coordinates (x0, y0, x1, y1), as passed to `focus_region`.
    Focus([u32; 4]),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    /// Simulated minutes when the action was taken.
    pub minutes: u64,
    pub action: Action,
}

/// The settings a world was created from, and every action taken since.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReplayLog {
    /// None for worlds loaded from saves made before replay logs existed.
    pub origin: Option<NewWorld>,
    pub entries: Vec<LogEntry>,
}

/// An exported replay: the log plus where the run ended and the state it reached.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayScript {
    pub format: String,
    pub model_version: u32,
    pub origin: NewWorld,
    pub entries: Vec<LogEntry>,
    /// Simulated minutes to run to.
    pub end_minutes: u64,
    /// `state_hash()` of the original run at `end_minutes`, in hex.
    pub state_hash: String,
}

impl Simulation {
    pub(crate) fn log(&mut self, action: Action) {
        let minutes = self.minutes;
        self.replay.entries.push(LogEntry { minutes, action });
    }

    /// The replay script for this world up to now.
    pub fn replay_script(&self) -> Result<ReplayScript, String> {
        let origin = self.replay.origin.clone().ok_or(
            "this world was saved by an older version without a replay log, so it cannot be replayed",
        )?;
        Ok(ReplayScript {
            format: REPLAY_FORMAT.into(),
            model_version: MODEL_VERSION,
            origin,
            entries: self.replay.entries.clone(),
            end_minutes: self.minutes,
            state_hash: format!("{:016x}", self.state_hash()),
        })
    }

    fn perform(&mut self, action: &Action) {
        // Failed actions are logged only when they changed something (a focus that first
        // left Individual mode), and fail the same way again, so errors are ignored.
        let _ = match *action {
            Action::Intervention(ref iv) => self.apply(iv.clone()),
            Action::Mode(m) => self.set_mode(m),
            Action::Focus([x0, y0, x1, y1]) => self.focus_region(x0, y0, x1, y1).map(|_| ()),
        };
    }
}

/// A replay in progress: recreates the world and applies the log at the recorded times.
pub struct Replay {
    pub sim: Simulation,
    entries: Vec<LogEntry>,
    next: usize,
    start: u64,
    end: u64,
    expected_hash: String,
}

impl Replay {
    pub fn new(script: ReplayScript) -> Result<Replay, String> {
        if script.format != REPLAY_FORMAT {
            return Err("not an Evolutions replay".into());
        }
        if script.model_version > MODEL_VERSION {
            return Err("replay was made by a newer engine".into());
        }
        if script
            .entries
            .windows(2)
            .any(|w| w[1].minutes < w[0].minutes)
        {
            return Err("replay entries are out of order".into());
        }
        let sim = Simulation::new(script.origin)?;
        Ok(Replay {
            start: sim.minutes,
            sim,
            entries: script.entries,
            next: 0,
            end: script.end_minutes,
            expected_hash: script.state_hash,
        })
    }

    /// Runs at most `max_steps` engine steps, applying entries as their times come up.
    /// Returns true once the replay has reached the end of the original run.
    pub fn advance(&mut self, max_steps: u64) -> Result<bool, String> {
        let mut steps = 0;
        loop {
            match self.entries.get(self.next) {
                Some(e) if self.sim.minutes >= e.minutes => {
                    if self.sim.minutes > e.minutes {
                        return Err(format!(
                            "replay diverged: an action at minute {} fell inside a step",
                            e.minutes
                        ));
                    }
                    let action = e.action.clone();
                    self.sim.perform(&action);
                    self.next += 1;
                    continue;
                }
                None if self.sim.minutes >= self.end => return Ok(true),
                _ => {}
            }
            if steps == max_steps {
                return Ok(false);
            }
            self.sim.step();
            steps += 1;
        }
    }

    /// Fraction of the run replayed so far.
    pub fn progress(&self) -> f64 {
        let span = self.end.saturating_sub(self.start).max(1);
        (self.sim.minutes.saturating_sub(self.start) as f64 / span as f64).min(1.0)
    }

    /// Simulated year the replay runs to.
    pub fn end_year(&self) -> f64 {
        self.end as f64 / self.sim.minutes_per_year()
    }

    /// Whether the replayed world matches the original run's state hash.
    pub fn matches(&self) -> bool {
        format!("{:016x}", self.sim.state_hash()) == self.expected_hash
    }

    /// Replays a whole script.
    pub fn run(script: ReplayScript) -> Result<Replay, String> {
        let mut r = Replay::new(script)?;
        while !r.advance(u64::MAX)? {}
        Ok(r)
    }
}
