/// <reference lib="webworker" />
// Engine host: owns the WASM simulation, runs the time loop, and streams frames
// to the UI (System Design §5 and §7).

import init, { Engine } from "../wasm/evo_wasm.js";
import { readAutosave, writeAutosave } from "./autosave";
import type { FrameMsg, FromWorker, ModeChoice, SimEvent, SpeciesInfo, StopCondition, ToWorker } from "./protocol";

const ctx = self as unknown as DedicatedWorkerGlobalScope;

let engine: Engine | null = null;
let playing = false;
let speed = 86400; // sim-seconds per real second (1 day / s)
let modeChoice: ModeChoice = "Auto";
/** The mode choice to return to when a focus region ends (focusing locks Individual mode). */
let modeBeforeFocus: ModeChoice | null = null;
let carryMinutes = 0;
let lastTick = performance.now();
let eventsSent = 0;
let lastSpeciesPost = 0;
let lastDensityPost = 0;
let slowSince = 0;
let achieved = 0;
let capExceeded = false;
let jumping = false;
let cancelJump = false;

let densityOn = false;
let densitySpecies: number | null = null;
let lastAutosave = 0;
/** The world changed since the last autosave (time ran, an intervention, a new world). */
let dirty = false;
let autosaving = false;
let autosavePending = false;

/** Above this speed the engine uses Population mode in Auto (sim-seconds per second). */
const POPULATION_SPEED = 30 * 86400;
const AUTOSAVE_MS = 60_000;

function post(msg: FromWorker, transfer: Transferable[] = []) {
  ctx.postMessage(msg, transfer);
}

function fail(e: unknown) {
  // A Rust panic traps the WASM instance and leaves the engine unusable; drop it so the
  // UI asks for a new world instead of failing on every call.
  if (e instanceof WebAssembly.RuntimeError) {
    engine = null;
    playing = false;
    post({ type: "error", message: `The simulation crashed (${e.message}). Create or open a world to continue.` });
    return;
  }
  post({ type: "error", message: e instanceof Error ? e.message : String(e) });
}

function replaceEngine(next: Engine) {
  engine?.free();
  engine = next;
  playing = false;
}

/** Keeps the latest world in IndexedDB so a closed tab can resume (PRD FR-P1). */
function autosave() {
  lastAutosave = performance.now();
  if (!engine || !dirty) return;
  if (autosaving) {
    // Save again once the write in progress finishes, so a new world is not skipped.
    autosavePending = true;
    return;
  }
  const info = JSON.parse(engine.world_info());
  const year = engine.year();
  const bytes = engine.save();
  dirty = false;
  autosaving = true;
  writeAutosave({ template: info.template, seed: info.seed, year, savedAt: Date.now() }, bytes.buffer as ArrayBuffer)
    // Storage can be unavailable (private windows, quota); autosave is best-effort.
    .catch(() => {})
    .finally(() => {
      autosaving = false;
      if (autosavePending) {
        autosavePending = false;
        autosave();
      }
    });
}

function sendCreated() {
  if (!engine) return;
  post(
    {
      type: "created",
      world: JSON.parse(engine.world_info()),
      elevation: engine.layer("elevation"),
      biome: engine.layer("biome"),
      water: engine.layer("water"),
    },
  );
  eventsSent = 0;
  lastSpeciesPost = 0;
  achieved = 0;
  if (modeBeforeFocus !== null) {
    modeChoice = modeBeforeFocus;
    modeBeforeFocus = null;
  }
  dirty = true;
  sendFrame(true);
  autosave();
}

function sendFrame(full = false) {
  if (!engine) return;
  const status = JSON.parse(engine.status());
  const events = JSON.parse(engine.events(eventsSent));
  eventsSent += events.length;
  const msg: FrameMsg = {
    type: "frame",
    status,
    playing,
    modeChoice,
    achievedSpeed: achieved,
    capExceeded,
    events,
  };
  const transfer: Transferable[] = [];
  const now = performance.now();
  if (status.mode === "Individual") {
    msg.positions = engine.positions();
    msg.species = engine.frame_species();
    msg.actions = engine.frame_actions();
    transfer.push(msg.positions.buffer, msg.species.buffer, msg.actions.buffer);
  }
  if (status.mode === "Population" || status.focus || densityOn) {
    if (full || now - lastDensityPost > 300) {
      lastDensityPost = now;
      msg.density = engine.density(densityOn && densitySpecies !== null ? densitySpecies : -1);
      msg.dominant = engine.dominant();
      transfer.push(msg.density.buffer, msg.dominant.buffer);
    }
  }
  if (full || now - lastSpeciesPost > 600 || events.length > 0) {
    lastSpeciesPost = now;
    msg.speciesList = JSON.parse(engine.species());
  }
  post(msg, transfer);
}

/**
 * After something forced Population mode (a jump, or the engine's cap), returns to the
 * mode the user chose, or flags that the world is too large for it.
 */
