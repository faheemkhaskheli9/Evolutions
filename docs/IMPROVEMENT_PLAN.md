# Improvement plan

This plan comes from an audit of the engine and app against the PRD and TRD. Items marked **done** are implemented, and each engine fix has a regression test in `engine/evo-engine/tests/validation.rs`.

## Phase 1: Correctness (done)

| # | Problem | Fix |
|---|---|---|
| 1 | Founders created at year 0 got age 0 because `birth_minute` saturated at 0. Population mode then treated every founder as a newborn, delaying breeding and skewing ageing. | `Animal::birth_minute` is now `i64`, so founders can be born before the world clock started. Saves stay compatible because the field is the same width. |
| 2 | Leaving a focus region created duplicate `(species, patch)` demes for animals that had walked out of the focus area. | Frozen demes are restored before the animals are absorbed, so those animals merge into the existing demes. |
| 3 | Meteor and Plague skipped frozen demes during a focus. A plague on an unknown species reported success. | Both interventions now include frozen demes. An unknown species id returns an error. |
| 4 | Newick export nested daughters oldest-innermost, which gave wrong topology and branch lengths for a parent with two or more daughters. | Splits now nest from the newest outwards, with each branch measured back to the previous split. |
| 5 | Extinct species showed every trait as 0, because the final empty sample was used. | The species list and the trait table use the last non-empty sample. `trait_table` returns `null` for an unknown id instead of panicking. |

## Phase 2: Web-app robustness (done)

- An exception in `sendFrame` no longer stops the worker loop. The loop re-arms in `finally`.
- Creating or loading a world builds the new engine before freeing the old one, so a bad `.evo` file keeps the current world.
- A WASM trap (Rust panic) drops the dead engine and asks the user to create or open a world.
- Stale `speciesDetail` requests after a world change are ignored.

## Phase 3: Performance (partly done)

- **Done:** the seasonal cosine is computed once per day or month instead of once per tile (`World::season_cos` and `World::temp_season`).
- **Done:** Population-mode display vegetation regrows in 12 monthly steps instead of 36 ten-day steps.
- **Done:** the predator release no longer rescans the whole prey map for every tile (O(tiles²) to O(tiles)).
- **Result:** a 300-year `earth` run is about 20% faster.
- **Done:** profiling (callgrind, 100-year `earth` run) showed Individual mode taking ~85% of the time, led by `tiles_around` and `Grid::build`. `tiles_around` now returns a fixed-size inline list and wraps each column once instead of once per tile, and `Grid::build` reuses its scratch buffers. A 30-year `earth` run executes 13% fewer instructions and saves a byte-identical world.
- **Next:** cache `patch_contexts()` between years and invalidate it on a climate shift (not yet measured separately, because it is inlined into `step_population`).
- **Next:** store `cap_of` per deme in predation.
- **Next:** limit species-check comparisons to nearby demes.

## Phase 4: Missing MVP features (mostly done)

| Requirement | Status |
|---|---|
| **FR-W1** world-generator settings | **Done.** "Customise world settings" in the new-world dialog sets size, land shape, land fraction, roughness, temperature, rainfall, axial tilt and latitude range, starting from the chosen template. The catalog now includes each template's settings. The engine rejects invalid settings (inverted or out-of-range latitudes, land fraction outside 0–1, non-finite values); see `custom_world_settings_apply_and_are_validated`. |
| **FR-T3** run until | **Done.** **Run until…** in the time bar fast-forwards until a speciation, an extinction, a population (all life or one species) or a species' mean trait crosses a threshold, within a year limit. A condition that already holds runs the full length and says so, instead of stopping at once. |
| **FR-P1** autosave | **Done.** The worker saves the world to IndexedDB after creating or loading it, after every fast-forward, and once a minute while it changes. At startup the new-world dialog offers **Resume**. Autosave is best-effort: storage errors are ignored. |
| **FR-O1** per-species map layers | **Density done.** The Population density layer has a species selector. This also fixes the layer showing an empty map in Individual mode, because the worker sent density only in Population mode or during a focus. **Next:** allele-frequency layers. |
| **FR-P2** replay log | **Next.** Record interventions and mode switches with their timestamps, so a run from the same seed replays identically in the browser. |

The e2e smoke test (`app/web/e2e/smoke.mjs`) covers custom settings, a single-species density layer, **Run until**, and resuming the autosave after a reload.
