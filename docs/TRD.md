# Evolutions — Technical Requirements Document (TRD)

| Field | Value |
|---|---|
| Document | Technical Requirements Document |
| Product | Evolutions |
| Version | 0.1 (Draft) |
| Date | 2026-10-15 |
| Status | Proposed |
| Related | [PRD](./PRD.md) · [System Design](./SYSTEM_DESIGN.md) |

---

## 1. Purpose and scope

This document turns the product requirements in the [PRD](./PRD.md) into technical requirements:

- the technology stack and platform constraints,
- the **scientific models** the engine must implement (with equations and default parameters),
- performance, determinism, persistence and interface requirements,
- the **validation suite** that proves the simulation behaves like real biology.

How the components are arranged and how data flows between them is covered in the [System Design](./SYSTEM_DESIGN.md).

Requirement IDs: `TR-<area><n>`. "MUST" = mandatory, "SHOULD" = strongly recommended, "MAY" = optional.

## 2. Technology stack

| Layer | Choice | Rationale |
|---|---|---|
| Simulation engine | **Rust** (stable, edition 2021+) | Memory safety, predictable performance, data-parallelism, compiles to native and WASM |
| ECS | `bevy_ecs` used standalone (no Bevy renderer) | Archetype/SoA storage, parallel system scheduling, mature |
| Parallelism | `rayon` work-stealing pool | Deterministic data-parallel iteration with ordered merges |
| RNG | Counter-based **Philox 4x32-10** (or equivalent keyed hash RNG) | Random numbers addressable by `(seed, tick, entity, stream)`, so results are independent of thread scheduling |
| Math | `glam` for vectors; `libm` for transcendental functions | `libm` gives bit-identical results across platforms |
| Desktop shell | **Tauri 2** | Small footprint; engine runs as a native sidecar process |
| UI | **TypeScript + React** | Productive UI development; shared with web build |
| Rendering | **three.js** (WebGPU renderer, WebGL2 fallback) with instanced meshes; 2D map in WebGL2 | Same renderer for desktop webview and browser |
| Charts | uPlot (time series), D3 (phylogeny, histograms) | Fast with large series |
| Engine ↔ UI protocol | **FlatBuffers** binary messages over a local socket (desktop) or `postMessage` with `SharedArrayBuffer` (web) | Zero-copy reads of large snapshots |
| Persistence | **SQLite** save container (single file) + zstd-compressed binary blobs | Transactional, portable, queryable |
| Web build | Engine compiled to `wasm32-unknown-unknown` with threads (`wasm-bindgen-rayon`), run in a Web Worker | Same engine code in the browser |
| Config / templates | TOML or JSON, validated with JSON Schema | Human-editable, moddable |
| Build / CI | Cargo workspace, pnpm workspace, GitHub Actions | Standard tooling |

**TR-S1** The engine MUST NOT depend on the UI; it MUST be runnable headless (CLI) for tests, batch experiments and fast-forward.
**TR-S2** All model parameters MUST live in versioned data files (no "magic numbers" in system code), with sources cited in comments or a parameter registry.

## 3. Platform and hardware requirements

| | Minimum | Reference (performance targets) |
|---|---|---|
| CPU | 4 cores, x86-64 (AVX2) or ARM64 | 8 cores / 16 threads, 2020+ desktop class |
| RAM | 8 GB | 16 GB |
| GPU | WebGL2-capable integrated | WebGPU-capable discrete or recent integrated |
| Storage | 2 GB free | SSD, 10 GB free |
| OS | Windows 10+, macOS 12+, Ubuntu 22.04+ | — |
| Browser (web build) | Latest Chrome, Edge, Firefox, Safari with cross-origin isolation (for `SharedArrayBuffer`) | — |

**TR-P1** The web build MAY limit world size and Individual-mode population (default cap 20,000 individuals).

## 4. Time model and simulation fidelity

### 4.1 Simulated time

**TR-T1** Simulated time MUST be kept as a 64-bit integer count of **sim-minutes** since world start (enough for more than 10¹³ years).
**TR-T2** One calendar is used: `day_length` and `year_length` come from world parameters (defaults 24 h, 365.25 days).

### 4.2 Fidelity modes

The engine MUST support three fidelity modes with different time steps:

| Mode | Unit simulated | Time step (Δt) | Target throughput (reference HW, reference world) | Used for |
|---|---|---|---|---|
| **Individual (I)** | Every organism as an entity | 15 sim-minutes (behaviour, physiology); movement interpolated for rendering | ≥ 250 ticks/s with 100,000 individuals (≈ 2.6 sim-days/s) | Watching, short time spans, local detail |
| **Population (P)** | Demes: age/sex-structured groups per species per habitat patch, with allele frequencies | 1 sim-year (with seasonally integrated vital rates) | ≥ 100 sim-years/s with 10,000 deme-species; up to ≈ 1,000 y/s on small worlds | Centuries to tens of thousands of years |
| **Macro (M)** | Population lineages / species: abundance, mean traits, G-matrix, range | 1,000 sim-years | ≥ 10,000 sim-years/s with 2,000 lineages; guaranteed floor 2,000 y/s on the largest world (1 My < 10 min) | 10⁵–10⁸ years |

**TR-T3** The time controller MUST choose the mode automatically from the requested speed and engine load, with hysteresis to prevent flapping, and MUST allow the user to lock a mode.
**TR-T4** **Regional fidelity** (P1): the world MAY run in mixed mode, with a focus region in Mode I and the rest in Mode P. Gene flow and migration across the boundary MUST be exchanged every P-step (individuals emigrate into demes as counts and allele contributions; immigrants are materialised by sampling, see §5.11).
**TR-T5** Mode transitions MUST conserve state within tolerances (§5.11.4) and MUST be deterministic.