function restoreMode() {
  if (!engine) return;
  if (modeBeforeFocus !== null && !JSON.parse(engine.status()).focus) {
    modeChoice = modeBeforeFocus;
    modeBeforeFocus = null;
  }
  if (modeChoice !== "Individual" || engine.mode() !== "Population") return;
  if (engine.population() <= engine.individual_cap()) {
    engine.set_mode("Individual");
    capExceeded = false;
  } else {
    capExceeded = true;
  }
}

/** Picks the fidelity mode for the requested speed (TRD TR-T3). */
function autoMode() {
  if (!engine || modeChoice !== "Auto") return;
  const mode = engine.mode();
  const st = JSON.parse(engine.status());
  if (speed >= POPULATION_SPEED && mode === "Individual") {
    engine.set_mode("Population");
    capExceeded = false;
  } else if (speed < POPULATION_SPEED && mode === "Population") {
    if (engine.population() <= engine.individual_cap()) {
      engine.set_mode("Individual");
      capExceeded = false;
    } else {
      capExceeded = true;
    }
  } else if (mode === "Individual" && !st.focus) {
    capExceeded = false;
  }
}

function loop() {
  const now = performance.now();
  const dt = Math.min(0.1, (now - lastTick) / 1000);
  lastTick = now;
  if (engine && playing && !jumping) {
    try {
      autoMode();
      carryMinutes += (speed * dt) / 60;
      const stepMin = engine.step_minutes();
      const start = performance.now();
      let ran = 0;
      while (carryMinutes >= stepMin && performance.now() - start < 22) {
        engine.step(1);
        carryMinutes -= stepMin;
        ran += stepMin;
      }
      if (ran > 0) dirty = true;
      let dropped = 0;
      if (carryMinutes > stepMin * 2) {
        // Falling behind: drop the backlog; in Auto mode, give up on Individual mode.
        dropped = carryMinutes - stepMin;
        carryMinutes = stepMin;
        if (modeChoice === "Auto" && engine.mode() === "Individual" && !JSON.parse(engine.status()).focus) {
          if (slowSince === 0) slowSince = now;
          if (now - slowSince > 2500) {
            engine.set_mode("Population");
            post({ type: "notice", message: "Too many animals to simulate individually at this speed — switched to Population mode." });
            slowSince = 0;
          }
        }
      } else {
        slowSince = 0;
      }
      // Achieved speed: the requested speed less any backlog dropped, smoothed over about a
      // second. (Steps of a whole year land on only some frames, so a per-frame count of
      // simulated time would flicker between zero and a spike.)
      if (dt > 0) {
        const now = Math.max(0, speed - (dropped * 60) / dt);
        achieved += (now - achieved) * Math.min(1, dt / 1);
      }
      restoreMode();
      if (engine.population() === 0) {
        playing = false;
        post({ type: "notice", message: "All life has died out." });
      }
    } catch (e) {
      playing = false;
      fail(e);
    }
  }
  try {
    if (engine && !jumping) sendFrame();
    if (engine && !jumping && now - lastAutosave > AUTOSAVE_MS) autosave();
  } catch (e) {
    fail(e);
  } finally {
    setTimeout(loop, 33);
  }
}

/** Returns why `until` is met now, or null. `seen` is the event count already checked. */
function conditionMet(e: Engine, until: StopCondition, seen: { events: number }): string | null {
  if (until.kind === "speciation" || until.kind === "extinction") {
    const kind = until.kind === "speciation" ? "Speciation" : "Extinction";
    const fresh: SimEvent[] = JSON.parse(e.events(seen.events));
    seen.events += fresh.length;
    const hit = fresh.find((ev) => ev.kind === kind);
    return hit ? hit.text : null;
  }
  const crossed = (v: number) => (until.op === "below" ? v < until.value : v > until.value);
  if (until.kind === "population" && until.species === null) {
    const n = e.population();
    return crossed(n) ? `Population ${until.op} ${until.value.toLocaleString()} (now ${n.toLocaleString()}).` : null;
  }
  const sp = (JSON.parse(e.species()) as SpeciesInfo[]).find((s) => s.id === until.species);
  if (!sp) return null;
  if (until.kind === "population") {
    return crossed(sp.n) ? `${sp.common_name}: population ${until.op} ${until.value.toLocaleString()} (now ${sp.n.toLocaleString()}).` : null;
  }
  const v = sp.mean?.[until.trait];
  return v !== undefined && sp.n > 0 && crossed(v) ? `${sp.common_name}: trait ${until.op} ${until.value} (now ${v.toPrecision(3)}).` : null;
}

