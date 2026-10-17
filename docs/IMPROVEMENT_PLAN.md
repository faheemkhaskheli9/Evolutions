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
- **Next:** cache `patch_contexts()` between years and invalidate it on a climate shift.
- **Next:** reuse scratch buffers in `Grid::build` and `tiles_around` in Individual mode.
- **Next:** store `cap_of` per deme in predation.
- **Next:** limit species-check comparisons to nearby demes.

## Phase 4: Missing MVP features (next)

- **FR-W1:** expose the random-world generator settings (size, land ratio, roughness, temperature, humidity) in `NewWorldDialog`.
- **FR-T3:** "run until" a speciation, an extinction, a population threshold or a trait threshold.
- **FR-P1:** autosave to IndexedDB, with restore at startup.
- **FR-O1:** per-species density and allele-frequency map layers.
- **FR-P2:** record interventions and mode switches with their timestamps, so a run from the same seed replays identically in the browser.
