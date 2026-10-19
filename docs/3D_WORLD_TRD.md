# Evolutions — 3D World: Technical Requirements Document (TRD)

| Field | Value |
|---|---|
| Document | Technical Requirements Document |
| Feature | A single 3D world that contains the whole simulation |
| Version | 0.1 |
| Date | 2026-10-19 |
| Status | Implemented (v0.2) |
| Related | [Plan](./3D_WORLD_PLAN.md) · [3D World PRD](./3D_WORLD_PRD.md) · [3D World System Design](./3D_WORLD_SYSTEM_DESIGN.md) · [Product TRD](./TRD.md) |

---

## 1. Scope

This document turns the [3D World PRD](./3D_WORLD_PRD.md) into technical requirements for the browser app (`app/web/`). The engine, the worker protocol and the save format do not change.

## 2. Technology

| Area | Requirement |
|---|---|
| Rendering library | three.js **0.170.0**, pinned exactly (`"three": "0.170.0"`, `"@types/three": "0.170.0"`). No other 3D dependency. |
| Graphics API | WebGL 2 through `THREE.WebGLRenderer` (antialiased, sRGB output). |
| Bundling | The 3D view is imported with `React.lazy`, so three.js is a separate chunk loaded only when the 3D view is shown. |
| Language | TypeScript (strict), same as the rest of the app. |

## 3. Coordinate system and scale

| Item | Requirement |
|---|---|
| Units | 1 scene unit = 1 tile. |
| Axes | x east (0…w, wrapping), z south (0…h; row 0 is the northern edge), y up. |
| Animals | Engine positions are already in tile units: animal (tx, ty) is the scene point (tx, height(tx, ty), ty). |
| Land relief | `RELIEF = 1/1500` tiles per metre (5,000 m ≈ 3.3 tiles). |
| Sea floor | Below sea level, heights are multiplied by `SEA_FLOOR = 0.12`. |
| Height lookup | Bilinear between tile centres, wrapping in x, clamped in z, never below 0 for objects standing on the surface. |

## 4. Scene requirements

| ID | Requirement | PRD |
|---|---|---|
| T-1 | Terrain: one indexed `BufferGeometry` with one vertex per tile centre plus one wrap column; vertex normals computed once per world. | W-1 |
| T-2 | East–west wrap: three meshes share the terrain geometry at x offsets −w, 0, +w. Animals, herds and boxes are placed on the copy nearest the camera target (`nearest(x) = x − round((x − cam.tx)/w)·w`). | W-4 |
| T-3 | Layer texture: a `CanvasTexture` built from a w×h canvas (one pixel per tile) composited from the terrain image, the chosen scalar layer and species ranges. `RepeatWrapping` in s, linear filtering, no mipmaps. Rebuilt only when the frame or layer changes. | W-5, W-6 |
| T-4 | Sea: translucent plane (opacity 0.55, no depth write) at y = −0.02 spanning three world widths. Open-sea strips north and south of the world at the mean ocean depth of the world, coloured with the terrain image's depth ramp at that depth. | W-2, W-3 |
| T-5 | Animals: one `InstancedMesh` of a merged low-poly body (capsule body, sphere head, four legs; ~1 unit long). Capacity grows by 30% when exceeded and is never reallocated per frame. Per-instance colour from the species palette. Resting (`action == 0`) scales height by 0.55. Yaw from a hash of the index. | L-1, L-2 |
| T-6 | Body size: `clamp(9 × unitsPerPixel, 0.12, 4)` scene units, so bodies are ≥ ~9 px on screen when zoomed out. Layout is redone only when data changes, size changes by > 5%, or the camera moves more than w/8 in x. | L-3 |
| T-7 | Herds: one reused `InstancedMesh` (opacity 0.8). Per occupied tile, `1 + floor(3t)` figures where `t = log1p(density)/log1p(max)`, jittered within the tile, in the dominant species' colour. Tiles inside the watched region are skipped. | L-4, L-6 |
| T-8 | Density columns: same reused mesh with a unit box, height `0.1 + 1.6t`, colour from the heat ramp (navy → magenta → amber). | L-5 |
| T-9 | Region boxes: `LineSegments` box edges (watched region white, drag preview yellow), not fogged. | I-5 |
| T-10 | Selection: yellow ring on the ground, radius `max(0.25, 14 × unitsPerPixel)`, drawn on top (no depth test). | I-4 |
| T-11 | Lighting: hemisphere light (sky/ground) plus one directional sun from the north-west. Sky-coloured background and linear fog scaled with camera distance. | — |