async function jump(years: number, until?: StopCondition) {
  if (!engine) return;
  jumping = true;
  cancelJump = false;
  const wasPlaying = playing;
  playing = false;
  let reason: string | null = null;
  try {
    if (engine.mode() === "Individual" && years > 2) engine.set_mode("Population");
    const startYear = engine.year();
    const target = startYear + years;
    const seen = { events: JSON.parse(engine.status()).events as number };
    // A condition that already holds would stop at once; only react to it becoming true.
    const armed = !until || until.kind === "speciation" || until.kind === "extinction" || conditionMet(engine, until, { events: 0 }) === null;
    dirty = true;
    while (engine.year() < target && !cancelJump && !reason) {
      const t0 = performance.now();
      while (engine.year() < target && performance.now() - t0 < 40) {
        engine.step(1);
        if (until && armed && (reason = conditionMet(engine, until, seen))) break;
      }
      post({ type: "progress", done: engine.year() - startYear, total: years, label: `Year ${Math.floor(engine.year()).toLocaleString()}` });
      sendFrame();
      await new Promise((r) => setTimeout(r, 0));
      if (engine.population() === 0) break;
    }
    if (engine.population() === 0) post({ type: "notice", message: "All life has died out." });
    else if (until && !armed) post({ type: "notice", message: "That condition already holds, so the run went the full length." });
    else if (until && !reason && !cancelJump) post({ type: "notice", message: `Ran ${years.toLocaleString()} years without the condition being met.` });
  } catch (e) {
    fail(e);
  }
  jumping = false;
  playing = wasPlaying && !reason;
  post({ type: "jumpDone" });
  if (reason && engine) post({ type: "notice", message: `Stopped in year ${Math.floor(engine.year()).toLocaleString()}: ${reason}` });
  try {
    if (engine) {
      restoreMode();
      sendFrame(true);
      autosave();
    }
  } catch (e) {
    fail(e);
  }
}

ctx.onmessage = (ev: MessageEvent<ToWorker>) => {
  const m = ev.data;
  try {
    switch (m.type) {
      case "create":
        // Build the new engine before freeing the old one so a failure keeps the current world.
        replaceEngine(new Engine(JSON.stringify(m.config)));
        sendCreated();
        break;
      case "load":
        replaceEngine(Engine.load(new Uint8Array(m.bytes)));
        sendCreated();
        break;
      case "restore":
        readAutosave()
          .then((slot) => {
            if (!slot) throw new Error("There is no autosaved world.");
            replaceEngine(Engine.load(new Uint8Array(slot.bytes)));
            sendCreated();
          })
          .catch(fail);
        break;
      case "play":
        playing = true;
        lastTick = performance.now();
        break;
      case "pause":
        playing = false;
        break;
      case "speed":
        speed = m.simSecondsPerSecond;
        carryMinutes = 0;
        break;
      case "mode":
        modeChoice = m.mode;
        modeBeforeFocus = null;
        if (engine && m.mode !== "Auto") {
          try {
            engine.set_mode(m.mode);
            capExceeded = false;
          } catch (e) {
            capExceeded = true;
            fail(e);
          }
        }
        sendFrame(true);
        break;
      case "focus":
        if (engine) {
          const n = engine.focus(...m.rect);
          if (modeBeforeFocus === null) modeBeforeFocus = modeChoice;
          modeChoice = "Individual";
          capExceeded = false;
          post({ type: "notice", message: `Watching ${n.toLocaleString()} animals individually in the selected region.` });
          sendFrame(true);
        }
        break;
      case "jump":
        void jump(m.years, m.until);
        break;
      case "cancel":
        cancelJump = true;
        break;
      case "apply":
        engine?.apply(JSON.stringify(m.intervention));
        dirty = true;
        sendFrame(true);
        break;
      case "pick":
        if (engine) post({ type: "picked", id: engine.pick(m.x, m.y, m.radius) });
        break;
      case "inspect":
        if (engine) post({ type: "inspect", id: m.id, data: JSON.parse(engine.inspect(m.id)) });
        break;
      case "speciesDetail":
        if (engine) {
          const traits = JSON.parse(engine.trait_table(m.id));
          // null: the species id belongs to a world that has since been replaced.
          if (traits) post({ type: "speciesDetail", id: m.id, traits, metrics: JSON.parse(engine.metrics(m.id)) });
        }
        break;
      case "layer":
        if (engine) {
          const data = engine.layer(m.kind);
          post({ type: "layer", kind: m.kind, data }, [data.buffer]);
        }
        break;
      case "density":
        densityOn = m.on;
        densitySpecies = m.species;
        sendFrame(true);
        break;
      case "save":
        if (engine) {
          const bytes = engine.save();
          post({ type: "saved", bytes: bytes.buffer as ArrayBuffer }, [bytes.buffer]);
        }
        break;
      case "newick":
        if (engine) post({ type: "newick", text: engine.newick() });
        break;
    }
  } catch (e) {
    fail(e);
  }
};

init().then(async () => {
  const slot = await readAutosave().catch(() => undefined);
  post({ type: "ready", catalog: JSON.parse(Engine.catalog()), autosave: slot?.info ?? null });
  loop();
}, fail);
