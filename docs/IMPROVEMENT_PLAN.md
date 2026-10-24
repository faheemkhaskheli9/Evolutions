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

## Phase 3: Performance (done)

- **Done:** the seasonal cosine is computed once per day or month instead of once per tile (`World::season_cos` and `World::temp_season`).
- **Done:** Population-mode display vegetation regrows in 12 monthly steps instead of 36 ten-day steps.
- **Done:** the predator release no longer rescans the whole prey map for every tile (O(tiles²) to O(tiles)).
- **Result:** a 300-year `earth` run is about 20% faster.
- **Done:** profiling (callgrind, 100-year `earth` run) showed Individual mode taking ~85% of the time, led by `tiles_around` and `Grid::build`. `tiles_around` now returns a fixed-size inline list and wraps each column once instead of once per tile, and `Grid::build` reuses its scratch buffers. A 30-year `earth` run executes 13% fewer instructions and saves a byte-identical world.
- **Done:** patch contexts are cached between years and rebuilt when the climate changes (`World::revision`, bumped by `refresh_biomes`) or when the patch size or habitat area per tile changes (`patch_contexts_follow_climate_and_settings`).
- **Done:** predation works out each hunted deme's cap once per patch instead of once per predator.
- **Done:** species checks compare demes in the same patch first and skip any pair already joined into one cluster. The clusters are the same as before (they are the connected components, whatever order the pairs are compared in), so a hard distance limit, which would change which demes count as one species, was not needed.
- **Result:** a 300-year `earth` run is about 7% faster (5.8 s to 5.4 s; Population mode only, 3.5 s to 3.35 s) and saves a byte-identical world. A callgrind profile of a 100-year Population-mode run shows what is left: binomial draws (~21%), `annual` (~19%), `derive` (~17%, mostly `powf`), yearly vegetation regrowth (~11%) and `merge_sorted` (~10%).

## Phase 4: Missing MVP features (done)

| Requirement | Status |
|---|---|
| **FR-W1** world-generator settings | **Done.** "Customise world settings" in the new-world dialog sets size, land shape, land fraction, roughness, temperature, rainfall, axial tilt and latitude range, starting from the chosen template. The catalog now includes each template's settings. The engine rejects invalid settings (inverted or out-of-range latitudes, land fraction outside 0–1, non-finite values); see `custom_world_settings_apply_and_are_validated`. |
| **FR-T3** run until | **Done.** **Run until…** in the time bar fast-forwards until a speciation, an extinction, a population (all life or one species) or a species' mean trait crosses a threshold, within a year limit. A condition that already holds runs the full length and says so, instead of stopping at once. |
| **FR-P1** autosave | **Done.** The worker saves the world to IndexedDB after creating or loading it, after every fast-forward, and once a minute while it changes. At startup the new-world dialog offers **Resume**. Autosave is best-effort: storage errors are ignored. |
| **FR-O1** per-species map layers | **Done.** The Population density layer has a species selector. This also fixes the layer showing an empty map in Individual mode, because the worker sent density only in Population mode or during a focus. The **Allele frequency** layer shows one allele's frequency per patch (animals and demes pooled; -1 where the species is absent). Its picker lists the species' polymorphic alleles (1–99%), trait loci first, by expected heterozygosity, with their effects, origin and overall frequency. Alleles are named by their permanent `uid`, so a choice survives allele-table compaction and mode switches (`allele_frequency_layer_tracks_alleles_across_modes`). |
| **FR-P2** replay log | **Done.** `Simulation::set_mode`, `focus_region` and `apply` log each action with its simulated minute (a refused action that changed nothing is not logged; automatic mode switches inside a step are not logged because the replay repeats them). The log and the creation settings are stored after the state in the save (`MODEL_VERSION` 2), as JSON, so they are not part of `state_hash()`. `Replay` recreates the world and applies the log at the recorded times; the JSON replay script carries the end time and the original state hash, so a replay reports whether it is identical (`replay_log_reproduces_the_run`). In the app: **Export replay log**, **Replay from the start**, and **Open** accepts a replay `.json`. |

The e2e smoke test (`app/web/e2e/smoke.mjs`) covers custom settings, a single-species density layer, the allele-frequency layer, **Run until**, and resuming the autosave after a reload, and exporting the replay log and replaying it to an identical world.

## Phase 5: Second audit (done)

A second audit, of the web app and the engine, found the bugs below. All are fixed. Each engine fix has a regression test in `engine/evo-engine/tests/validation.rs`, and the e2e smoke test now checks the region-then-jump case.

### Engine

