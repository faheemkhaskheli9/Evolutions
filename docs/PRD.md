# Evolutions — Product Requirements Document (PRD)

| Field | Value |
|---|---|
| Document | Product Requirements Document |
| Product | Evolutions — a realistic biological evolution simulator |
| Version | 0.1 (Draft) |
| Date | 2026-10-15 |
| Status | Proposed |
| Related | [Technical Requirements (TRD)](./TRD.md) · [System Design](./SYSTEM_DESIGN.md) |

---

## 1. Summary

Evolutions is a simulation of the living world. The user creates a world (random, fully custom, or from a template), seeds it with animals and/or early humans, and watches them live: they forage, hunt, drink, rest, compete, court, mate, raise young, get sick, age and die. Every organism carries a real diploid genome. Offspring inherit a recombined mix of their parents' genes plus new mutations, so the population changes over generations under natural selection, sexual selection, genetic drift, gene flow and mutation, the same forces that act in the real world.

Nothing is scripted. Adaptations, new species, extinctions, predator–prey arms races, island dwarfism, or cold-adapted fur appear only when the simulated environment favours them. The user can speed time from real-time observation up to millions of years per minute, so they can see evolution happen.

**One-line pitch:** *"A living Earth in a box: build a world, release life into it, and fast-forward through deep time to see what it becomes."*

## 2. Problem and opportunity

- Evolution is one of the most important ideas in science and one of the most often misunderstood ("animals choose to adapt", "evolution has a goal", "humans descended from chimpanzees").
- Existing products sit at two extremes:
  - **Games** (Spore, Species: ALRE, Equilinox, Thrive) are fun but often use teleological or player-driven mechanics, abstract genetics, or unrealistic timescales.
  - **Scientific tools** (SLiM, NEMO, simuPOP, AVIDA) are rigorous but have no world, no visual organisms, and require programming.
- **Opportunity:** a product that is scientifically grounded (genetics, physiology, ecology), visual and explorable, and able to cover timescales from one afternoon of an animal's life to millions of years.

## 3. Goals and non-goals

### 3.1 Goals
1. **Realism first.** Evolution must emerge from mechanisms (heredity, variation, differential survival and reproduction), not from scripted outcomes. Biological, physiological and ecological rules are based on published models.
2. **Deep time.** Users can move smoothly from watching one animal to fast-forwarding millions of years.
3. **Creative control over starting conditions:** worlds, climates and founder species. No control over the outcome.
4. **Explainability.** Users can always ask *why*: why a trait spread, why a species split, why a population crashed.
5. **Reproducibility.** The same seed and settings produce the same history. Worlds can be saved, branched, shared and replayed.

### 3.2 Non-goals (for v1)
- Civilization, cities, agriculture, or technology beyond the Paleolithic (possible future expansion).
- Molecular or cellular simulation (protein folding, cell biology). The genome is an abstract but realistic model of genes and alleles.
- Origin-of-life or abiogenesis. Simulations start from existing multicellular organisms.
- Full fluid-dynamics weather or geophysics. Climate uses physically motivated simplified models.
- Multiplayer real-time play (sharing worlds asynchronously is in scope).
- Fully aquatic or aerial ecosystems in MVP. MVP covers terrestrial vertebrates and plants; flight and aquatic body plans are planned for later.

## 4. Target users and personas

| Persona | Description | Primary need |
|---|---|---|
| **Curious Explorer** (primary) | Gamer or science fan, 16–45, likes sandbox and simulation games | Build worlds, see surprising outcomes, share stories and screenshots |
| **Educator / Student** | High-school or university biology teacher and their students | Demonstrate natural selection, drift, speciation and Hardy–Weinberg with reproducible scenarios and clear graphs |
| **Hobby Scientist** | Hobby or academic researcher, data-literate | Run controlled experiments, vary parameters, export data (CSV, Newick trees) |
| **Content Creator** | YouTuber or streamer | Dramatic, watchable evolution timelapses; cinematic camera; replays |

## 5. Key concepts (glossary)