## 5. Scientific model specifications

All defaults below are **starting points** that MUST be tunable through parameter files and MUST be covered by the validation suite (§9).

### 5.1 World and terrain

**TR-W1 Grid.** The world is a 2D grid of square tiles on a cylinder (wraps east–west; poles at top and bottom). Default 1024 × 512 tiles; tile edge configurable (default 2 km). The web build defaults to 512 × 256.
**TR-W2 Terrain generation.** Elevation MUST be generated by:
1. Plate seeding (Voronoi plates with random motion vectors); convergent boundaries raise mountain ranges, divergent boundaries create rifts.
2. Multi-octave noise (fBm / ridged noise) for detail.
3. Hydraulic and thermal erosion passes (droplet-based, configurable iteration count).
4. Sea level chosen to hit the target land fraction.
**TR-W3 Hydrology.** Flow direction by D8 steepest descent, with depression filling (priority-flood) creating lakes; flow accumulation from the precipitation field determines rivers; tiles above a flow threshold are rivers and count as water sources. Seasonal ponds appear when monthly precipitation exceeds evaporation.

### 5.2 Climate

**TR-C1 Insolation.** Daily mean insolation per tile from latitude, axial tilt ε, day of year (solar declination) and the solar constant (standard astronomical formula).
**TR-C2 Temperature.** Monthly mean surface temperature:

$$T = T_{lat}(\phi, \text{month}) - \Gamma \cdot z + \Delta T_{ocean}(d_{coast}) + \Delta T_{GHG} + \Delta T_{event}$$

with lapse rate Γ = 6.5 °C/km, ocean moderation reducing seasonal amplitude near coasts, a global greenhouse offset, and event offsets (volcanic winter, impact winter). Daily variation is added as a sinusoid plus noise.
**TR-C3 Precipitation.** Three-cell circulation (Hadley/Ferrel/polar) sets prevailing wind bands and baseline humidity per latitude. Moisture is advected along the wind from ocean tiles; orographic lift on windward slopes triggers rain and dries the leeward side (rain shadow). Monthly precipitation shifts with the seasonal movement of the ITCZ.
**TR-C4 Biomes.** Biome per tile from annual temperature and precipitation using a Whittaker-style lookup (tundra, boreal forest, temperate forest, grassland, desert, savanna, tropical forest, ice …).
**TR-C5 Long-term climate (P1).** In Modes P and M, global temperature MUST follow orbital forcing built from Milankovitch-like periodicities (~23 ky precession, ~41 ky obliquity, ~100 ky eccentricity) plus a user offset. Ice-sheet extent and sea level follow from global temperature (default ≈ −120 m at full glacial), exposing land bridges.
**TR-C6 Continental drift (P2).** In Mode M, plates move at 1–10 cm/yr; terrain is re-derived every 1 My.

### 5.3 Vegetation (primary production)

**TR-V1** Plant biomass is a per-tile field (several functional types: grass, shrub, tree, fruit/seed), not individual plants.
**TR-V2** Potential net primary productivity (NPP) MUST follow the **Miami model** (Lieth 1975), modulated monthly:

$$NPP = \min\left(\frac{3000}{1 + e^{1.315 - 0.119T}},\; 3000\,(1 - e^{-0.000664P})\right) \quad [\text{g dry matter}/\text{m}^2/\text{yr}]$$

**TR-V3** Biomass grows logistically toward a biome-specific maximum standing biomass at a rate set by NPP. Grazing removes biomass; overgrazing below a threshold lowers regrowth (degradation), recovering over years.
**TR-V4** Each plant type has energy density (kJ/g), digestibility, toughness and toxicity values. Herbivore gut traits determine how much energy is extracted.
**TR-V5 (P2)** Plant defence traits (toughness, toxicity) evolve per tile in response to grazing pressure, at the same cost/benefit balance as for animals.

### 5.4 Genome model

**TR-G1 Structure.** Each species lineage has a genome architecture: `C` chromosome pairs (default 12), `L` loci (default 384 functional + 128 neutral marker loci + 64 incompatibility loci), each locus at a fixed genetic map position (centiMorgans) on a chromosome. The architecture is inherited by descendant species and changes only through structural mutation (P1).
**TR-G2 Alleles.** An allele is an entry in a global, append-only **allele table**: `allele_id → { locus, parent_allele_id, origin_tick, effect vector (sparse, over traits), dominance h ∈ [0,1], is_null }`. Genomes store `u32 allele_id`s (2 per locus). This continuum-of-alleles design keeps full allele genealogy for analytics.
**TR-G3 Genotype to genetic value.** For quantitative trait *k*:

$$G_k = \mu_k + \sum_{\ell}\Big[a_{\ell k}(x_1) + a_{\ell k}(x_2)\Big] + \sum_{\ell} d_{\ell k}(x_1,x_2) + \sum_{(\ell,m)\in E_k} \epsilon_{\ell m k}$$

- additive effects *a* from the alleles' effect vectors,
- dominance deviation *d* from the alleles' dominance coefficients (h = 0.5 additive, 0 recessive, 1 dominant),
- a sparse set of **epistatic** locus pairs *E* (default 5% of trait loci),
- **pleiotropy**: one allele's effect vector may touch several traits. New mutations draw correlated effects from a species-specific mutational covariance matrix **M**.

