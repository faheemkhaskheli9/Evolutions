# Evolutions — 3D World: System Design

| Field | Value |
|---|---|
| Document | System Design |
| Feature | A single 3D world that contains the whole simulation |
| Version | 0.1 |
| Date | 2026-10-19 |
| Status | Implemented (v0.2) |
| Related | [Plan](./3D_WORLD_PLAN.md) · [3D World PRD](./3D_WORLD_PRD.md) · [3D World TRD](./3D_WORLD_TRD.md) · [Product System Design](./SYSTEM_DESIGN.md) |

---

## 1. Overview

The 3D world is a second view over the data the app already has. The engine (Rust/WASM in a Web Worker) is unchanged; it sends the same frame snapshots it sends to the flat map. The UI turns those snapshots into one three.js scene.

```text
 Web Worker                     Main thread
 ┌─────────────┐  frame msg    ┌──────────────────────────────────────────────────────┐
 │ engine      │──────────────▶│ App.tsx                                              │
 │ (WASM)      │ positions,    │  frame ref + frameVersion, layer, tool, selection    │
 │             │ species,      │        │ MapProps (identical for both views)         │
 │             │ actions,      │        ▼                                             │
 │             │ density,      │  view === "3d" ? <WorldView3D/> : <MapView/>         │
 │             │ dominant,     │        │                                             │
 │             │ focus, mode   │        ▼                                             │
 └─────────────┘               │  WorldView3D.tsx  ── input, React wiring             │
        ▲                      │        │                                             │
        │ pick / focus /       │        ▼                                             │
        │ apply messages       │  render/world3d.ts (World3D) ── scene, camera,       │
        └──────────────────────│        picking, layout, on-demand rendering          │
                               │        │                                             │
                               │  components/mapLayers.ts ── layer images (shared     │
                               │        with the flat map)                            │
                               └──────────────────────────────────────────────────────┘
```

## 2. Components

| File | Responsibility |
|---|---|
| `src/components/mapLayers.ts` | Builds one-pixel-per-tile images: `buildTerrain` (biomes, relief shading, water by depth), `buildScalar` (temperature, rainfall, vegetation), `buildDemes` (species ranges or density colours). Used by both views. |
| `src/render/world3d.ts` | `World3D`: owns the `WebGLRenderer`, scene, camera state, meshes and ray-casting. Framework-free; React never touches three.js objects directly. |
| `src/components/WorldView3D.tsx` | React component: creates and disposes `World3D`, feeds it world and frame data, turns pointer and wheel events into camera moves, picks, region boxes and tool clicks. |
| `src/components/MapView.tsx` | The flat map; exports `MapProps`, the contract both views share. |
| `src/App.tsx` | Chooses the view (stored in `localStorage`), lazy-loads `WorldView3D`, falls back to the flat map without WebGL. |

## 3. Data flow

1. **World creation.** The worker sends elevation, biome and water grids once. `WorldView3D` calls `World3D.setWorld(w, h, elevation)`, which builds the terrain geometry, sea and open-sea strips and frames the camera. `buildTerrain` runs once per world (memoised).
2. **Each frame.** The worker posts a frame; `App` stores it in a ref and bumps `frameVersion`. `WorldView3D`'s effect then:
   - composites the layer canvas (terrain → scalar layer → species ranges) and hands it to `setTexture` (one texture upload);
   - calls `setHerds(density, dominant, …)`, `setAnimals(positions, species, actions, …)` and `setFocus(rect)`; these only store references and mark the layout dirty;
   - requests a render.
3. **Render.** At most once per animation frame, `World3D.render()` updates the camera, re-lays out animals and herds if the data, zoom (> 5%) or camera x (> w/8) changed, places boxes and the selection ring, and draws.
4. **Input.** Pointer events become camera changes (pan, orbit, zoom) or messages to the worker (`pick`, `focus`, `apply` for meteor and founders), exactly as with the flat map.

Rendering is on demand: nothing redraws while the simulation is paused and the camera is still.

## 4. Scene contents

