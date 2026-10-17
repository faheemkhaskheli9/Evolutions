/// <reference lib="webworker" />
// Engine host: owns the WASM simulation, runs the time loop, and streams frames
// to the UI (System Design §5 and §7).

import init, { Engine } from "../wasm/evo_wasm.js";
import type { FrameMsg, FromWorker, ModeChoice, ToWorker } from "./protocol";

const ctx = self as unknown as DedicatedWorkerGlobalScope;

let engine: Engine | null = null;
let playing = false;
let speed = 86400; // sim-seconds per real second (1 day / s)
let modeChoice: ModeChoice = "Auto";
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

/** Above this speed the engine uses Population mode in Auto (sim-seconds per second). */
const POPULATION_SPEED = 30 * 86400;

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
  sendFrame(true);
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
  if (status.mode === "Population" || status.focus) {
    if (full || now - lastDensityPost > 300) {
      lastDensityPost = now;
      msg.density = engine.density(-1);
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
      achieved = dt > 0 ? (ran * 60) / dt : 0;
      if (carryMinutes > stepMin * 2) {
        // Falling behind: drop the backlog; in Auto mode, give up on Individual mode.
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
  } catch (e) {
    fail(e);
  } finally {
    setTimeout(loop, 33);
  }
}

async function jump(years: number) {
  if (!engine) return;
  jumping = true;
  cancelJump = false;
  const wasPlaying = playing;
  playing = false;
  try {
    if (engine.mode() === "Individual" && years > 2) engine.set_mode("Population");
    const startYear = engine.year();
    const target = startYear + years;
    while (engine.year() < target && !cancelJump) {
      const t0 = performance.now();
      while (engine.year() < target && performance.now() - t0 < 40) engine.step(1);
      post({ type: "progress", done: engine.year() - startYear, total: years, label: `Year ${Math.floor(engine.year()).toLocaleString()}` });
      sendFrame();
      await new Promise((r) => setTimeout(r, 0));
      if (engine.population() === 0) break;
    }
  } catch (e) {
    fail(e);
  }
  jumping = false;
  playing = wasPlaying;
  post({ type: "jumpDone" });
  try {
    if (engine) sendFrame(true);
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
          modeChoice = "Individual";
          capExceeded = false;
          post({ type: "notice", message: `Watching ${n.toLocaleString()} animals individually in the selected region.` });
          sendFrame(true);
        }
        break;
      case "jump":
        void jump(m.years);
        break;
      case "cancel":
        cancelJump = true;
        break;
      case "apply":
        engine?.apply(JSON.stringify(m.intervention));
        sendFrame(true);
        break;
      case "pick":
        if (engine) post({ type: "picked", id: engine.pick(m.x, m.y, m.radius) });
        break;
      case "inspect":
        if (engine) post({ type: "inspect", data: JSON.parse(engine.inspect(m.id)) });
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

init().then(() => {
  post({ type: "ready", catalog: JSON.parse(Engine.catalog()) });
  loop();
}, fail);