**TR-G4 Phenotype.** $P_k = f_k(G_k, \text{env}, \text{age}, \text{condition}) + E_k$, with environmental noise $E_k \sim \mathcal{N}(0, \sigma^2_{E,k})$ set so that founder heritability matches literature ranges (morphology h² ≈ 0.4–0.6, life history h² ≈ 0.1–0.3, behaviour h² ≈ 0.2–0.4). Developmental functions *f* apply plasticity (for example nutrition during growth scales adult size) and constraints (§5.5).
**TR-G5 Meiosis.** Gamete formation MUST:
1. place crossovers per chromosome with count ~ Poisson(map length in Morgans), default ≈ 1 per chromosome arm,
2. assemble the recombinant haplotype,
3. assort chromosomes independently.
**TR-G6 Mutation.** Per locus per gamete, mutation probability μ (Strict default 1×10⁻⁵ per functional locus per generation; neutral marker loci 1×10⁻⁴). A mutation creates a new allele whose effect = parent effect + δ, δ ~ MVN(0, **M**) scaled per locus; a fraction (default 5%) are large-effect or loss-of-function. Realism dial "Accelerated" multiplies μ by up to 1,000 and MUST be shown in the UI and save metadata.
**TR-G7 Heritable mutation rate (P1).** A modifier locus scales μ within [0.1×, 10×], so mutation rate itself can evolve.
**TR-G8 Structural mutations (P1).** Duplication (new locus copy that can diverge), deletion, inversion (suppresses recombination in the inverted segment), chromosome fusion/fission (changes C and contributes to incompatibility).
**TR-G9 Sex determination.** XY (default), ZW, or temperature-dependent (template choice). Sex-linked loci are supported.
**TR-G10 Incompatibility loci (Dobzhansky–Muller).** For each pair of *derived* alleles at different incompatibility loci, incompatibility is a deterministic hash: `incompatible(a, b) = H(world_seed, a, b) < p_DMI` (default p_DMI = 0.02). Hybrid viability/fertility is multiplied by $(1 - s_{DMI})^{n}$ for *n* incompatible pairs present (default s_DMI = 0.3). Because the number of possible pairs grows quadratically with divergence, the expected "snowball effect" (Orr 1995) emerges.
**TR-G11 Memory.** Genome storage MUST use structural sharing (copy-on-write chromosome blocks, reference-counted), so a population of 100,000 uses ≤ 200 MB for genomes.

### 5.5 Traits, body plan and constraints

**TR-B1** The MVP body plan is the **terrestrial tetrapod vertebrate** (endotherm by default; ectotherm templates allowed). Bird-like flight and aquatic plans are P2.
**TR-B2** The trait set MUST include at least:

