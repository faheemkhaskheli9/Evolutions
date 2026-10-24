import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { EngineClient } from "./engine/client";
import type { AlleleInfo, AutosaveInfo, Catalog, FromWorker, Hydrology, ModeChoice, NewWorldConfig, Sample, SimEvent, SpeciesInfo, Status, TraitRow, WorldInfo } from "./engine/protocol";
import { inspectReply } from "./engine/protocol";
import { MapView, type FrameData, type Layer, type Tool } from "./components/MapView";
import { SpeciesPanel } from "./components/SpeciesPanel";
import { PhyloTree } from "./components/PhyloTree";
import { EventsPanel } from "./components/EventsPanel";
import { Inspector } from "./components/Inspector";
import { WorldPanel } from "./components/WorldPanel";
import { NewWorldDialog } from "./components/NewWorldDialog";
import { RunUntil } from "./components/RunUntil";
import { describeSpeed, downloadBlob, formatDate, formatYears, SPEEDS, type RGB } from "./util";

// three.js is large; load it only when the 3D view is shown.
const WorldView3D = lazy(() => import("./components/WorldView3D").then((m) => ({ default: m.WorldView3D })));

type Tab = "species" | "tree" | "events" | "inspect" | "world";
type View = "3d" | "map";

const VIEW_KEY = "evolutions.view";
function savedView(): View {
  try {
    return localStorage.getItem(VIEW_KEY) === "map" ? "map" : "3d";
  } catch {
    return "3d";
  }
}

interface WorldState {
  info: WorldInfo;
  elevation: Float32Array;
  biome: Float32Array;
  water: Float32Array;
  hydro: Hydrology;
}