| # | Problem | Fix |
|---|---|---|
| 1 | `width * height` could overflow in the world-size check, so a huge custom size passed it and terrain generation panicked (a WASM trap in the browser). `plates` and `mountain_height_m` were not checked at all. | `checked_mul` for the size; `plates` must be 1–64 and the mountain height 0–20,000 m (`bad_world_settings_are_rejected`). |
| 2 | A founder release with no habitat (clicking open ocean) returned an error but still created a species. The next year logged it as extinct, and it showed up as a root in the tree and in the Newick export. | Habitat is found before the genome architecture and species are created (`failed_founder_release_creates_no_species`). |
| 3 | Loading checked only the magic bytes and version, so a damaged save could load and then panic on the first step (for example, `tick_minutes = 0` divides by zero). | `Simulation::validate` checks time and patch parameters, the terrain size, and every deme's and animal's species, genome and patch (`damaged_save_is_rejected`). |
| 4 | A watch region dragged across the east–west seam selected the inverted rectangle, nearly the whole world. Coordinates past the map gave an empty or inverted focus. | The UI sends unwrapped coordinates; `focus_region` clamps to the map and lets the region run up to one world width past the seam (`focus_region_wraps_across_the_seam`, `focus_region_clamps_to_the_map`). |
| 5 | The east/west part of a daughter species' name averaged patch x on a line, so a range spanning the seam got the wrong side. | Circular mean of x. |
| 6 | A meteor at `fx` outside 0–1 measured distances wrongly. | `fx` wraps and `fy` is clamped. |
| 7 | CLI: `--individual-years` was measured from year 0 instead of the start of the run, so it did nothing for a loaded save. Bad values (`--years abc`, `--mode Individaul`) and unknown options were silently ignored, and an unreadable `--load` file panicked. | Measured from the start year; bad values and unknown options exit with a message (code 2); `--mode individual` is applied; `--debug` is documented. |

### Web app

| # | Problem | Fix |
|---|---|---|
| 1 | Watching a region locked **Simulation detail** to Individual. A jump then ended the region in Population mode, and nothing switched back: at slow speeds time barely moved, and the selector still read Individual. | The mode choice from before the region is restored once the region ends; after a jump, Individual mode is re-entered when the world fits. |
| 2 | After **New world** or **Open**, queued Inspector requests for the old animal could be answered by the new engine, showing an unrelated animal with the same id. | Replies carry the animal id and are ignored unless it is still the one inspected. When the animal dies, the Inspector says so and stops polling. |
| 3 | Panning more than two world widths, or zooming out, left blank strips. | The map draws as many east–west copies as fill the view. |
| 4 | The achieved-speed label flickered between 0 and a spike in Population mode, because a one-year step lands on only some frames. | It shows the requested speed less any dropped backlog, smoothed over about a second. |
| 5 | Choosing the same `.evo` file twice did nothing. | The file input is reset after each pick. |
| 6 | **Run until** could send a remembered species that had since gone extinct. | A species that is no longer alive falls back to **All life**. |
| 7 | Number boxes (founder count, release year, epidemic %) clamped on every keystroke: deleting "100" left "2". | `NumberInput` clamps on blur or Enter. |
| 8 | Holding Space toggled play repeatedly, and Space during a jump was lost. Esc did not cancel a tool while a box had focus. | Key repeat and jumps are ignored; Esc works everywhere. |
| 9 | Autosave was skipped when only an intervention changed the world while paused, and a new world could go unsaved for a minute if an older save was still being written. | A dirty flag replaces the year check, and a save requested during a write runs when it finishes. |
| 10 | A jump that ended because all life died reported that the condition was never met. | It reports that all life has died out. |
| 11 | Charts: a single sample drew empty axes; year ticks read "0.0" or "1,001"; log-scale ticks fell on values like 11,535. The **World** tab was clipped in the side panel. | One sample draws a dot; ticks use round values (powers of ten on log scales); tabs share the panel width. |

The user guide is updated to match (speed presets, mode overrides, regions across the seam, autosave, keyboard), and its screenshots are regenerated, including a new one of the founder table.

### Next

- **Done:** replay log (FR-P2), as listed in Phase 4.
- **Done:** allele-frequency map layer (FR-O1).
- **Done:** old `.evo` files load. Version 1 saves have no replay log; the earliest ones (from the first engine commit) also lack allele uids and `SimParams::dmi_rate_factor`. The loader tries the version 1 layout with uids, then the earliest layout, which gives alleles uids in table order and the current default `dmi_rate_factor`. Tests: a generated version 1 save in both layouts (`version_1_saves_load_and_continue`) and a real save from the first engine commit (`earliest_save_format_loads_and_continue`, `tests/fixtures/v1_pre_uid.evo`).
- **Done:** `EngineClient` starts its worker on first use and has `terminate()`, which the app calls when it unmounts, so React StrictMode in development no longer leaves a second worker running.
