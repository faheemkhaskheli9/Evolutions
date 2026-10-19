# Evolutions — 3D World: Product Requirements Document (PRD)

| Field | Value |
|---|---|
| Document | Product Requirements Document |
| Feature | A single 3D world that contains the whole simulation |
| Version | 0.1 |
| Date | 2026-10-19 |
| Status | Implemented (v0.2), phase 5 proposed |
| Related | [Plan](./3D_WORLD_PLAN.md) · [3D World TRD](./3D_WORLD_TRD.md) · [3D World System Design](./3D_WORLD_SYSTEM_DESIGN.md) · [Product PRD](./PRD.md) |

---

## 1. Summary

Evolutions shows its world as a flat, top-down map. This feature puts the whole simulation into **one 3D world**: a landscape with real relief and seas, on which every animal stands, walks and rests, and over which the user can fly, orbit and zoom. Everything the simulation contains is visible in that one scene: terrain and biomes, climate layers, individual animals, populations, species ranges, the watched region and the selected animal. All existing tools keep working in it.

**One-line pitch:** *"Fly over the living world you created and watch evolution happen on the ground."*

## 2. Problem

- A flat map hides the cause of much of what happens. Mountains split populations, rain shadows make deserts and coasts limit dispersal, but on a flat map these are only colours.
- Individuals are dots. It is hard to feel that a herd is a group of animals, or to compare two species side by side.
- The app has several separate visual surfaces (map, layer overlays, density colours, region box). Users must assemble them in their heads.

## 3. Goals and non-goals

### 3.1 Goals

| ID | Goal |
|---|---|
| G1 | One scene shows the whole world and everything living in it, at any time scale. |
| G2 | Relief is visible: mountains, valleys, coasts and sea basins. |
| G3 | Individual animals look like animals, coloured by species, with visible resting behaviour. |
| G4 | Life simulated as populations (Population mode) is also visible in the scene, and is not mistaken for individuals. |
| G5 | Every existing tool works in 3D: inspect, watch region, map layers, meteor, introduce species. |
| G6 | Navigation is easy for a first-time user: pan, zoom and orbit with the mouse. |
| G7 | It runs smoothly in a normal laptop browser and never blocks someone without 3D support. |

### 3.2 Non-goals (this version)

- Photorealistic rendering, detailed animal models, or per-animal animation.
- Changing the engine or the scientific models. The 3D world is a view only.
- VR, first-person walking, or a desktop build.

## 4. Users and scenarios

| Persona (from the [product PRD](./PRD.md)) | Scenario |
|---|---|
| Student | Creates *Island Dwarfism Lab*, orbits to see the islands rise from the sea, and watches hares on a small island over 10,000 years. |
| Teacher | Shows a class the rain shadow behind a mountain range with the rainfall layer draped over the relief. |
| Curious adult | Zooms into a herd of deer resting on a plain, clicks one, and reads its genes and state. |
| Researcher | Switches to the density layer to see population columns rise and fall across the continent after a meteor. |

## 5. Functional requirements

Priorities: **P0** must have, **P1** should have, **P2** could have.

### 5.1 World

| ID | Requirement | Priority |
|---|---|---|
| W-1 | Terrain is a 3D surface whose height follows the world's elevation, with the vertical scale exaggerated enough to see mountain ranges. | P0 |
| W-2 | The sea is a translucent surface at sea level; the sea floor is visible through it but flattened so oceans read as basins. | P0 |
| W-3 | The world appears surrounded by open sea, with no visible edge at the north and south. | P1 |
| W-4 | The world wraps east–west: panning past the edge continues without a seam. | P0 |
| W-5 | Biome colours are draped over the terrain; the temperature, rainfall, vegetation and population-density layers replace them when chosen. | P0 |
| W-6 | Species ranges (the dominant species per area) are shown on the terrain and density layers in Population mode and outside a watched region. | P1 |

### 5.2 Life

| ID | Requirement | Priority |
|---|---|---|
| L-1 | Each animal simulated individually is drawn as a small four-legged figure standing on the terrain, coloured by species. | P0 |
| L-2 | Figures face different directions; resting animals are drawn crouched. | P1 |
| L-3 | Figures stay visible when zoomed out (minimum on-screen size), and shrink to natural scale when zoomed in. | P0 |
| L-4 | In Population mode each occupied area shows 1–4 translucent figures in its dominant species' colour, more where density is higher. | P0 |
| L-5 | On the density layer each occupied tile is a column whose height and colour show log density. | P1 |
| L-6 | Inside a watched region, animals are drawn individually; outside it, as populations. | P0 |

### 5.3 Interaction

| ID | Requirement | Priority |
|---|---|---|
| I-1 | Drag pans the view, with the ground following the pointer. | P0 |
| I-2 | Right-drag (or Ctrl/Alt-drag) orbits; the camera angle is limited between 18° and 88° above the horizon. | P0 |
| I-3 | Scroll zooms toward the point under the pointer. | P0 |
| I-4 | Click selects the nearest animal and opens it in the Inspector; the selected animal is marked with a ring. | P0 |
| I-5 | Shift-drag, or the **Watch region** tool, draws a box on the ground and starts a watched region. | P0 |
| I-6 | Meteor and **Place** (introduce species) act at the clicked ground point. | P0 |
| I-7 | A **3D world / Flat map** switch over the view; the choice is remembered on that device. | P0 |
| I-8 | The view opens framed on the whole world, seen from the south at 62°. | P1 |

### 5.4 Fallback and accessibility

| ID | Requirement | Priority |
|---|---|---|
| A-1 | Without WebGL the app shows the flat map and says why. | P0 |
| A-2 | The 3D canvas has an accessible label listing its controls. | P1 |
| A-3 | All information in the scene is also available in the side panels (species, Inspector, layers). | P0 |

## 6. Non-functional requirements

| ID | Requirement |
|---|---|
| N-1 | 60 fps on a mid-range laptop GPU with 5,000 individuals; at least 30 fps with 25,000 (the Individual-mode cap) and in Population mode on the largest template. |
| N-2 | The 3D code does not add to the initial download for users of the flat map (loaded on demand). |
| N-3 | Switching views keeps the simulation running and loses no state. |
| N-4 | The end-to-end smoke test passes in the 3D view. |

## 7. Success metrics

- At least 80% of sessions keep the 3D view (it is the default).
- Users find and use orbit within their first session (orbit used in at least 50% of sessions).
- No increase in "cannot select an animal" feedback compared with the flat map.

## 8. Release

- **v0.2 (this release):** all P0 and P1 requirements above. 3D is the default view; the flat map stays one click away.
- **Later:** smooth movement and walk cycles, phenotype-driven body shape, day–night and seasonal lighting, rivers as water, level of detail, camera follow. See [plan phase 5](./3D_WORLD_PLAN.md#phase-5-next-proposed).