| Object | three.js type | Notes |
|---|---|---|
| Terrain (×3 copies) | `Mesh` sharing one indexed `BufferGeometry` + `MeshLambertMaterial` with the layer `CanvasTexture` | Copies at x = −w, 0, +w give a seamless east–west wrap. One vertex per tile centre plus a wrap column. |
| Sea | `Mesh` (plane), translucent, no depth write | At y = −0.02 over three world widths. |
| Open sea | Two `Mesh` strips (children of the sea) | North and south of the world, at the mean ocean depth and colour, so the edges disappear. |
| Individuals | `InstancedMesh` (detailed or simple body) | One instance per animal; colour per species; crouched when resting; yaw hashed from index. Geometry switches to the 36-triangle body when a figure is under 16 px long. |
| Herds | `InstancedMesh` (simple body, opacity 0.8) | 1–4 figures per occupied tile, in the dominant species' colour; skipped inside the watched region. |
| Density columns | Same mesh slot, unit box | Height and heat-ramp colour from log density. Replaces herds on the density layer. |
| Watched region | `LineSegments` (box edges), white | |
| Region preview | `LineSegments`, yellow | While dragging. |
| Selection | `Mesh` (ring), drawn on top | Radius scales with zoom. |
| Lights | `HemisphereLight` + `DirectionalLight` | Sun from the north-west. |
| Atmosphere | Background colour + `Fog` | Fog distances scale with camera distance. |

### 4.1 Heights

```text
height(tile) = elevation ≥ 0 ? elevation × RELIEF : elevation × RELIEF × SEA_FLOOR
RELIEF = 1/1500 tiles per metre, SEA_FLOOR = 0.12
heightAt(tx, ty) = bilinear between tile centres, x wrapping, y clamped, never below 0
```

Animals, herds and the selection ring stand at `heightAt`, so they sit on the terrain and float on the sea surface over water.

### 4.2 Sizes and level of detail

```text
unitsPerPixel = 2 · distance · tan(FOV/2) / canvasHeight
bodySize      = clamp(9 · unitsPerPixel, 0.12, 4)    (≥ ~9 px on screen)
detailed body when bodySize / unitsPerPixel ≥ 16 px, else simple body
herds always use the simple body
```

Meshes are reused across frames and grow by 30% when full, so steady-state frames upload only instance matrices and colours.

## 5. Camera

The camera orbits a target point on the ground:

```text
position = target + distance · (cos(pitch)·sin(yaw), sin(pitch), cos(pitch)·cos(yaw))
target y = half the terrain height under the target (keeps mountains in view)
pitch ∈ [18°, 88°], distance ∈ [1.2, 1.6 · max(w, h)]
target x wraps into [0, w); target z clamped to [0, h]
```

- **Pan** keeps the ground point first pressed under the pointer (ray–plane intersection at sea level). When the target wraps, the drag anchor is shifted by the same world width.
- **Zoom** scales the distance and moves the target toward the ground point under the pointer, so zooming feels anchored.
- **Orbit** changes yaw and pitch.
- **Fit** (on world load) looks north at the world centre from 62°, far enough for the whole world to fit.

## 6. Picking

| Action | Method |
|---|---|
| Click to inspect | Ray from the camera through the pointer, intersected with the three terrain meshes (fallback: plane at sea level). The hit's x is wrapped into [0, w) and sent as `pick` with a radius of `max(0.3, 8 · unitsPerPixel)` tiles; the engine returns the nearest animal. |
| Meteor / introduce species | Same ray-cast; the point is sent as fractions of the world (`fx = tx/w`, `fy = ty/h`). |
| Watch region | Both corners ray-cast in unwrapped x; normalised so the box may cross the east–west seam, then sent as integer tile bounds with `focus`. |
| Tests | `window.__evoMap.toScreen(tx, ty)` projects a tile point to client pixels, so end-to-end tests click real animals. |

## 7. Lifecycle and failure handling

- `World3D` is created when the canvas mounts and disposed on unmount (renderer, textures, geometries, instanced meshes).
- If `new WebGLRenderer` throws, `WorldView3D` calls `onUnsupported`; `App` stores the flat-map choice and shows a notice.
- A `ResizeObserver` keeps the drawing buffer and camera aspect in step with the layout.
- three.js is lazy-loaded; `Suspense` shows an empty map area while the chunk loads.

## 8. Performance

| Concern | Design |
|---|---|
| Many animals | One instanced draw call; simple body when small on screen. |
| Many herds | Simple body, reused mesh, layout only when data or zoom changes. |
| Texture updates | One w×h canvas upload per frame (a few hundred kB at most). |
| Idle | No rendering unless something changed. |
| Download | three.js in its own chunk (~510 kB, ~130 kB gzipped), not loaded by the flat map. |

## 9. Extension points

- **Smooth movement:** keep the previous positions and interpolate in `render()` between frames.
- **Phenotype bodies:** extend the frame message with per-animal body mass and leg length, and scale instances non-uniformly.
- **Lighting by time:** drive the sun direction and hemisphere colours from the engine clock, and add a snow tint from the temperature layer.
- **Level of detail for very large counts:** switch distant instances to camera-facing sprites.
