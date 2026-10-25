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

## Phase 6: Web-app unit tests and lint (done, #30)

- **Unit tests:** Vitest (`npm test` in `app/web/`, 58 tests, under a second). Covered: `util.ts` (formatting, colour ramps, achieved-speed smoothing), `mapLayers.ts` (terrain, climate, vegetation, allele, population and snow pixels; east–west copies), chart ranges and ticks in `LineChart`, `EngineClient` (lazy start, listeners, start errors, `terminate` and restart, with a fake `Worker`), the `inspect` reply matching in `protocol.ts`, and `autosave.ts` with `fake-indexeddb`.
- To make them testable without a browser, the pure parts moved out of components and the worker without changing behaviour: the map layers compute RGBA pixels (`terrainPixels`, `scalarPixels`, `demePixels`, `snowPixels`) and `worldCopies`; the chart has `chartRange`, `yAxisTicks` and `xTickLabel`; `NumberInput` has `parseClamped`; **Run until** has `pickSpecies`; the worker's autosave scheduling is the `Autosaver` class; achieved speed is `smoothSpeed`; and the Inspector uses `inspectReply`.
- **Regression tests** for the Phase 5 web bugs that are pure logic: 2 (stale Inspector replies), 3 (map copies), 4 (achieved speed), 6 (extinct species in **Run until**), 7 (number boxes), 9 (autosave during a write) and 11 (chart ticks), plus the StrictMode worker leak. The others need the engine or the DOM (Phase 2's stale `speciesDetail` replies are filtered in the worker, by the engine returning `null` for an unknown species; keyboard handling, the file input, the clipped tab) and stay covered by the e2e smoke test.
- **Lint:** ESLint 9 (`npm run lint`) with the recommended JavaScript and TypeScript rules and the React Hooks rules, failing on any warning.
- **CI:** the web job runs `npm run lint` and `npm test` before the WASM build.


## Phase 7: Data export from the app (done, #21)

- **Engine:** `evo_engine::export` holds the writers: `species_csv` / `species_csv_files` (per-species `year,n,het`, the trait means, then each trait's SD as `<trait>_sd`), `chronicle_csv` (`year,kind,species,species_name,text`, quoted per RFC 4180) and `settings_json` (template, seed, realism, generator settings, founders). `newick` was already there.
- **CLI:** `--csv` now uses the shared writer, so its files gain the SD columns after the existing mean columns. New `--chronicle <file>` and `--settings <file>`.
- **App:** **Export data** in the World tab downloads the time series (all species or one), the Chronicle, the Newick tree and the settings, through new `evo-wasm` methods (`species_csv_files`, `chronicle_csv`, `settings_json`).
- **Tests:** `evo-cli/tests/export.rs` runs `evo run --load … --years 0` with every export option and checks each file is byte-identical to what the engine returns for the same save (what the app downloads), and checks the headers. Vitest covers `exportFiles` in `protocol.ts`; the e2e smoke test downloads each file and checks its header.

## Phase 8: Faster Population-mode core (done, #14)

Benchmark: `evo run --template earth --seed 1 --years 300 --mode population`, release build, median of 5 runs, the old and new binaries run alternately on the same machine.

| | Before | After |
|---|---|---|
| Wall time | 3.22 s | 2.49 s (−23%; −26% against the 3.35 s measured in Phase 3) |
| Instructions (callgrind) | 29.6 G | 14.3 G (−52%) |

Wall time falls less than the instruction count because what is left is mostly memory access (allele tables, deme pools) and `libm` calls.

**Changes that keep fixed-seed results bit-for-bit.** With the old random draws put back, the CSV, Chronicle, Newick and `.evo` outputs of a 300-year Population run and a 30-year Auto run are byte-identical to the previous engine.
- **Selection gradient:** fitness doesn't read 5 traits (boldness, sociality, preference, choosiness, dispersal), so their gradient is exactly 0 and is not computed (`FITNESS_NEUTRAL`). For the others, only the perturbed trait's transform is recomputed. When that trait isn't body mass, the mass powers (`MassTerms`) are reused. When it is one of the 6 traits the heat balance doesn't read (`HEAT_NEUTRAL`), the deme's 24-step thermal year (`year_heat`) is reused too. Unit tests check that each shortcut gives bit-identical fitness.
- **`derive`:** `kleiber_w` and `adult^0.25` are computed once instead of 3 and 2 times. Newborns' cold/heat stress uses `derive_thermal`, which computes only the four fields `thermal` reads. `annual` takes the adult's `Derived` instead of recomputing it. The prey cap reuses the fed-to-capacity fecundity already worked out for each deme.
- **Vegetation:** `grow_vegetation_year` skips tiles whose yearly cycle has reached a bit-exact fixed point. The flags are cleared when the climate changes (`revision`), in Individual mode, and when an event zeroes stocks. They are not saved, so they are rebuilt after loading. Vegetation fell from ~13% of instructions to under 1%.
- **`merge_sorted`:** merges into a reused buffer. When both pools carry the same alleles (the usual case), it averages them in place.
- **Allele frequencies:** deme mean latents are computed once per year, not twice.

**Changes to the random stream** (`MODEL_VERSION` 3, save layout unchanged):
- `Rng::binomial` for n ≤ 64 uses inversion: one uniform draw, walking up the pmf, instead of n draws. `Rng::poisson` for λ < 30 also uses inversion instead of Knuth's product of uniforms. Both are exact samplers of the same distributions.
- The normal approximations in `binomial` and `poisson` use Marsaglia's polar method instead of Box–Muller (no `cos`). `Rng::normal` itself is unchanged.
- In Wright–Fisher resampling, the last allele takes the remaining copies instead of drawing `Binomial(left, ≈1)`.

Effect on results: same distributions, different draws. So fixed-seed runs differ in detail, in both modes (Poisson draws are also used for mutation in meiosis). On `earth`, seeds 1–8, 300 years, the outcomes are alike. In each version 3 of 24 species go extinct: the deer in seeds 1 and 4 and the wolf in seed 7 before, the deer in seed 4 and the wolf in seeds 6 and 7 after. Final hare and wolf numbers and body masses fall in the same ranges. Seed 1's deer, which died out in year 162, now hangs on with 9 animals. The validation suite passes, including the `--ignored` long tests. New unit tests check the small-n binomial against its pmf, the moments of the polar normal, and `merge_sorted` against a plain merge.

**Not done:** `Rng::binomial` (~22% of instructions, mostly the normal approximation in Wright–Fisher resampling) and the deme mean latents (~8%, also recomputed by `record_metrics`) are now the largest costs. Multi-threading is #15.

## Phase 9: Checkpoints and rewind (done, #17)

- **Worker:** `app/web/src/engine/checkpoints.ts` keeps checkpoints: `.evo` saves gzipped with the browser's `CompressionStream`, in memory, up to 200 MB. One is taken when a world is created, opened or replayed; every 10 simulated years (at most one a second, at least every 30 s while time moves); before each jump (before its switch to Population mode); and before each intervention. Over budget, the checkpoint whose neighbours are closest together relative to its age is dropped (those before an intervention count double), so spacing grows roughly geometrically into the past. A checkpoint still compressing when the history is replaced is dropped.
- **Rewind:** loads the checkpoint as a new engine and drops the later checkpoints. The Chronicle's "go to just before this" finds the latest checkpoint whose event count is at most the event's index (the worker now numbers events). The rewound world becomes the autosave.
- **App:** a **Rewind** strip under the time bar shows the checkpoints by year, coloured by reason, with their count and memory.
- **Engine:** no change to the simulation or save format. `evo-wasm` gains `state_hash()`.
- **Tests:** `rewinding_to_a_checkpoint_reproduces_the_run` takes checkpoints across a mode switch and before an intervention, and checks each reaches the original's `state_hash()`. Vitest checks the store stays under 200 MB through a simulated 10,000-year run, keeps recent history denser, finds the checkpoint before an event, and round-trips compression. The e2e smoke test rewinds to the checkpoint before a 100-year jump, jumps again and compares state hashes, and uses **⟲** to undo a climate shift.
- **Not done:** checkpoints are not stored with the autosave in IndexedDB, so they last for the session only. Branching from a checkpoint is #18.

## Phase 10: Population-genetics charts (done, #20)

- **Engine:** new `popgen` module. At each metrics sample, per species, from neutral loci: H_T (pooled) and H_S (per patch, weighted by size), both with Nei's small-sample correction; F_ST = (H_T − H_S)/H_T; Nₑ from the adult sex ratio (adults counted as the Population-mode step counts them); Nₑ from the loss of heterozygosity over the last 20 generations (generation time = age at maturity, at least 1 year), found by inverting F' = (1 − μ)²·(1/2Nₑ + (1 − 1/2Nₑ)F) by bisection; the number of segregating alleles; and the frequencies of up to 48 alleles between 1% and 99% (plus those tracked in the previous sample, so a trajectory shows its end). μ is the effective neutral rate: nominal for animals, and for demes capped at `MAX_NEW_NEUTRAL` new alleles per deme per year as the Population step does. Without that correction, a 180,000-hare Blank Slate run reported Nₑ ≈ 1,000.
- **Determinism:** the measures read the state and draw no random numbers, so fixed-seed outcomes and `state_hash()` are unchanged.
- **Save format:** `MODEL_VERSION` 4 writes the series after the replay log. Version 3 and older saves load with an empty series, which then starts from the load point.
- **Export:** `species_csv_files` adds `species_<id>_<name>_popgen.csv` per species (`year,ht,hs,fst,ne,ne_h,generation_years,segregating_alleles`).
- **App:** a Population genetics section in the Species panel (heterozygosity, F_ST, census and effective size, segregating alleles, and up to five allele trajectories picked from the species' allele list). Multi-series charts show a legend.
- **Tests:** `v_g3_ne_from_heterozygosity_decay` (V-G3 drift with N = 50: the estimator gives Nₑ within 20% of 50); `v_s1_fst_rises_across_a_strait` (V-S1 strait setup: F_ST ≈ 0.33 after 400 years against ≈ 0.04 in the same world without the strait); `popgen_series_saved_and_old_saves_start_fresh`; unit tests for the estimator and the unbiased heterozygosity; Vitest for the chart series.
- **Note on the acceptance criteria:** Blank Slate's 300 founder hares grow to ~180,000 within 20 years, and heterozygosity then sits near mutation–drift balance, so its Nₑ cannot be checked against the founder count in a live run. The 1/(2Nₑ) decay and the estimator are checked on the V-G3 drift instead, as the issue suggests. In Blank Slate the heterozygosity-based Nₑ is mostly blank or very large, as expected for so many animals.

## Phase 11: "Why?" explanations (done, #19)

- **Engine:** new `explain` module. Per species it keeps (a) yearly *vitals* for the last 60 years: the hazard from starvation, dehydration, predation, heat, cold and ageing, food and fecundity, and the selection gradient on every trait split into seven drivers (those hazards and reproduction); (b) a decimated (≤ 400 samples) series of numbers and hazards for crashes and extinctions; (c) a decimated series of each trait's genetic mean and additive variance V_A (latent scale, from pooled allele frequencies) with Nₑ. Explanations are written when the event happens and kept by event index.
- **Selection gradient split (Population mode):** `fitness` became `fitness_parts`, which also returns survival, each hazard, fecundity and newborns' heat and cold stress. With w = s + R/4 and s = e^(−H), d ln w = (−s·dH + dR/4)/w; the young-survival terms of dR (they survive at e^(−(H/2 + birth stress))) are moved to their hazard, so insulation's benefit to newborns counts as cold stress rather than reproduction. **Individual mode:** each cause's share is the viability selection differential it caused (dead animals' latent mean minus the living mean, times the death rate) over V_P.
- **Drift test:** Lande (1976): drift adds V_A/Nₑ to the variance of the mean per generation, so the change over an interval is compared with √(Σ V_A·Δt/Nₑ). z < 2 is "consistent with drift", z ≥ 3 "selection", in between "mixed". In Population mode Nₑ is the breeding sex-ratio Nₑ the step resamples with, each year a generation; in Individual mode it is the lower of that and the heterozygosity-based Nₑ, per generation time. Using min(Nₑ, Nₑ from H) in Population mode too made the test too lenient (mean z² ≈ 0.75); the model's own Nₑ gives 1.14.
- **Crashes and extinctions:** the peak since the species' last crash (from the metrics series), hazards around it against the last three years, and the hazard that rose most (or, if none rose, the one that stayed highest). Causes: a plague or meteor in the window (intervention), a new or growing meat-eating species (predation), a climate offset change of ≥ 0.5 °C, or a food shortage (with "outgrew its food supply" when food fell from the peak).
- **Speciation:** `detect_splits` now also returns the populations of the new species and of the largest remaining cluster. Their merged summaries give the three factors of `compatibility` (`compatibility_parts`), and isolation is split by −ln of each. For allopatric splits, the straight line between the nearest patch centres is checked for water and mountains (> 2,000 m).
- **Determinism and saves:** nothing here draws random numbers or changes the state, so fixed-seed outcomes and `state_hash()` are unchanged (checked by comparing saves before and after). `MODEL_VERSION` 5 writes the records after the population-genetics series; older saves load without them.
- **API:** `events_json` marks explained events with `why: true`; `why_json(index)`; `explain_trait_json(species, key, from, to)`; WASM `why()` and `explain_trait()`; CLI `--why <file>`.
- **App:** **Why?** on Chronicle events opens a panel (verdict badge, plain-language answer, points, a collapsible table of numbers, and a link to the matching chart). Under the Species tab's trait chart, *Why did … change?* explains the charted trait since the species arose.
- **Tests:** `why_insulation_rises_from_cold_in_the_ice_age` (Ice Age seed 3, 200 years: selection, cold stress, z ≈ 6), and the ignored `why_insulation_event_names_cold_stress` for the Chronicle event itself (year 951, z ≈ 10); `why_neutral_trait_change_is_drift_in_a_small_population` (Blank Slate, 60 founders, ~650 hares: all five fitness-neutral traits are drift); `drift_test_is_calibrated_on_neutral_traits` (120 century-long tests over 8 seeds: mean z² 1.14, one with z ≥ 3); `why_crash_from_plague_and_saves` (a plague crash is explained as an intervention; records survive saves; version 4 saves load without them); V-S1 now checks the speciation "Why?" (allopatric, open water, DMI the main factor); unit tests for the verdict thresholds and the drift variance; Vitest for the chart links; e2e steps for both panels.