| Term | Meaning |
|---|---|
| **World** | A planet or region with terrain, water, climate, seasons and vegetation |
| **Template** | Predefined world or species setup (for example "Earth-like Continent", "Galápagos Archipelago", "Ice Age Eurasia") |
| **Organism / Individual** | One simulated animal or human with a genome, a body (phenotype), needs, behaviour and life history |
| **Genome** | Diploid chromosomes made of genes (loci) with alleles; inherited through meiosis and recombination; mutates |
| **Phenotype** | Observable traits (size, fur, speed, diet, brain size, colour …) developed from genes and environment |
| **Deme / Population** | A local interbreeding group in one area |
| **Species** | Groups that are reproductively isolated from each other, detected automatically by the simulation |
| **Lineage / Phylogeny** | The family tree of species over time |
| **Simulation fidelity mode** | Individual (every animal simulated), Population (groups simulated statistically), Macro (species and lineages over deep time). See §7.7 |
| **Intervention** | A user action that changes conditions (for example meteor impact, climate shift, introducing a species). Never a direct change to evolution itself |

## 6. User journeys

### J1 — "Quick start: random world"
1. User clicks **New World → Random**, picks a size and optionally a seed.
2. A world is generated with continents, mountains, rivers, biomes and climate, plus a randomly balanced starter ecosystem (plants, herbivores, predators).
3. The simulation starts at 1× speed. The user follows a rabbit-like herbivore and watches it eat, drink, flee a predator and mate.
4. User switches to **1,000 years/sec**. Population graphs show predator–prey cycles. After 50,000 years the herbivores in the northern cold zone are larger with thicker fur (Bergmann's rule emerging), and the app announces: *"New species detected: Lineage H-2 split from H-1 (isolated by mountain range)."*

### J2 — "Galápagos experiment" (template)
1. User picks the **Archipelago** template and a "Finch-like ground bird" founder species.
2. Founders colonise islands with different seed sizes. Over thousands of generations the beak-depth distributions on different islands diverge.
3. User opens **Trait Explorer**, sees beak depth vs. island seed hardness, and exports the data as CSV.

### J3 — "Human origins"
1. User picks the **East African Rift** template with an "Early Hominin" founder population and an ape-like relative.
2. Climate variability (Milankovitch cycles) drives cycles of forest and savanna. Hominins in open habitats show selection for bipedal efficiency, larger home ranges and tool use; brain size increases only where diet quality can pay its energy cost.
3. Cultural traits (tool techniques) spread through social learning, and the user sees a "culture tree" next to the genetic tree.

### J4 — "Custom world"
1. User opens the **World Editor**: paints terrain height, places oceans, sets axial tilt, solar output, CO₂ level and starting temperature.
2. In the **Species Designer** they build a founder species from body-plan parameters. Biological constraints are enforced (for example a 500 kg animal cannot run 120 km/h; metabolic cost scales with mass).
3. They save the world as a reusable template and share it.

### J5 — "What if?"
1. After 2 million simulated years, the user creates a **branch** (timeline fork).
2. In the branch they trigger a meteor impact with global cooling, then run both timelines and compare the phylogenies side by side.

## 7. Functional requirements

Priority: **P0** = MVP, **P1** = v1.0, **P2** = later.

### 7.1 World creation

| ID | Requirement | Priority |
|---|---|---|
| FR-W1 | Create a **random world** from a seed with parameters: size, land/ocean ratio, roughness, mean temperature, humidity | P0 |
| FR-W2 | Create a world from a **template** (minimum set in §7.1.1) | P0 |
| FR-W3 | **World Editor**: paint elevation, water, biomes and resource richness; set climate parameters (axial tilt, solar constant, greenhouse level, day length, year length) | P1 |
| FR-W4 | Procedural generation produces physically plausible terrain (tectonic-like mountain ranges, river networks flowing downhill to sea, lakes in basins) | P0 |
| FR-W5 | Climate derived from latitude, elevation (lapse rate), distance to ocean and prevailing winds (rain shadows) | P0 |
| FR-W6 | Seasons from axial tilt; day/night cycle | P0 |
| FR-W7 | Long-term climate change: glacial/interglacial cycles, sea-level change, opening and closing of land bridges | P1 |
| FR-W8 | Slow continental drift over millions of years (Macro mode only) | P2 |
| FR-W9 | Save a custom world as a **user template**; import and export templates | P1 |

#### 7.1.1 Launch templates
- **Earth-like Continent**: temperate default with balanced ecosystem.
- **Archipelago** (Galápagos-like): island isolation and adaptive radiation.
- **Pangaea**: one supercontinent, with optional break-up in Macro mode.
- **Ice Age Eurasia**: cold steppe, glacial cycles, land bridges.
- **East African Rift**: hominin origins with variable climate.
- **Island Dwarfism Lab**: small isolated islands with large-mammal founders.
- **Desert & Oasis**: water scarcity, physiological adaptation.
- **Blank Slate**: flat world, uniform climate (for controlled experiments and teaching).

### 7.2 Organisms and species setup

| ID | Requirement | Priority |
|---|---|---|
| FR-S1 | Seed a world with founder populations from a **species library** (for example rabbit-like herbivore, deer-like grazer, wolf-like pack hunter, big-cat ambush predator, rodent, ground bird, early hominin, ape) | P0 |
| FR-S2 | **Species Designer**: define a founder species by body-plan parameters (mass, diet, locomotion, insulation, sociality, reproductive strategy …), which are turned into a starting genome with realistic standing genetic variation | P1 |
| FR-S3 | Constraint validation: designer rejects or warns on biologically impossible combinations (allometry, energy budget, gestation vs. body mass) | P1 |
| FR-S4 | Set founder count, sex ratio, age structure, location, and genetic diversity (high diversity vs. bottlenecked founders) | P0 |
| FR-S5 | Plants are simulated as a resource layer with growth, seasonality and grazing pressure; plants have simplified evolution of defences (toxicity, toughness) | P0 resource layer / P2 plant evolution |

### 7.3 Life simulation (individual level)

Every individual must:

| ID | Requirement | Priority |
|---|---|---|
| FR-L1 | **Eat**: forage plants, scavenge, or hunt according to diet traits; digestive efficiency depends on genes (gut type, dentition) | P0 |
| FR-L2 | **Drink**: maintain hydration; water loss depends on temperature, activity, body size and kidney efficiency traits | P0 |
| FR-L3 | **Energy and metabolism**: metabolic rate scales with body mass (Kleiber's law); energy stored as fat reserves; starvation causes death | P0 |
| FR-L4 | **Thermoregulation**: heat and cold stress depend on insulation, body size, surface area, and endo-/ectothermy | P0 |
| FR-L5 | **Rest and sleep**, with activity patterns (diurnal, nocturnal, crepuscular) as heritable traits | P1 |
| FR-L6 | **Movement and senses**: speed, stamina, vision, smell and hearing ranges determined by phenotype and affect foraging, hunting and predator detection | P0 |
| FR-L7 | **Predation**: chase and escape outcomes from relative speed, stamina, detection, camouflage, group defence, size and weapons | P0 |
| FR-L8 | **Social behaviour**: herding, pack hunting, territoriality, dominance hierarchies, kin recognition (heritable tendencies) | P1 |
| FR-L9 | **Reproduction**: sexual maturity, oestrous/breeding seasons, courtship, **mate choice** (sexual selection), mating, gestation or egg incubation, birth, litter/clutch size, parental care, weaning | P0 |
| FR-L10 | **Growth and development**: juveniles grow toward adult size; growth depends on nutrition (phenotypic plasticity) | P0 |
| FR-L11 | **Ageing and death**: senescence (age-dependent mortality), starvation, dehydration, predation, disease, injury, exposure, accidents | P0 |
| FR-L12 | **Disease and parasites**: transmissible pathogens with host resistance genes and pathogen evolution (co-evolutionary arms race) | P1 |
| FR-L13 | **Learning**: individuals learn from experience (for example avoid toxic food, remember water sources); social species learn from others | P1 |
| FR-L14 | **Migration and dispersal**: natal dispersal and seasonal migration as heritable behaviour | P1 |

### 7.4 Genetics and evolution

| ID | Requirement | Priority |
|---|---|---|
| FR-G1 | Each individual has a **diploid genome** (pairs of chromosomes, genes at loci, two alleles per locus); optional haplodiploid or asexual modes for special templates | P0 |
| FR-G2 | **Mendelian inheritance** with meiosis, independent assortment and **recombination / crossing over** (linked genes are inherited together more often) | P0 |
| FR-G3 | **Mutation**: point mutations (allele effect changes), gene duplication, deletion; mutation rate itself is heritable within bounds | P0 (point) / P1 (structural) |
| FR-G4 | **Quantitative (polygenic) traits**: most traits are controlled by many genes with small effects plus environmental variance; dominance, epistasis and pleiotropy supported | P0 |
| FR-G5 | **Trade-offs and constraints**: pleiotropy and energy costs create real trade-offs (big brain costs energy; speed vs. endurance; litter size vs. offspring quality) | P0 |
| FR-G6 | **Natural selection** emerges from differential survival and reproduction. No fitness function is written by hand | P0 |
| FR-G7 | **Sexual selection**: heritable preferences and ornaments; runaway selection possible | P1 |
| FR-G8 | **Genetic drift**: stronger in small populations; bottlenecks and founder effects visible | P0 |
| FR-G9 | **Gene flow** between populations through dispersal; barriers (mountains, water, deserts) reduce it | P0 |
| FR-G10 | **Speciation**: reproductive isolation accumulates through genetic divergence (genetic incompatibilities, mate-preference divergence, timing differences). Allopatric, parapatric and sympatric speciation are all possible. Hybrid zones are possible | P0 (allopatric) / P1 (others) |
| FR-G11 | **Automatic species detection** and naming; species tree (phylogeny) maintained with split times | P0 |
| FR-G12 | **Extinction** from demographic collapse; extinct species remain in the phylogeny and the "fossil record" | P0 |
| FR-G13 | **Phenotypic plasticity** and maternal effects (for example nutrition affects adult size) | P1 |
| FR-G14 | **Co-evolution**: predator–prey, host–parasite, plant–herbivore arms races | P1 |

### 7.5 Humans

| ID | Requirement | Priority |
|---|---|---|
| FR-H1 | Humans and hominins are **animals under the same rules**: genome, physiology, ecology. No special-casing of evolution | P0 |
| FR-H2 | Hominin-specific heritable traits: bipedalism efficiency, brain size (with high energy cost), extended childhood, hand dexterity, sweat-based cooling, hair density, skin pigmentation (UV vs. vitamin-D trade-off with latitude), lactase persistence (when herding is present, P2) | P1 |
| FR-H3 | **Cultural evolution**: tools, fire use, food processing and hunting techniques are learned behaviours passed on socially with imperfect fidelity; culture changes the selective environment (gene–culture co-evolution, for example cooking reducing gut size) | P1 |
| FR-H4 | Social groups (bands), cooperation, kin structure, food sharing | P1 |
| FR-H5 | Proto-language as a communication-efficiency trait that improves cultural transmission (abstract model) | P2 |

### 7.6 Environment dynamics and interventions

| ID | Requirement | Priority |
|---|---|---|
| FR-E1 | Vegetation grows according to climate (temperature, rainfall, sunlight, season) and recovers from grazing; overgrazing degrades habitat | P0 |
| FR-E2 | Water sources: rivers, lakes, seasonal ponds; droughts | P0 |
| FR-E3 | Natural events: fires, floods, droughts, harsh winters, volcanic eruptions | P1 |
| FR-E4 | **User interventions** ("god tools"): meteor impact, volcanic winter, climate shift (± °C), sea-level change, introduce or remove a species, create or remove a barrier, plague outbreak | P1 |
| FR-E5 | Interventions are logged as events visible on the timeline and in all charts | P1 |
| FR-E6 | Interventions may change **conditions only**, never genomes directly. Direct genetic edits are an optional, clearly labelled "Sandbox Lab" mode that is excluded from the realism score | P2 |

### 7.7 Time control and fast-forward

| ID | Requirement | Priority |
|---|---|---|
| FR-T1 | Play, pause, step | P0 |
| FR-T2 | Speed control with presets: real-time-ish (1 sim-minute/sec), 1 day/s, 1 year/s, 100 y/s, 10,000 y/s, 1 My/s | P0 |
| FR-T3 | **Jump to future**: "advance N years" or "run until event" (for example until a speciation, an extinction, population < X, or a trait threshold) | P0 |
| FR-T4 | **Automatic fidelity switching**: at high speeds the engine moves from Individual mode to Population mode and then to Macro mode, and back again when the user slows down or zooms in. The user sees which mode is active and how it affects detail | P0 (Individual + Population) / P1 (Macro) |
| FR-T5 | **Focus region**: the area the user is watching runs at Individual fidelity while the rest of the world runs at Population fidelity | P1 |
| FR-T6 | **Rewind** to earlier checkpoints | P1 |
| FR-T7 | **Branch timelines** from any checkpoint and compare branches | P1 |
| FR-T8 | Background fast-forward with a progress bar, ETA and cancel; UI stays responsive | P0 |

### 7.8 Observation, analytics and explanation

| ID | Requirement | Priority |
|---|---|---|
| FR-O1 | 2D map view with layers (terrain, biomes, temperature, rainfall, vegetation, population density per species, gene/allele frequency heatmaps) | P0 |
| FR-O2 | 3D/2.5D view of terrain and organisms with procedurally generated appearance based on phenotype (size, proportions, colour, fur) | P1 |
| FR-O3 | **Inspector** for any individual: genome view, traits, needs, current goal, family tree, life history, cause of death | P0 |
| FR-O4 | **Species panel**: population over time, range map, mean traits, genetic diversity, time since origin, ancestors and descendants | P0 |
| FR-O5 | **Phylogeny viewer**: interactive tree of life over time, with extinct branches; export as Newick | P0 |
| FR-O6 | **Trait explorer**: distribution and history of any trait; compare populations; correlation with environment | P0 |
| FR-O7 | **Population-genetics charts**: allele frequencies, heterozygosity, F_ST, effective population size | P1 |
| FR-O8 | **"Why?" explanations**: for a trait change or speciation, show the evidence (selection gradient, survival and reproduction differences, isolation barrier, drift likelihood) in plain language | P1 |
| FR-O9 | **Event feed / chronicle**: notable events (new species, extinction, population crash, new adaptation, first tool use) with timestamps; click to jump there | P0 |
| FR-O10 | **Fossil record mode**: sparse, biased sampling of the past, for teaching what paleontologists actually see | P2 |
| FR-O11 | Data export: CSV (time series, traits), Newick (phylogeny), JSON (world/template) | P1 |
| FR-O12 | Cinematic camera / follow mode, screenshot and timelapse recording | P2 |

### 7.9 Persistence and sharing

| ID | Requirement | Priority |
|---|---|---|
| FR-P1 | Save and load full simulation state; autosave | P0 |
| FR-P2 | Deterministic replay from seed + settings + intervention log | P0 |
| FR-P3 | Share worlds and templates as files | P1 |
| FR-P4 | Online gallery for templates and notable histories | P2 |

### 7.10 Education and settings

| ID | Requirement | Priority |
|---|---|---|
| FR-X1 | **Realism dial**: "Strict" (real mutation rates, generation times) to "Accelerated" (mutation and timescale multipliers for faster results in classrooms). The current setting is always visible | P0 |
| FR-X2 | Guided lessons and scenarios (Hardy–Weinberg, peppered moth-style selection, founder effect, island rule, ring species) | P2 |
| FR-X3 | Tooltips explaining each biological concept with references | P1 |
| FR-X4 | Accessibility: colour-blind-safe palettes, scalable UI, keyboard navigation | P1 |

## 8. Realism principles (product rules)

These rules guide every design decision. A feature that breaks them needs explicit sign-off.

1. **No teleology.** Organisms never "decide" to evolve. Change happens only through inheritance with variation plus differential reproduction.
2. **No free lunch.** Every trait has a cost: energy, materials, development time, or trade-offs through pleiotropy.
3. **Mechanism over outcome.** Known biological patterns (Bergmann's rule, Allen's rule, island rule, Cope's rule, predator–prey cycles, character displacement) must *emerge*; they are used as **validation tests**, never coded directly.
4. **Grounded parameters.** Default rates come from literature (mutation rates, allometric scaling, metabolic rates, life-history scaling). Any acceleration is explicit and labelled.
5. **Stochasticity matters.** Chance (drift, mutation, accidents) is part of evolution; the same start with different seeds should give different, plausible histories.
6. **Honest abstraction.** Where we simplify (for example abstract genes instead of DNA sequences), the UI and docs say so.

## 9. Non-functional requirements (product-level)

Detailed in the [TRD](./TRD.md).

- **Platforms:** Desktop (Windows, macOS, Linux) first; web browser second (smaller worlds).
- **Scale:** at least 100,000 simultaneously simulated individuals on a mid-range 8-core desktop in Individual mode; worlds with millions of organisms through Population mode.
- **Speed:** 1 million simulated years in under 10 minutes in Macro mode on reference hardware.
- **Responsiveness:** UI interactions < 100 ms; rendering ≥ 30 FPS while the simulation runs.
- **Determinism:** identical results for identical seed, settings, intervention log and engine version.
- **Reliability:** autosave; no data loss on crash beyond the last autosave (default every 5 real minutes).
- **Privacy:** fully offline by default; no account required for local use.

## 10. Success metrics

| Metric | Target (6 months after v1.0) |
|---|---|
| Scientific validation suite pass rate (see TRD §9) | 100% of P0 tests |
| Median session length | ≥ 35 min |
| % of sessions reaching at least one speciation event | ≥ 60% |
| % of users who use fast-forward ≥ 1,000 years | ≥ 70% |
| Worlds and templates shared per week | ≥ 500 |
| Educator adoption (registered classroom users) | ≥ 200 classrooms |
| Crash-free sessions | ≥ 99.5% |
| Average rating | ≥ 4.3 / 5 |

## 11. Release plan

| Milestone | Scope | Indicative duration |
|---|---|---|
| **M0 — Science prototype** | Headless engine: genome, inheritance, mutation, simple world grid, herbivores + plants, Population mode; validation tests (Hardy–Weinberg, drift, breeder's equation) | 8 weeks |
| **M1 — MVP (P0)** | Random world + 4 templates, 6 founder species, Individual + Population modes with auto-switching, 2D map, inspector, species panel, phylogeny, save/load, speed control and jump-to-future | +16 weeks |
| **M2 — v1.0 (P1)** | Macro mode, world editor, species designer, humans and culture, disease, interventions, focus region, branching, 3D view, export, "Why?" explanations | +20 weeks |
| **M3 — v1.x (P2)** | Plant evolution, continental drift, flight/aquatic body plans, gallery, lessons, fossil mode, cinematic tools | Ongoing |

## 12. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Realism vs. performance: real evolution needs millions of years and huge populations | High | Multi-fidelity engine (Individual / Population / Macro) with validated transitions; focus region |
| Evolution is slow and the user sees "nothing happening" | High | Event chronicle, run-until-event, accelerated realism dial, highlighted trait changes |
| Results look unrealistic (monsters, runaway traits) | High | Allometric and energetic constraints; validation suite against known biological patterns; expert review |
| Model complexity makes the engine hard to debug | Medium | Determinism, per-system unit tests, replay, invariants and telemetry |
| Ecosystems collapse too often (everything goes extinct) | Medium | Template balancing, auto-tuned starter ecosystems, warnings ("predators will likely overhunt") rather than hidden rescue |
| Sensitive topics (human evolution, skin colour) | Medium | Neutral scientific framing, consultation with anthropologists, focus on adaptation to environment (UV) with clear explanations; no ranking of populations |

## 13. Open questions

1. Should plants fully evolve in v1 or stay a resource layer with simple defence evolution?
2. Art direction for organisms: stylised procedural creatures vs. more naturalistic rendering?
3. Monetisation: premium desktop app, education licences, or free/open core?
4. Scientific advisory board: which domains (population genetics, paleoanthropology, ecology)?
5. Do we support user scripting or mods (custom models) in v1?

## 14. Acceptance criteria (MVP)

- A user can create a random world and a template world, add at least two interacting species, and run for 100,000 simulated years without a crash.
- In the Blank Slate template, a neutral allele in a population of N = 100 is fixed or lost at rates consistent with theory (see TRD validation tests).
- In a two-region world separated by an impassable barrier, the two populations diverge and are automatically classified as separate species within the expected range of generations for the configured parameters.
- A cold-climate population of a founder species evolves larger body size and/or greater insulation than a warm-climate population of the same founders across ≥ 70% of seeds.
- Speed changes and Individual ↔ Population transitions keep population sizes and mean trait values continuous (within tolerance defined in the TRD).
- Saving, loading and replaying the same seed reproduce identical results.
