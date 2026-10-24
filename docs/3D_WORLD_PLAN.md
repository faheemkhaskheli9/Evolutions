# 3D world — implementation plan

| Field | Value |
|---|---|
| Document | Implementation plan |
| Feature | A single 3D world that contains the whole simulation |
| Date | 2026-10-19 |
| Status | Phases 1–8 done |
| Related | [3D World PRD](./3D_WORLD_PRD.md) · [3D World TRD](./3D_WORLD_TRD.md) · [3D World System Design](./3D_WORLD_SYSTEM_DESIGN.md) |

---

## Goal

Replace the flat map with one 3D scene in which the user sees everything the simulation holds: relief, seas, climate, every animal simulated one by one, the herds simulated as populations, the watched region and the selected animal. Every existing tool (inspect, watch region, meteor, introduce species, map layers) keeps working in the 3D view. The flat map stays available as a fallback and for users who prefer it.

The engine does not change. The 3D world is a new view over the same frame data the flat map already receives.

## Phases

### Phase 1: Shared layer images (done)

| # | Task | Result |
|---|---|---|
| 1.1 | Move the terrain, climate-layer and species-range image builders out of `MapView.tsx` into `components/mapLayers.ts`. | The flat map and the 3D world draw the same colours from the same code. |
| 1.2 | Export the flat map's props as `MapProps`. | Both views take identical props, so the app switches between them without other changes. |

### Phase 2: The scene (done)

| # | Task | Result |
|---|---|---|
| 2.1 | Add `three` 0.170.0 (exact version) and its types. | |
| 2.2 | `render/world3d.ts`: terrain mesh with one vertex per tile, heights from elevation (land 1 tile per 1,500 m; sea floor flattened to 12%). | Mountains, coasts and basins are visible in relief. |
| 2.3 | Drape the layer image over the terrain as a texture. | Biomes, temperature, rainfall, vegetation and species ranges all appear on the 3D surface. |
| 2.4 | Translucent sea at sea level; open sea beyond the north and south edges at the world's mean ocean depth and colour. | The world reads as land in an ocean, with no visible edge. |
| 2.5 | Three terrain copies (west, centre, east) and animals placed on the copy nearest the camera. | East–west wrapping is seamless while panning. |
| 2.6 | Animals as one instanced mesh (a small four-legged body), coloured by species, facing varied directions, crouched when resting. | Thousands of animals in one draw call. |
| 2.7 | Population-mode herds: 1–4 translucent figures per occupied tile in the dominant species' colour; on the density layer, columns whose height is log density. | Life simulated as populations is visible too, and drawn so it is not mistaken for individuals. |
| 2.8 | Watched-region box, drag preview box, and a selection ring. | |
| 2.9 | Bodies scale with zoom so that a herd stays visible from far away (at least ~9 px long). | |

### Phase 3: Controls and app integration (done)

| # | Task | Result |
|---|---|---|
| 3.1 | `components/WorldView3D.tsx`: drag to pan (the ground follows the pointer), right-drag or Ctrl/Alt-drag to orbit, scroll to zoom toward the pointer, click to inspect, shift-drag to watch a region. | Same gestures as the flat map plus orbit. |
| 3.2 | Meteor and introduce-species clicks are ray-cast onto the terrain. | Tools act where the user clicks in 3D. |
| 3.3 | View switch (3D world / Flat map) over the map, remembered in local storage. | |
| 3.4 | Fall back to the flat map with a notice when WebGL is unavailable. | |
| 3.5 | Load three.js lazily (its own chunk). | The main bundle stays at ~190 kB; the 3D chunk (~510 kB, 130 kB gzipped) loads only when shown. |
| 3.6 | Keep the `window.__evoMap` test hook (`toScreen`, `firstAnimal`) in the 3D view. | The end-to-end smoke test and the guide-screenshot script run unchanged against the 3D view. |

### Phase 4: Quality (done)

| # | Task | Result |
|---|---|---|
| 4.1 | Reuse the herd instanced mesh between frames instead of reallocating it. | Population mode no longer rebuilds GPU buffers every frame. |
| 4.1b | Level of detail: a 36-triangle body for herds and for animals shorter than 16 px on screen (the detailed body is ~290 triangles). | Population mode drew tens of millions of triangles per frame; it now draws about an eighth of that, which keeps the page responsive on weak GPUs. |
| 4.2 | End-to-end smoke test passes with the 3D view as the default. | |
| 4.3 | User guide and README describe the 3D world, with screenshots. | |

### Phase 5: Life and time in the scene (done)