export default function App() {
  const client = useMemo(() => new EngineClient(), []);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [autosave, setAutosave] = useState<AutosaveInfo | null>(null);
  const [world, setWorld] = useState<WorldState | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [playing, setPlaying] = useState(false);
  const [modeChoice, setModeChoice] = useState<ModeChoice>("Auto");
  const [capExceeded, setCapExceeded] = useState(false);
  const [achieved, setAchieved] = useState(0);
  const [speed, setSpeed] = useState(SPEEDS[1].value);
  const [species, setSpecies] = useState<SpeciesInfo[]>([]);
  const [events, setEvents] = useState<SimEvent[]>([]);
  const [tab, setTab] = useState<Tab>("species");
  const [selectedSpecies, setSelectedSpecies] = useState<number | null>(null);
  const [detail, setDetail] = useState<{ id: number; traits: TraitRow[]; metrics: Sample[] } | null>(null);
  const [inspectId, setInspectId] = useState<number | null>(null);
  const [inspectData, setInspectData] = useState<Record<string, unknown> | null>(null);
  const [layer, setLayer] = useState<Layer>("terrain");
  const [layerData, setLayerData] = useState<Float32Array | null>(null);
  const [densitySpecies, setDensitySpecies] = useState<number | null>(null);
  const [alleleSpecies, setAlleleSpecies] = useState<number | null>(null);
  const [alleleUid, setAlleleUid] = useState<number | null>(null);
  const [alleles, setAlleles] = useState<{ species: number; list: AlleleInfo[] } | null>(null);
  /** The layer last requested, so a late reply for another layer is not drawn. */
  const layerKind = useRef<string | null>(null);
  const [tool, setTool] = useState<Tool>("inspect");
  const [view, setView] = useState<View>(savedView);
  const [progress, setProgress] = useState<{ done: number; total: number; label: string; title?: string } | null>(null);
  const [notices, setNotices] = useState<{ id: number; text: string; error?: boolean }[]>([]);
  const [showNew, setShowNew] = useState(true);
  const [busy, setBusy] = useState(false);
  const [founderSpecies, setFounderSpecies] = useState("hare");
  const [founderCount, setFounderCount] = useState(100);
  const frame = useRef<FrameData>({ mode: "Individual", focus: null });
  const [frameVersion, setFrameVersion] = useState(0);
  const noticeId = useRef(0);
  const inspectRef = useRef<number | null>(null);
  inspectRef.current = inspectId;

  const notify = useCallback((text: string, error = false) => {
    const id = ++noticeId.current;
    setNotices((n) => [...n.slice(-3), { id, text, error }]);
    setTimeout(() => setNotices((n) => n.filter((x) => x.id !== id)), error ? 9000 : 5000);
  }, []);

  useEffect(() => {
    const off = client.on((m: FromWorker) => {
      switch (m.type) {
        case "ready":
          setCatalog(m.catalog);
          setAutosave(m.autosave);
          break;
        case "created":
          setWorld({ info: m.world, elevation: m.elevation, biome: m.biome, water: m.water, hydro: m.hydro });
          setEvents([]);
          setSpecies([]);
          setSelectedSpecies(null);
          setInspectId(null);
          setInspectData(null);
          setDetail(null);
          setShowNew(false);
          setBusy(false);
          setLayerData(null);
          setDensitySpecies(null);
          setAlleleSpecies(null);
          setAlleleUid(null);
          setAlleles(null);
          break;
        case "frame": {
          const f = frame.current;
          f.mode = m.status.mode;
          f.focus = m.status.focus;
          if (m.positions) {
            f.positions = m.positions;
            f.species = m.species;
            f.actions = m.actions;
            f.ids = m.ids;
            f.shape = m.shape;
          } else if (m.status.mode === "Population") {
            f.positions = undefined;
            f.species = undefined;
            f.actions = undefined;
            f.ids = undefined;
            f.shape = undefined;
          }
          if (m.snow) f.snow = m.snow;
          if (m.forage) f.forage = m.forage;
          f.day = m.status.day_of_year;
          f.days = m.status.days;
          f.speed = m.playing ? m.achievedSpeed : 0;
          if (m.density) {
            f.density = m.density;
            f.dominant = m.dominant;
          }
          setFrameVersion((v) => v + 1);
          setStatus(m.status);
          setPlaying(m.playing);
          setModeChoice(m.modeChoice);
          setCapExceeded(m.capExceeded);
          setAchieved(m.achievedSpeed);
          if (m.events.length) setEvents((e) => [...e, ...m.events].slice(-2000));
          if (m.speciesList) setSpecies(m.speciesList);
          break;
        }
        case "progress":
          setProgress({ done: m.done, total: m.total, label: m.label, title: m.title });
          break;
        case "jumpDone":
          setProgress(null);
          break;
        case "inspect": {
          // Ignore replies for an animal no longer inspected (or from a replaced world).
          const reply = inspectReply(inspectRef.current, m);
          if (reply === "update") setInspectData(m.data);
          else if (reply === "gone") {
            // It died, or left the simulated area: keep its last state and stop polling.
            setInspectId(null);
            setInspectData((d) => (d ? { ...d, gone: true } : null));
          }
          break;
        }
        case "picked":
          if (m.id >= 0) {
            inspectRef.current = m.id;
            setInspectId(m.id);
            setTab("inspect");
            client.send({ type: "inspect", id: m.id });
          }
          break;
        case "speciesDetail":
          setDetail({ id: m.id, traits: m.traits, metrics: m.metrics });
          break;
        case "layer":
          if (m.kind === layerKind.current) setLayerData(m.data);
          break;
        case "alleles":
          setAlleles({ species: m.species, list: m.list });
          break;
        case "saved":
          downloadBlob(m.bytes, `evolutions-${Date.now()}.evo`);
          notify("World saved.");
          break;
        case "newick":
          downloadBlob(m.text, "phylogeny.nwk", "text/plain");
          break;
        case "replayScript":
          downloadBlob(m.text, `evolutions-replay-${Date.now()}.json`, "application/json");
          notify("Replay log exported. Open it to replay the run.");
          break;
        case "notice":
          notify(m.message);
          break;
        case "error":
          setBusy(false);
          notify(m.message, true);
          break;
      }
    });
    return () => {
      off();
      client.terminate();
    };
  }, [client, notify]);

  // Keep the inspector and species detail fresh while time runs.
  useEffect(() => {
    if (inspectId === null) return;
    const t = setInterval(() => client.send({ type: "inspect", id: inspectId }), 500);
    return () => clearInterval(t);
  }, [inspectId, client]);
  useEffect(() => {
    if (selectedSpecies === null) return;
    client.send({ type: "speciesDetail", id: selectedSpecies });
    const t = setInterval(() => client.send({ type: "speciesDetail", id: selectedSpecies }), 1500);
    return () => clearInterval(t);
  }, [selectedSpecies, client]);
  useEffect(() => {
    setLayerData(null);
    const kind =
      !world || layer === "terrain" || layer === "population"
        ? null
        : layer === "alleles"
          ? alleleSpecies !== null && alleleUid !== null
            ? `allele:${alleleSpecies}:${alleleUid}`
            : null
          : layer === "temperature"
            ? "temperature"
            : layer === "rainfall"
              ? "precip"
              : "forage";
    layerKind.current = kind;
    if (!kind) return;
    client.send({ type: "layer", kind });
    if (kind === "precip") return;
    const t = setInterval(() => client.send({ type: "layer", kind }), 1500);
    return () => clearInterval(t);
  }, [layer, world, client, alleleSpecies, alleleUid]);

  // Allele-frequency layer: the species defaults to the selected one, and its list of
  // polymorphic alleles is refreshed while time runs.
  useEffect(() => {
    if (layer !== "alleles" || alleleSpecies !== null) return;
    const alive = species.filter((s) => s.extinct_year === null);
    const pick = alive.find((s) => s.id === selectedSpecies) ?? alive[0];
    if (pick) setAlleleSpecies(pick.id);
  }, [layer, alleleSpecies, species, selectedSpecies]);
  useEffect(() => {
    if (!world || layer !== "alleles" || alleleSpecies === null) return;
    client.send({ type: "alleles", species: alleleSpecies });
    const t = setInterval(() => client.send({ type: "alleles", species: alleleSpecies }), 3000);
    return () => clearInterval(t);
  }, [layer, alleleSpecies, world, client]);
  useEffect(() => {
    // Start with the most variable allele; keep a chosen one even if it is lost or fixed.
    if (alleles && alleles.species === alleleSpecies && alleleUid === null && alleles.list.length) setAlleleUid(alleles.list[0].uid);
  }, [alleles, alleleSpecies, alleleUid]);

  useEffect(() => {
    if (world) client.send({ type: "density", on: layer === "population", species: densitySpecies });
  }, [layer, densitySpecies, world, client]);

  useEffect(() => {
    if (species.length && selectedSpecies === null) setSelectedSpecies(species[0].id);
  }, [species, selectedSpecies]);

  const togglePlay = useCallback(() => client.send({ type: playing ? "pause" : "play" }), [client, playing]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (showNew) return;
      if (e.key === "Escape") setTool("inspect");
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement) return;
      if (e.code === "Space") {
        e.preventDefault();
        if (!e.repeat && world && !progress) togglePlay();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [togglePlay, showNew, world, progress]);

  const speciesColors = useMemo(() => new Map<number, RGB>(species.map((s) => [s.id, s.color])), [species]);
  const create = (cfg: NewWorldConfig) => {
    setBusy(true);
    client.send({ type: "create", config: cfg });
  };
  const load = async (file: File) => {
    if (file.name.toLowerCase().endsWith(".json")) {
      // A replay script (PRD FR-P2): rebuild the world from its seed and logged actions.
      client.send({ type: "replay", script: await file.text() });
      return;
    }
    setBusy(true);
    const bytes = await file.arrayBuffer();
    client.send({ type: "load", bytes }, [bytes]);
  };
  const restore = () => {
    setBusy(true);
    client.send({ type: "restore" });
  };
  const setSpeedAndSend = (v: number) => {
    setSpeed(v);
    client.send({ type: "speed", simSecondsPerSecond: v });
  };
  const chooseView = (v: View) => {
    setView(v);
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      // Storage may be blocked; the choice then lasts for this session only.
    }
  };
  const apply = (iv: Record<string, unknown>) => client.send({ type: "apply", intervention: iv });
  const selected = inspectData && !inspectData.gone && typeof inspectData.x === "number" ? { x: inspectData.x as number, y: inspectData.y as number } : null;

  const mapProps = world && {
    world: world.info,
    elevation: world.elevation,
    biome: world.biome,
    water: world.water,
    hydro: world.hydro,
    frame,
    frameVersion,
    speciesColors,
    layer,
    layerData,
    selected,
    selectedId: selected ? inspectId : null,
    tool,
    onPick: (x: number, y: number, r: number) => client.send({ type: "pick", x, y, radius: r }),
    onRegion: (rect: [number, number, number, number]) => {
      setTool("inspect");
      client.send({ type: "focus", rect });
    },
    onToolClick: (fx: number, fy: number) => {
      if (tool === "meteor") apply({ kind: "Meteor", fx, fy, radius: 0.06 });
      if (tool === "founders") apply({ kind: "AddFounders", group: { species: founderSpecies, count: founderCount, diversity: 0.8, region: [fx, fy, 0.04] } });
      setTool("inspect");
    },
  };

  if (!catalog) {
    return (
      <div className="loading">
        <div className="spinner" />
        <p>Loading the simulation engine…</p>
      </div>
    );
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden>
            <path d="M12 2C7 6 5 10 6 14c1 4 5 7 6 8 1-1 5-4 6-8 1-4-1-8-6-12z" fill="currentColor" opacity="0.9" />
            <path d="M12 6v14M12 12l-3-3M12 15l3-3" stroke="var(--bg)" strokeWidth="1.6" fill="none" />
          </svg>
          <span>Evolutions</span>
        </div>
        {world && status && (
          <div className="world-meta">
            <span className="world-name">{world.info.template}</span>
            <span className="date">{formatDate(status.year, status.day_of_year, status.hour)}</span>
            <span className={`badge mode-${status.mode.toLowerCase()}`} title="Individual: every animal simulated hourly. Population: groups simulated yearly with the same biology.">
              {status.mode}
              {status.focus ? " · region" : ""}
            </span>
            <span className="stat">
              <b>{status.population.toLocaleString()}</b> organisms
            </span>
            <span className="stat">
              <b>{status.species_alive}</b> species
            </span>
          </div>
        )}
        <div className="actions">
          <button onClick={() => setShowNew(true)}>New world</button>
          <button disabled={!world} onClick={() => client.send({ type: "save" })}>
            Save
          </button>
          <label className="button file">
            Open
            <input
              type="file"
              accept=".evo,.json"
              onChange={(e) => {
                const file = e.target.files?.[0];
                // Reset so choosing the same file again still fires onChange.
                e.target.value = "";
                if (file) void load(file);
              }}
            />
          </label>
        </div>
      </header>

      <main className="main">
        <section className="map-wrap">
          {mapProps ? (
            view === "3d" ? (
              <Suspense fallback={<div className="empty-map" />}>
                <WorldView3D
                  {...mapProps}
                  onUnsupported={() => {
                    chooseView("map");
                    notify("3D needs WebGL, which this browser does not provide. Showing the flat map.", true);
                  }}
                />
              </Suspense>
            ) : (
              <MapView {...mapProps} />
            )
          ) : (
            <div className="empty-map" />
          )}
          {world && (
            <div className="view-switch" role="radiogroup" aria-label="View">
              {(
                [
                  ["3d", "3D world"],
                  ["map", "Flat map"],
                ] as [View, string][]
              ).map(([k, label]) => (
                <button key={k} role="radio" aria-checked={view === k} className={view === k ? "active" : ""} onClick={() => chooseView(k)}>
                  {label}
                </button>
              ))}
            </div>
          )}
          {capExceeded && status?.mode === "Population" && (
            <div className="map-hint">
              {status.population.toLocaleString()} organisms is more than can be simulated one by one. Shift-drag on the map (or use <b>Watch region</b>) to watch a region
              individually.
            </div>
          )}
          {tool !== "inspect" && <div className="map-hint tool">{tool === "focus" ? "Drag a box to watch that region individually." : "Click on the map."} Esc to cancel.</div>}
          <div className="legend" aria-hidden>
            {species
              .filter((s) => s.extinct_year === null)
              .slice(0, 8)
              .map((s) => (
                <span key={s.id}>
                  <i className="swatch" style={{ background: `rgb(${s.color.join(",")})` }} />
                  {s.common_name}
                </span>
              ))}
          </div>
        </section>

        <aside className="side">
          <nav className="tabs" role="tablist">
            {(
              [
                ["species", "Species"],
                ["tree", "Tree"],
                ["events", "Chronicle"],
                ["inspect", "Inspector"],
                ["world", "World"],
              ] as [Tab, string][]
            ).map(([k, label]) => (
              <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "active" : ""} onClick={() => setTab(k)}>
                {label}
                {k === "events" && events.length > 0 && <span className="count">{events.length > 999 ? "999+" : events.length}</span>}
              </button>
            ))}
          </nav>
          <div className="tab-content">
            {tab === "species" && <SpeciesPanel species={species} year={status?.year ?? 0} catalog={catalog} selected={selectedSpecies} onSelect={setSelectedSpecies} detail={detail} />}
            {tab === "tree" && (
              <PhyloTree
                species={species}
                year={status?.year ?? 0}
                selected={selectedSpecies}
                onSelect={(id) => {
                  setSelectedSpecies(id);
                  setTab("species");
                }}
                onExport={() => client.send({ type: "newick" })}
              />
            )}
            {tab === "events" && (
              <EventsPanel
                events={events}
                species={species}
                onSelectSpecies={(id) => {
                  setSelectedSpecies(id);
                  setTab("species");
                }}
              />
            )}
            {tab === "inspect" && (
              <Inspector
                data={inspectData}
                onClose={() => {
                  setInspectId(null);
                  setInspectData(null);
                }}
              />
            )}
            {tab === "world" && (
              <WorldPanel
                catalog={catalog}
                status={status}
                species={species}
                layer={layer}
                setLayer={setLayer}
                densitySpecies={densitySpecies}
                setDensitySpecies={setDensitySpecies}
                alleleSpecies={alleleSpecies}
                setAlleleSpecies={(id) => {
                  setAlleleSpecies(id);
                  setAlleleUid(null);
                  setAlleles(null);
                }}
                alleles={alleles && alleles.species === alleleSpecies ? alleles.list : null}
                alleleUid={alleleUid}
                setAlleleUid={setAlleleUid}
                tool={tool}
                setTool={setTool}
                modeChoice={modeChoice}
                setMode={(m) => client.send({ type: "mode", mode: m })}
                apply={apply}
                founderSpecies={founderSpecies}
                setFounderSpecies={setFounderSpecies}
                founderCount={founderCount}
                setFounderCount={setFounderCount}
                onExportReplay={() => client.send({ type: "exportReplay" })}
                onReplay={() => client.send({ type: "replay" })}
                busy={!!progress}
              />
            )}
          </div>
        </aside>
      </main>

      <footer className="timebar">
        <button className="play" onClick={togglePlay} disabled={!world || !!progress} aria-label={playing ? "Pause" : "Play"} title="Space">
          {playing ? (
            <svg viewBox="0 0 24 24" width="20" height="20">
              <rect x="6" y="5" width="4" height="14" fill="currentColor" />
              <rect x="14" y="5" width="4" height="14" fill="currentColor" />
            </svg>
          ) : (
            <svg viewBox="0 0 24 24" width="20" height="20">
              <path d="M7 5l12 7-12 7z" fill="currentColor" />
            </svg>
          )}
        </button>
        <div className="speeds" role="radiogroup" aria-label="Speed">
          {SPEEDS.map((s) => (
            <button key={s.value} role="radio" aria-checked={speed === s.value} className={speed === s.value ? "active" : ""} onClick={() => setSpeedAndSend(s.value)}>
              {s.label}
            </button>
          ))}
        </div>
        <span className="achieved" title="Speed actually achieved">
          {playing ? `≈ ${describeSpeed(achieved)}` : "paused"}
        </span>
        <div className="jumps">
          <span className="muted">Jump ahead</span>
          {[100, 1000, 10000, 100000].map((y) => (
            <button key={y} disabled={!world || !!progress} onClick={() => client.send({ type: "jump", years: y })}>
              +{y >= 1000 ? `${y / 1000}k` : y} yr
            </button>
          ))}
          <RunUntil catalog={catalog} species={species} disabled={!world || !!progress} onRun={(years, until) => client.send({ type: "jump", years, until })} />
        </div>
      </footer>

      {progress && (
        <div className="progress-overlay" role="status">
          <div className="progress-card">
            <b>{progress.title ?? "Fast-forwarding…"}</b>
            <span>
              {progress.label} — {formatYears(progress.done)} of {formatYears(progress.total)}
            </span>
            <div className="meter-track energy">
              <div style={{ width: `${(100 * progress.done) / progress.total}%` }} />
            </div>
            <button onClick={() => client.send({ type: "cancel" })}>Stop here</button>
          </div>
        </div>
      )}

      <div className="toasts" aria-live="polite">
        {notices.map((n) => (
          <div key={n.id} className={`toast ${n.error ? "error" : ""}`}>
            {n.text}
          </div>
        ))}
      </div>

      {showNew && <NewWorldDialog catalog={catalog} onCreate={create} onLoad={load} autosave={world ? null : autosave} onRestore={restore} onClose={world ? () => setShowNew(false) : undefined} busy={busy} />}
    </div>
  );
}