## 5. Camera and input

| ID | Requirement | PRD |
|---|---|---|
| C-1 | Orbit camera defined by target (tx, ty), distance, yaw and pitch. Pitch limited to [18°, 88°]; distance to [1.2, 1.6 × max(w, h)]. Target x wraps into [0, w); target z clamped to [0, h]. | I-2 |
| C-2 | Initial framing: target at the world centre, yaw 0 (looking north), pitch 62°, distance chosen so the whole world fits. | I-8 |
| C-3 | Near/far planes scale with distance (`near = max(0.02, d/200)`, `far = 8d + 200`). | — |
| C-4 | Pan: the ground point under the pointer at press stays under the pointer (ray–plane intersection at y = 0). | I-1 |
| C-5 | Zoom: factor `exp(deltaY × 0.0015)`, moving the target toward the ground point under the pointer. | I-3 |
| C-6 | Orbit: right button, or left button with Ctrl or Alt; 0.006 rad per pixel. The context menu is suppressed on the canvas. | I-2 |
| C-7 | Click (moved < 4 px): ray-cast against the terrain copies (falling back to the sea-level plane); wrap x into [0, w) and send `pick` with radius `max(0.3, 8 × unitsPerPixel)`, or the meteor / founders tool click as fractions of the world. | I-4, I-6 |
| C-8 | Region drag: corners from terrain ray-casts in unwrapped x; the box is normalised so x0 ∈ [0, w) and x1 ≤ x0 + w − 1, then sent as integer tile bounds. | I-5 |
| C-9 | Rendering is on demand (`requestAnimationFrame` only after a change), not a continuous loop. | N-1 |

## 6. Integration

| ID | Requirement |
|---|---|
| X-1 | `WorldView3D` takes exactly the flat map's `MapProps` plus `onUnsupported`. `App` passes the same props object to whichever view is active. |
| X-2 | The view choice is stored in `localStorage` under `evolutions.view` (`"3d"` default, or `"map"`); storage errors are ignored. |
| X-3 | If creating the WebGL renderer throws, the app switches to the flat map and shows: "3D needs WebGL, which this browser does not provide. Showing the flat map." |
| X-4 | The canvas keeps the `map` class and exposes `window.__evoMap = { toScreen, firstAnimal }` so tests drive both views the same way. |
| X-5 | The worker frame message carries `actions` (one byte per animal, from `frame_actions()`) alongside positions and species. |
| X-6 | The renderer, textures and instanced meshes are disposed when the view unmounts. |

## 7. Performance requirements

| ID | Requirement |
|---|---|
| P-1 | One draw call for all individuals and one for all herds or columns. |
| P-2 | No GPU buffer allocation per frame in steady state (meshes reused; only instance matrices and colours updated). |
| P-3 | Targets: 60 fps at 5,000 individuals and ≥ 30 fps at 25,000 on a mid-range laptop GPU (PRD N-1). |
| P-4 | Initial JavaScript for the flat-map path does not include three.js. |

## 8. Testing

| ID | Requirement |
|---|---|
| Q-1 | `npm run typecheck` and `npm run build` pass. |
| Q-2 | `npm run test:e2e` passes with the 3D view as the default (all steps, including inspect, watch region and interventions). |
| Q-3 | `npm run screenshots` regenerates the user-guide images in the 3D view. |