| Group | Traits |
|---|---|
| Size and shape | adult body mass, limb length index, appendage size (ears/tail, Allen's rule), body condition set-point |
| Locomotion | muscle fibre ratio (sprint vs. endurance), locomotion mode (cursorial, ambulatory, climbing) |
| Thermal | insulation (fur/feathers/fat), sweating/panting capacity, thermal-neutral zone centre |
| Diet | plant digestion efficiency, meat digestion efficiency, dentition axis (grinding ↔ shearing), gut length |
| Water | renal concentrating ability |
| Senses | visual acuity, night vision, olfaction, hearing |
| Appearance | coat luminance, coat hue, pattern (stripes/spots), ornament size |
| Cognition | relative brain size, learning rate, social learning propensity |
| Behaviour | boldness, aggression, sociality/group size preference, territoriality, activity period, dispersal propensity, migration tendency, parental care |
| Life history | age at maturity modifier, litter/clutch size, gestation modifier, investment in maintenance (lifespan), breeding seasonality |
| Sexual | ornament size, mate choosiness, preferred ornament value |
| Immunity | immune investment, resistance loci (MHC-like, matching pathogen antigens) |
| Hominin (P1) | bipedal efficiency, manual dexterity, hair density, skin pigmentation, eccrine sweat density, childhood duration |

**TR-B3 Allometry.** Physiological and life-history quantities MUST scale with body mass *M* (kg) using empirical allometric exponents, with genetic modifiers allowed only within bounded ranges (default ±30% of the allometric prediction):

| Quantity | Scaling | Reference |
|---|---|---|
| Basal metabolic rate | $B = 70\,M^{0.75}$ kcal/day (endotherm); ≈ 1/10 for ectotherm at 20 °C with Q₁₀ = 2.5 | Kleiber 1932 |
| Field metabolic rate | ≈ 2–4 × BMR depending on activity | Nagy 1987 |
| Cost of locomotion | $P/M = 10.7\,M^{-0.316}\,v + 6.03\,M^{-0.303}$ W/kg | Taylor et al. 1982 |
| Maximum speed | $v_{max} = a M^{b}\,(1 - e^{-h M^{i}})$ (hump-shaped) | Hirt et al. 2017 |
| Age at maturity, gestation, lifespan | ∝ $M^{0.25}$ | Calder 1984; Western 1979 |
| Water turnover | ∝ $M^{0.8}$ | Nagy & Peterson 1988 |
| Heat-exchange surface | ∝ $M^{0.67}$ × appendage modifier | — |
| Home range (herbivores / carnivores) | ∝ $M^{1.0}$ / $M^{1.4}$ (emergent check, not imposed) | Kelt & Van Vuren 2001 |
| Brain tissue cost | ≈ 20× per-gram cost of resting muscle | Aiello & Wheeler 1995 |

**TR-B4 Energetic cost of traits.** Every trait with a biological cost MUST add to maintenance or construction energy (for example insulation adds to growth cost and causes overheating risk in heat; ornaments add cost and predation conspicuousness; immune investment adds maintenance cost; brain mass adds maintenance cost). Without this, the PRD's "no free lunch" rule is violated.

### 5.6 Physiology

**TR-PH1 Energy budget.** Each tick: `ΔE = intake·efficiency − (BMR + activity + thermoregulation + growth + reproduction + immune) · Δt`. Surplus is stored as fat (39 kJ/g at conversion efficiency ≈ 0.8); deficit burns fat, then lean mass. Death when lean mass < 60% of structural mass.
**TR-PH2 Growth.** Ontogenetic growth follows West et al. 2001: $\frac{dm}{dt} = a\,m^{3/4}\left[1 - \left(\frac{m}{M}\right)^{1/4}\right]$, limited by available surplus energy (plasticity: undernourished juveniles mature smaller).
**TR-PH3 Thermoregulation.** Heat loss ∝ surface × (T_body − T_air − wind/sun corrections) / insulation. Below the lower critical temperature, extra metabolic heat is required; above the upper critical temperature, evaporative cooling costs water. Exceeding tolerance accumulates heat/cold stress causing damage and death.
**TR-PH4 Hydration.** Water lost through respiration, evaporation (temperature- and activity-dependent), urine (set by renal concentrating ability) and lactation; gained from drinking and from food water content. Death at ≈ 15–20% body-water loss (default 15%).
**TR-PH5 Mortality.** Intrinsic mortality per year follows a Gompertz–Makeham hazard $\mu(t) = A + B\,e^{\gamma t}$, with B and γ affected by maintenance investment and body mass. Extrinsic causes (predation, starvation, dehydration, exposure, disease, injury, accident) are simulated explicitly and recorded as cause of death.
**TR-PH6 Condition and injury.** Fights, failed hunts and falls cause injuries that lower speed and foraging until healed (healing has an energy cost).

### 5.7 Behaviour and cognition

**TR-BH1 Architecture.** Utility-based action selection (MVP): for each candidate action (forage, drink, hunt, flee, rest/sleep, seek mate, court, mate, care for young, follow group, migrate, explore, thermoregulate (shade/shelter), fight, groom/socialise), compute `utility = f(internal drives, perception, memory) · w_gene`. Drives are normalised physiological deficits (hunger, thirst, fatigue, fear, thermal stress, libido, parental urge). **Weights come from heritable behavioural traits**, so behaviour evolves.
**TR-BH2 Perception.** Detection probability per tick depends on distance, sense acuity, target camouflage (coat colour vs. background biome colour), target noise/scent, light level (night vision) and vegetation cover. No omniscient knowledge.
**TR-BH3 Memory and learning (P1).** Each individual keeps a small spatial memory (≤ 16 entries: water, food patches, danger) and simple reinforcement for food choices (aversion to toxic food). Social learners copy memory entries and behaviours from group members, weighted by social learning propensity.
**TR-BH4 Movement.** Continuous positions in world coordinates; steering toward targets with obstacle and terrain cost (slope, water, vegetation). Long-distance navigation uses precomputed **flow fields** per region for water and refuges, shared by all agents.
**TR-BH5 Predation encounters.** A detected encounter starts a pursuit, resolved analytically within the tick using speeds, acceleration, stamina curves, head-start distance and terrain (so outcome does not depend on tick size). Capture success then depends on prey size relative to predator, group defence, and weapons/armour traits.
**TR-BH6 Groups.** Group membership, leadership (oldest/most dominant), and dominance hierarchy from fights. Group vigilance lowers each member's predation risk ("many eyes"); larger groups increase competition.
**TR-BH7 (P2)** Optional evolved neural-network controllers (fixed-topology small networks with heritable weights) as an alternative behaviour model for research use.

### 5.8 Reproduction and mating

**TR-R1** Sexual maturity reached at the age and minimum body condition set by life-history traits.
**TR-R2** Breeding seasonality: females enter oestrus by season/photoperiod (heritable timing) and condition threshold; males' readiness depends on condition and hormones (abstracted).
**TR-R3** Mating systems emerge from behaviour traits and ecology (monogamy, polygyny, lek-like display); no mating system is hard-coded per species.
**TR-R4 Mate choice.** The choosy sex (default females; determined by relative parental investment) evaluates candidates by ornament size vs. its preferred value, condition and genetic similarity (inbreeding avoidance by kin recognition, P1). Choice is probabilistic (logistic on preference match × choosiness), and has search costs.
**TR-R5** Conception probability depends on fertility (age, condition, hybrid fertility from §5.4 TR-G10). The offspring genome is formed by two meioses (TR-G5) plus mutation (TR-G6).
**TR-R6** Gestation length and litter size come from traits and condition; gestation and lactation energy costs are taken from the mother's budget; offspring with low birth mass have higher mortality.
**TR-R7** Parental care (feeding, protection, carrying) scales with parental care traits and is a trade-off against future reproduction.
**TR-R8** Asexual/parthenogenetic reproduction MAY be enabled in templates (P2).

### 5.9 Ecology

**TR-E1** Interactions between species (predation, competition for plants/water/space, scavenging) MUST be emergent from individual behaviour in Mode I.
**TR-E2** Carcasses are entities with decaying energy, used by scavengers.
**TR-E3 Disease (P1).** Pathogens are entities with their own small genomes (antigen loci, virulence, transmissibility, host range); transmission on contact within distance; SEIR-like within-host progression; host resistance from MHC-like loci matching; pathogen mutation each transmission with probability μ_path. Trade-off between virulence and transmissibility emerges from host death limiting spread.
**TR-E4** Nutrient recycling: dead biomass returns a fraction to tile fertility, which raises local NPP (bounded).

### 5.10 Species detection and phylogeny

**TR-SP1** The engine MUST classify individuals (Mode I) and demes (Mode P) into species using the **biological species concept**, computed periodically (default every 50 sim-years in I/P mode):
1. Build a graph whose nodes are local populations (demes).
2. Edge weight = expected reproductive compatibility = mean hybrid viability × fertility (TR-G10) × mating probability (overlap of preference/ornament distributions and breeding timing).
3. Species = connected components with edges above threshold τ (default 0.5), with hysteresis (a split must persist for 3 consecutive checks).
**TR-SP2** Split points are recorded in a **phylogeny** (species tree) with time, parent, cause tags (geographic barrier, ecological divergence, sexual isolation, polyploidy/structural) and a descriptive name (generated binomial).
**TR-SP3** Extinction is recorded when abundance reaches zero; extinct species keep their trait history for the fossil and phylogeny views.
**TR-SP4** Neutral-marker genetic distance (Nei's D, F_ST) MUST be computed between species and demes for analytics and for Macro-mode divergence clocks.

### 5.11 Population mode (Mode P)

**TR-MP1 Deme.** A deme is one species in one **habitat patch** (default 8 × 8 tiles). State:
- age × sex abundance vector (Leslie/Lefkovitch classes),
- per-locus allele frequency table (functional, neutral and incompatibility loci; alleles with frequency < 1/(2N) dropped into an "other" bin),
- additive genetic mean vector $\bar{z}$ and **G**-matrix for quantitative traits,
- mean condition and a **reservoir** of up to K = 64 representative haplotypes (to restore linkage structure when switching back to Mode I).
**TR-MP2 Vital rates via surrogate evaluation.** Each P-step, vital rates (survival, fecundity per class) MUST be computed by evaluating the **same physiology, behaviour and ecology functions** used in Mode I on a sample of S virtual individuals (default S = 32) drawn from the deme's phenotype distribution and local environment, with encounter rates from density-dependent functional responses (Holling type II) whose parameters derive from traits (attack rate from predator speed and detection vs. prey speed and camouflage). These surrogate rates MUST be calibrated against Mode I (§9, test V-P3).
**TR-MP3 Demography.** $\mathbf{n}_{t+1} = \mathbf{A}(\bar z, \text{env}, \text{density})\,\mathbf{n}_t$ with demographic stochasticity (binomial survival, Poisson births).
**TR-MP4 Selection on quantitative traits.** Lande–Arnold: $\Delta\bar{z} = \mathbf{G}\,\boldsymbol{\beta}$, with selection gradient β estimated from regression of relative fitness on phenotype in the surrogate sample. **G** updates for selection, mutation input (**M**) and drift.
**TR-MP5 Allele frequencies.** Per locus: selection using allelic marginal fitness from the surrogate sample, then **Wright–Fisher sampling** with effective size $N_e$ (computed from sex ratio and variance in reproductive success), then mutation.
**TR-MP6 Migration.** Stepping-stone dispersal between neighbouring patches, with rates from dispersal traits and landscape resistance (terrain, water, biome suitability); migrants carry allele frequencies and trait means.
**TR-MP7 Approximations.** Mode P assumes Hardy–Weinberg and linkage equilibrium within demes except for what the reservoir retains. This MUST be documented in the UI ("Population mode: statistical approximation").

### 5.12 Macro mode (Mode M)

**TR-MM1 Unit.** A **population lineage**: a geographically connected set of demes of one species. State: abundance, range (tile mask at coarse resolution), $\bar{z}$, **G**, neutral divergence matrix to other lineages, incompatibility count to sister lineages.
**TR-MM2 Trait evolution.** Multivariate Ornstein–Uhlenbeck: $\Delta\bar{z} = \alpha(\theta(\text{env}, \text{community}) - \bar{z})\,\Delta t + \text{drift}$, where the optimum θ is obtained by **numerical optimisation of the Mode-P fitness model** for the lineage's environment (cached per environment class) and the drift variance is $\mathbf{G}/N_e$ per generation. α is derived from **G** and the curvature of the fitness surface (Lande 1976).
**TR-MM3 Abundance.** Logistic growth toward carrying capacity K computed from habitat area × productivity × trophic efficiency (≈ 10% per level) and body-mass scaling of density (Damuth's rule, N ∝ M^−0.75), with environmental stochasticity.
**TR-MM4 Speciation.** When a lineage's range is split by a barrier, it becomes two lineages. Neutral divergence grows at ≈ 2μt; incompatibilities accumulate at a rate quadratic in the number of substitutions; ecological divergence adds mating-timing and preference divergence. When reproductive compatibility < τ, a speciation event is recorded. Peripatric and sympatric speciation use a per-lineage hazard that depends on ecological niche width and sexual-selection strength (calibrated from Mode P runs).
**TR-MM5 Extinction.** Per step, extinction probability from a population viability approximation (abundance, growth-rate variance, range size, climate mismatch $\|\theta - \bar z\|$).
**TR-MM6 Interactions.** Community-level competition via niche overlap (traits + range overlap) and predator–prey coupling via generalised Lotka–Volterra on lineage abundances.

### 5.13 Mode transitions

**TR-X1 I → P (aggregate).** Per species per patch: count individuals into classes; compute allele frequencies, $\bar z$, **G** (from additive genetic values, not phenotypes); fill the haplotype reservoir by stratified sampling of genomes; store mean condition.
**TR-X2 P → I (materialise).** For each deme create *N* individuals:
- genomes by recombining reservoir haplotypes, then importance-resampling to match current allele frequencies (iterative proportional fitting over loci, max 10 iterations),
- age/sex from classes; body condition and fat reserves sampled around the deme mean,
- positions by sampling habitat suitability within the patch; group/pair bonds re-formed by a short warm-up (≤ 1 sim-day, hidden).
**TR-X3 P ↔ M.** P → M merges connected demes into lineages (abundance-weighted means; pooled **G**). M → P distributes abundance over suitable habitat using the lineage's niche model; allele frequencies are synthesised from neutral-divergence and trait-mean constraints (marker loci drawn to reproduce stored pairwise distances; functional loci set to reproduce $\bar z$ with **G**).
**TR-X4 Conservation tolerances** (checked automatically after each transition in debug and CI builds):

| Quantity | Tolerance |
|---|---|
| Total abundance per species | ± 1% (I↔P), ± 5% (P↔M) |
| Mean trait value | ± 0.05 phenotypic SD (I↔P), ± 0.1 SD (P↔M) |
| Expected heterozygosity (neutral loci) | ± 5% (I↔P), ± 10% (P↔M) |
| Pairwise F_ST between demes | ± 0.02 |
| Species identity and phylogeny | Exact (no transition may create or remove species) |

### 5.14 Humans and culture (P1)

**TR-HU1** Hominin templates use the same model with the hominin trait group (TR-B2). Brain energy cost (TR-B3) MUST apply, so encephalisation can only evolve when diet quality, cooking or food sharing pays for it.
**TR-HU2 Culture.** Cultural traits are units (technique, tool type, food-processing method, fire use) with: energy/hunting benefit, learning difficulty, transmission fidelity. Transmission vertical (parents), oblique (elders) and horizontal (peers), weighted by social learning traits and group size. Copy errors produce cultural "mutations"; innovations arise with a small probability scaled by brain size and learning traits. Group size and connectivity affect cultural retention (Henrich 2004 effect, emergent).
**TR-HU3 Gene–culture co-evolution.** Culture changes the selective environment (cooking raises digestibility → reduced selection for gut length; clothing reduces cold stress → relaxed selection on hair density; projectile tools change hunting success).
**TR-HU4 Skin pigmentation.** Fitness effects from UV-driven folate degradation (dark favoured at high UV) vs. vitamin-D synthesis (light favoured at low UV), using tile UV index from insolation.
**TR-HU5** Civilisation-level systems (agriculture, settlements) are out of scope.

## 6. Performance requirements

| ID | Requirement |
|---|---|
| TR-PF1 | Mode I: ≥ 250 ticks/s with 100,000 individuals and 1024×512 world on reference hardware; ≥ 1,000 ticks/s with 10,000 individuals |
| TR-PF2 | Mode P: ≥ 100 sim-years/s with 10,000 deme-species |
| TR-PF3 | Mode M: ≥ 10,000 sim-years/s with 2,000 lineages; floor 2,000 y/s on the largest world |
| TR-PF4 | World generation (1024×512, incl. erosion and climate): ≤ 20 s |
| TR-PF5 | Mode transition I↔P: ≤ 2 s for 100,000 individuals; P↔M ≤ 1 s |
| TR-PF6 | UI frame rate ≥ 30 FPS (target 60) during simulation; engine runs on threads separate from the UI and renderer |
| TR-PF7 | Engine → UI snapshot latency ≤ 100 ms; snapshots limited to the visible viewport plus aggregates |
| TR-PF8 | Memory: ≤ 4 GB engine RSS at 100,000 individuals including history buffers |
| TR-PF9 | Save ≤ 5 s, load ≤ 10 s for the reference world; autosave MUST NOT pause the simulation for more than 200 ms (copy-on-write snapshot, background write) |
| TR-PF10 | A performance regression > 10% on the benchmark suite MUST fail CI |

## 7. Determinism and reproducibility

| ID | Requirement |
|---|---|
| TR-D1 | Given engine version, parameter-set hash, world seed and intervention log, the simulation MUST be bit-for-bit reproducible on the same platform |
| TR-D2 | Cross-platform determinism (x86-64 vs. ARM64, native vs. WASM) SHOULD hold; achieved by `libm`, no fused multiply-add assumptions (FMA disabled or used explicitly everywhere), no fast-math, fixed reduction order |
| TR-D3 | All randomness MUST come from the counter-based RNG keyed by `(world_seed, tick, entity_id, stream_id)`; no thread-local or time-based RNG |
| TR-D4 | Parallel systems MUST write to per-entity slots or per-thread buffers merged in a deterministic order (sorted by entity ID) |
| TR-D5 | Entity IDs MUST be allocated deterministically (monotonic 64-bit counter, never reused) |
| TR-D6 | Hash maps that are iterated MUST use deterministic hashing and ordering (`IndexMap` / `BTreeMap` or sorted iteration) |
| TR-D7 | A CI job MUST run the same scenario twice (and on two platforms) and compare state hashes every 1,000 ticks |
| TR-D8 | User interventions MUST be recorded with the exact tick at which they apply; the UI's command is queued and applied at the start of the next tick |

## 8. Data and persistence

**TR-DA1 Save container.** One `.evo` file = SQLite database with:

| Table / blob | Content |
|---|---|
| `meta` | engine version, schema version, parameter-set hash, seed, realism dial, created/updated, current tick, mode |
| `world` | terrain, hydrology, climate baselines (zstd blobs, chunked 64×64 tiles) |
| `env_state` | dynamic fields: vegetation, fertility, water, current climate offsets (chunked blobs) |
| `allele` | global allele table |
| `entity_chunk` | Mode I ECS state, columnar per component, zstd-compressed, chunked by spatial region |
| `deme`, `lineage` | Mode P / M state |
| `species`, `phylogeny` | species registry and tree edges |
| `event` | chronicle events and interventions (tick, type, payload JSON) |
| `metric` | time series (species abundance, trait means, heterozygosity …) downsampled by level (per year, per 100 y, per 10 ky) |
| `checkpoint` | references to full state snapshots used for rewind/branching |
| `branch` | timeline branches (parent checkpoint, label) |

**TR-DA2** Schema migrations MUST be versioned; newer engines MUST load older saves (read-only for histories if the model changed, with a warning that replay may differ).
**TR-DA3 Checkpoints.** Automatic checkpoints at increasing spacing (log-scale: recent ones dense, old ones sparse). Default disk budget 2 GB per world, configurable.
**TR-DA4 Exports.** CSV (metrics, traits, individuals), Newick and NeXML (phylogeny), JSON (templates), PNG (map layers).
**TR-DA5 Templates** are JSON/TOML files validated against a published JSON Schema: world generation parameters or painted heightmap (PNG 16-bit), climate settings, founder species definitions, events schedule.

## 9. Validation and testing requirements

### 9.1 Scientific validation suite (MUST run in CI, headless)

Each test runs many seeds and passes when results fall within statistical tolerance of theory or of the known pattern.

| ID | Test | Expected result | Mode | Priority |
|---|---|---|---|---|
| V-G1 | Hardy–Weinberg: random mating, no selection, large N | Genotype frequencies ≈ p², 2pq, q² (χ² test, p > 0.01 in ≥ 95% of seeds) | I, P | P0 |
| V-G2 | Neutral fixation probability | P(fixation) of a new neutral allele ≈ 1/(2N) | I, P | P0 |
| V-G3 | Drift: heterozygosity decay | $H_t \approx H_0 (1 - 1/(2N_e))^t$ | I, P | P0 |
| V-G4 | Fixation probability of beneficial allele | ≈ $\frac{1-e^{-2s}}{1-e^{-4Ns}}$ (Kimura) | I, P | P0 |
| V-G5 | Breeder's equation | Response to truncation selection R ≈ h²S over first 5 generations | I | P0 |
| V-G6 | Linkage | Recombination fraction between loci matches Haldane map function | I | P0 |
| V-G7 | Mutation–drift equilibrium | Expected heterozygosity ≈ 4Nμ/(1+4Nμ) | P | P1 |
| V-G8 | Isolation by distance | F_ST increases with distance in a stepping-stone world | P | P1 |
| V-E1 | Predator–prey dynamics | Sustained or damped oscillations with predator lag in Lotka–Volterra-like regime | I, P | P0 |
| V-E2 | Carrying capacity | Herbivore abundance tracks NPP; density ∝ M^−0.75 across body sizes | I, P | P1 |
| V-E3 | Competitive exclusion / character displacement | Two competitors on one resource axis: exclusion or trait divergence | I, P | P1 |
| V-B1 | Bergmann's rule | Cold populations evolve larger body mass than warm populations (≥ 70% of seeds) | P | P0 |
| V-B2 | Allen's rule | Cold populations evolve smaller appendages | P | P1 |
| V-B3 | Island rule | Large-mammal founders dwarf, small-mammal founders grow on small predator-free islands | P, M | P1 |
| V-B4 | Camouflage | Coat luminance converges to background luminance under visual predation | I, P | P0 |
| V-B5 | Pathogen virulence trade-off | Intermediate virulence evolves | I, P | P1 |
| V-S1 | Allopatric speciation | Barrier-separated populations become separate species; time scales with 1/μ and N as expected | P, M | P0 |
| V-S2 | Hybrid zone | Narrow stable cline at secondary contact with incompatibilities | I, P | P1 |
| V-S3 | Snowball | Number of incompatibilities grows faster than linearly with divergence time | P | P1 |
| V-P1 | I ↔ P conservation | Tolerances of TR-X4 hold | I, P | P0 |
| V-P2 | P ↔ M conservation | Tolerances of TR-X4 hold | P, M | P1 |
| V-P3 | Surrogate calibration | Mode P trajectories (abundance, trait mean) over 200 years fall within the 90% envelope of Mode I trajectories from the same start | I, P | P0 |
| V-H1 | Pigmentation cline | Hominin skin pigmentation correlates with UV index | P | P1 |
| V-H2 | Culture and group size | Cultural repertoire size increases with population connectivity | I, P | P2 |

### 9.2 Engineering tests

- **Unit tests** per system (meiosis, mutation, energy budget, climate formulas) with property-based tests (`proptest`) for invariants (energy never NaN, allele frequencies sum to 1, no negative abundances).
- **Determinism tests** (TR-D7).
- **Benchmarks** (`criterion`) for TR-PF1–PF5, run on dedicated CI hardware.
- **Soak test:** 10 million sim-years in mixed modes without crash, leak (> 5% RSS growth after warm-up) or invariant violation.
- **Fuzzing** of save loading and template parsing (`cargo-fuzz`).
- **UI tests:** Playwright end-to-end on main journeys (PRD J1–J5).

## 10. Engine interface requirements

**TR-API1** The engine exposes a message-based API (FlatBuffers schemas versioned in `/schema`). The same API is used by the UI, the CLI and tests.

| Category | Messages (examples) |
|---|---|
| Lifecycle | `CreateWorld(template \| params, seed)`, `LoadWorld(path)`, `SaveWorld(path)`, `CloseWorld` |
| Time | `Play`, `Pause`, `Step(n)`, `SetSpeed(sim_seconds_per_real_second)`, `LockMode(I\|P\|M\|Auto)`, `RunUntil(condition)`, `Cancel` |
| Branching | `CreateCheckpoint(label)`, `Rewind(checkpoint_id)`, `Branch(checkpoint_id, label)`, `SwitchBranch(id)` |
| Population | `AddFounders(species_def, count, region, diversity)`, `RemoveSpecies(id, region)` |
| Interventions | `ApplyEvent(type, params, region, at_tick)` |
| Viewport | `SetViewport(bounds, zoom, layers)`, `SetFocusRegion(bounds)` |
| Queries | `InspectEntity(id)`, `GetSpecies(id)`, `GetPhylogeny(range)`, `GetMetrics(series, range, resolution)`, `GetLayer(layer, bounds, resolution)`, `ExplainChange(species, trait, range)` |
| Streams (engine → UI) | `FrameSnapshot` (visible entities: id, pos, heading, species, appearance, action), `Tick/ClockUpdate`, `ModeChanged`, `Event`, `Progress`, `Warning`, `Error` |

**TR-API2** Commands MUST be idempotent where possible and acknowledged with a correlation ID. Long operations report progress at ≥ 2 Hz and MUST be cancellable.
**TR-API3** CLI: `evo run --template earth --seed 42 --years 1e6 --mode auto --out run.evo --export-csv metrics/`.

## 11. Security, privacy and telemetry

- **TR-SEC1** Fully offline operation; no network access required.
- **TR-SEC2** Templates and saves from others are untrusted input: strict schema validation, size limits (templates ≤ 50 MB, saves ≤ 20 GB), no executable content. Mods with code are out of scope until a sandbox (WASM) exists.
- **TR-SEC3** Optional, opt-in anonymous telemetry (crash reports, performance stats); no personal data; can be disabled at any time.
- **TR-SEC4** Online gallery (P2) requires accounts, moderation, and signed uploads; specified separately.

## 12. Assumptions and constraints

- Abstract genes (not nucleotide sequences) are an accepted simplification; mutation rates are per locus per generation.
- Individual-level simulation of every organism over millions of years is infeasible; multi-fidelity modelling (§4.2) is the core technical bet. The surrogate calibration test (V-P3) is the main guard against the modes disagreeing.
- Parameter values from the literature are for real Earth animals; for other world settings (different gravity, day length) only parameters with a clear physical dependency are changed.

## 13. Traceability (PRD → TRD)

| PRD area | TRD sections |
|---|---|
| 7.1 World creation (FR-W*) | §5.1, §5.2, §5.3, §8 (templates) |
| 7.2 Species setup (FR-S*) | §5.4, §5.5 (TR-B3 constraints), §10 `AddFounders` |
| 7.3 Life simulation (FR-L*) | §5.6, §5.7, §5.8, §5.9 |
| 7.4 Genetics and evolution (FR-G*) | §5.4, §5.10, §5.11, §5.12 |
| 7.5 Humans (FR-H*) | §5.14 |
| 7.6 Environment / interventions (FR-E*) | §5.2, §5.3, §10 `ApplyEvent` |
| 7.7 Time control (FR-T*) | §4, §5.11–5.13, §6, §8 checkpoints |
| 7.8 Observation (FR-O*) | §5.10, §8 metrics, §10 queries |
| 7.9 Persistence (FR-P*) | §7, §8 |
| 7.10 Settings (FR-X*) | TR-G6 realism dial |

## 14. References (selected)

- Aiello, L. C. & Wheeler, P. (1995). The expensive-tissue hypothesis. *Current Anthropology*.
- Calder, W. A. (1984). *Size, Function, and Life History*.
- Damuth, J. (1981). Population density and body size in mammals. *Nature*.
- Haller, B. C. & Messer, P. W. (2019). SLiM 3: forward genetic simulations beyond the Wright–Fisher model. *Mol. Biol. Evol.*
- Henrich, J. (2004). Demography and cultural evolution. *American Antiquity*.
- Hirt, M. R. et al. (2017). A general scaling law reveals why the largest animals are not the fastest. *Nat. Ecol. Evol.*
- Kimura, M. (1962). On the probability of fixation of mutant genes in a population. *Genetics*.
- Kleiber, M. (1932). Body size and metabolism. *Hilgardia*.
- Lande, R. (1976, 1979); Lande, R. & Arnold, S. J. (1983). Quantitative genetics of multivariate evolution.
- Lieth, H. (1975). Modeling the primary productivity of the world.
- Nagy, K. A. (1987). Field metabolic rate and food requirement scaling in mammals and birds. *Ecol. Monogr.*
- Orr, H. A. (1995). The population genetics of speciation: the evolution of hybrid incompatibilities. *Genetics*.
- Taylor, C. R., Heglund, N. C. & Maloiy, G. M. O. (1982). Energetics and mechanics of terrestrial locomotion. *J. Exp. Biol.*
- West, G. B., Brown, J. H. & Enquist, B. J. (2001). A general model for ontogenetic growth. *Nature*.
- Jablonski, N. G. & Chaplin, G. (2000). The evolution of human skin coloration. *J. Hum. Evol.*