| # | Task | Result |
|---|---|---|
| 5.1 | The engine sends animal ids with each frame. Each animal eases from where it is drawn to its new position over the time between position updates, faces the way it walks, and swings its legs (diagonal pairs in opposite phase) while it moves. A move over 3 tiles snaps. | Animals walk instead of stepping from frame to frame. |
| 5.2 | The engine sends four shape values per animal (current mass, leg length, ornament, coat luminance). Body length scales with the cube root of mass relative to the frame's geometric mean (0.7–1.6×); legs stretch and lift the body; the tail plume grows with the ornament and is drawn orange; the coat lightens or darkens the species colour. One shader does this for all instances. | Evolved and individual differences, and the young, are visible directly. |
| 5.3 | Day–night light from the engine's clock: the sun rises in the east, crosses the south and sets in the west; sky and fog go through dusk to night. Shown up to ~1 hour/s, fading out by 6 hours/s. Snow cover is sent when the day changes (below freezing with enough precipitation, as for camouflage) and laid over land and lakes. | Time of day and the seasons are visible in the scene. |
| 5.4 | The engine exposes D8 flow directions (`downstream`) and the filled water surface (`water_level`). Rivers are ribbons from each river tile to the tile it drains into, wider with log flow; lakes are flat surfaces at their level. River tiles in the 3D texture show the land they cross. | Rivers and lakes read as water, not as blue tiles. |
| 5.5 | Above 20,000 figures, small animals and herd figures are drawn as one sprite each (a silhouette, tinted per figure). | Large worlds stay responsive on weak GPUs. |
| 5.6 | **Follow animal** button for the inspected animal; dragging, a new selection or its death stops it. The selection ring tracks the animal smoothly. | |

### Phase 6: Seasons, plants and paths (done)

| # | Task | Result |
|---|---|---|
| 6.1 | The engine adds `axial_tilt` to the world info. The sun is placed from the latitude at the camera target, the solar declination for the day of the year (furthest north on day 172) and the hour angle. Above ~1 hour/s the light is the day's mid-morning sun, dimmed when that sun is below the horizon. | Seasons show in the light: low winter sun, midnight sun and polar night beyond the polar circles. |
| 6.2 | The worker sends the `forage` layer about once a second. Conifers, broadleaf trees and grass tufts are instanced around the camera target (up to 70 tiles away) once a tile is 10 px across, with counts per tile by biome. Grass is drawn in proportion to forage and dries from green to straw; trees keep at least half their number. Half the plants are drawn until a tile is 30 px across. Plants show on the Terrain and Vegetation layers. | Grazing pressure is visible in the scene. |
| 6.3 | The engine adds `days` (days since the start) to the status. The 3D view records the inspected animal's reported positions and draws them as a line over the last simulated day, fading with age and ending at the animal as drawn. A new selection clears it. | Where the animal has been is visible at a glance. |

### Phase 7: Seasons on the plants, and shadows (done)

| # | Task | Result |
|---|---|---|
| 7.1 | The snow cover (already sent once a day) is passed to the plants. Snow whitens tree crowns and buries grass (thinned from 30% cover, gone at 70%). Broadleaf trees in temperate grassland, forest and rainforest beyond the tropics follow the season of their hemisphere: green in summer, gold or red (per tree, a few days apart) from mid-September, bare from mid-November, light green as the leaves come out in April–May. The plants are laid out again when the day changes, at most four times a second. | The seasons show on the plants as well as on the ground. |
| 7.2 | The sun casts PCF soft shadows (2,048² map). The shadow camera covers a square about as wide as the view around the camera target, so shadows are sharp close up and still cover the screen when zoomed out; the normal bias grows with the texel size. Terrain, trees and grass cast and receive shadows; animals cast them only when drawn at true size (bodies enlarged to stay visible do not). Shadows fade out with the light at dusk. | Low winter sun reads as such: long shadows behind trees and mountains. |

### Phase 8: Shadows that match, and wind (done)

| # | Task | Result |
|---|---|---|
| 8.1 | Animals cast their shadows through a depth material bent by the same shader as their bodies (leg length, swinging legs, ornament plume). | Shadows match the drawn shape exactly. |
| 8.2 | Each plant vertex carries how far it bends (0 at the base, growing with height²; grass bends most, conifers least). The vertex shader pushes plants downwind (from the west) with a sway that rolls across the land and slow gusts, on the wall clock; the shadow depth material sways the same way. The scene keeps drawing while plants are shown. | The scene feels alive when time is paused. |

## Testing

- `npx tsc --noEmit` and `npx vite build` must pass.
- `npm run test:e2e` runs the whole smoke test in the 3D view (create, play, inspect, jump, layers, run-until, watch region, interventions, autosave).
- `npm run screenshots` regenerates the user-guide screenshots in the 3D view.

## Risks

| Risk | Mitigation |
|---|---|
| No WebGL (old hardware, locked-down browsers) | Automatic fallback to the flat map with a notice. |
| Large worlds with many herds drop frames | Instancing, mesh reuse, layout only when data or zoom changes (or while animals move); sprites above 20,000 figures (5.5). |
| Plants add many triangles | Low-poly plants (12–28 triangles), only near the camera target and only when zoomed in, half as many until tiles are large, at most 80,000 of each kind. |
| Shadows draw the scene twice | One 2,048² map fitted to the view; sprites and herds cast none. |
| Wind redraws every frame | Only while plants are shown (zoomed in near the camera target); plants are low-poly and capped. |
| 3D picking is less precise than the flat map | Pick radius scales with zoom; zooming in makes selection exact. |
