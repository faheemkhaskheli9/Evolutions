//! Scientific validation suite (TRD §9.1). Each test compares simulated outcomes
//! with population-genetics theory or with a known biological pattern.
//!
//! Fast tests run on every `cargo test`; long ecological tests are `#[ignore]`d and
//! run with `cargo test --release -- --ignored`.

use evo_engine::genetics::{AlleleTable, FounderSpec, Genome, GenomeArch};
use evo_engine::population::multinomial_resample;
use evo_engine::rng::Rng;
use evo_engine::traits::{Trait, N_TRAITS};
use evo_engine::world::{LandShape, WorldGenParams};
use evo_engine::{FounderGroup, Mode, NewWorld, Simulation};

fn arch(seed: u64, diversity: f32) -> (GenomeArch, AlleleTable) {
    let mut table = AlleleTable::default();
    let mut rng = Rng::new(seed);
    let mut mean = [0f32; N_TRAITS];
    let mut sd = [0.25f32; N_TRAITS];
    for t in Trait::ALL {
        mean[t as usize] = t.to_latent(if t == Trait::BodyMass { 5.0 } else { 0.5 });
    }
    sd[Trait::BodyMass as usize] = 0.2;
    let spec = FounderSpec {
        latent_mean: mean,
        latent_sd: sd,
        diversity,
    };
    let a = GenomeArch::create(0, &spec, &mut table, &mut rng);
    (a, table)
}

/// Random mating with real meiosis (no mutation, no selection).
fn next_generation(a: &GenomeArch, pop: &[Genome], n: usize, rng: &mut Rng) -> Vec<Genome> {
    (0..n)
        .map(|_| {
            let m = &pop[rng.below(pop.len())];
            let f = &pop[rng.below(pop.len())];
            Genome {
                hap: [a.meiosis(m, rng), a.meiosis(f, rng)],
            }
        })
        .collect()
}

/// V-G1 Hardy–Weinberg: after one round of random mating, genotype counts at a
/// biallelic contrast (most common allele vs. the rest) fit p², 2pq, q².
#[test]
fn v_g1_hardy_weinberg() {
    let mut passes = 0;
    let seeds = 20;
    for seed in 0..seeds {
        let (a, _) = arch(100 + seed, 1.0);
        let mut rng = Rng::new(seed);
        let founders: Vec<Genome> = (0..2000).map(|_| a.founder_genome(&mut rng)).collect();
        let pop = next_generation(&a, &founders, 2000, &mut rng);
        let l = a.neutral_loci[0];
        let target = pop[0].hap[0][l];
        let (mut aa, mut ab, mut bb) = (0f64, 0f64, 0f64);
        for g in &pop {
            match (g.hap[0][l] == target, g.hap[1][l] == target) {
                (true, true) => aa += 1.0,
                (false, false) => bb += 1.0,
                _ => ab += 1.0,
            }
        }
        let n = aa + ab + bb;
        let p = (2.0 * aa + ab) / (2.0 * n);
        let q = 1.0 - p;
        let exp = [p * p * n, 2.0 * p * q * n, q * q * n];
        let chi2: f64 = [aa, ab, bb]
            .iter()
            .zip(exp.iter())
            .map(|(o, e)| (o - e).powi(2) / e)
            .sum();
        if chi2 < 6.63 {
            passes += 1; // p > 0.01, 1 d.f.
        }
    }
    assert!(
        passes >= 18,
        "Hardy–Weinberg held in only {passes}/{seeds} seeds"
    );
}

/// V-G2 Neutral fixation probability ≈ 1/(2N).
#[test]
fn v_g2_neutral_fixation_probability() {
    let n = 50u64;
    let reps = 20_000;
    let mut rng = Rng::new(42);
    let mut fixed = 0;
    for _ in 0..reps {
        let mut pool = vec![
            (0u32, 1.0 - 1.0 / (2 * n) as f32),
            (1u32, 1.0 / (2 * n) as f32),
        ];
        for _ in 0..2000 {
            multinomial_resample(&mut pool, 2 * n, &mut rng);
            if pool.len() == 1 {
                break;
            }
        }
        if pool.len() == 1 && pool[0].0 == 1 {
            fixed += 1;
        }
    }
    let expected = reps as f64 / (2 * n) as f64;
    let sd = (reps as f64 * 0.01 * 0.99).sqrt();
    assert!(
        (fixed as f64 - expected).abs() < 4.0 * sd,
        "fixed {fixed}, expected {expected:.0} ± {:.0}",
        4.0 * sd
    );
}

/// V-G3 Heterozygosity decays as H_t ≈ H₀(1 − 1/2N)^t under drift (diploid meiosis).
#[test]
fn v_g3_heterozygosity_decay() {
    let n = 50usize;
    let t = 40;
    let reps = 120;
    let (a, _) = arch(7, 1.0);
    let mut ratio_sum = 0.0;
    for r in 0..reps {
        let mut rng = Rng::new(1000 + r);
        let mut pop: Vec<Genome> = (0..n).map(|_| a.founder_genome(&mut rng)).collect();
        let het = |pop: &[Genome]| -> f64 {
            // Expected heterozygosity 1 − Σp² averaged over neutral loci.
            a.neutral_loci
                .iter()
                .map(|&l| {
                    let mut c = std::collections::BTreeMap::new();
                    for g in pop {
                        *c.entry(g.hap[0][l]).or_insert(0.0) += 1.0;
                        *c.entry(g.hap[1][l]).or_insert(0.0) += 1.0;
                    }
                    let tot = 2.0 * pop.len() as f64;
                    1.0 - c.values().map(|x: &f64| (x / tot).powi(2)).sum::<f64>()
                })
                .sum::<f64>()
                / a.neutral_loci.len() as f64
        };
        let h0 = het(&pop);
        for _ in 0..t {
            pop = next_generation(&a, &pop, n, &mut rng);
        }
        ratio_sum += het(&pop) / h0;
    }
    let observed = ratio_sum / reps as f64;
    let expected = (1.0 - 1.0 / (2.0 * n as f64)).powi(t);
    assert!(
        (observed - expected).abs() < 0.05,
        "H_t/H_0 = {observed:.3}, expected {expected:.3}"
    );
}

/// V-G4 Fixation probability of a beneficial allele ≈ (1 − e^{−2s}) / (1 − e^{−4Ns}) (Kimura 1962).
#[test]
fn v_g4_beneficial_fixation_kimura() {
    let (n, s) = (100u64, 0.05f64);
    let reps = 6000;
    let mut rng = Rng::new(9);
    let mut fixed = 0;
    for _ in 0..reps {
        let mut p = 1.0 / (2 * n) as f64;
        loop {
            let ps = p * (1.0 + s) / (1.0 + p * s);
            let mut pool = vec![(0u32, (1.0 - ps) as f32), (1u32, ps as f32)];
            multinomial_resample(&mut pool, 2 * n, &mut rng);
            p = pool
                .iter()
                .find(|x| x.0 == 1)
                .map(|x| x.1 as f64)
                .unwrap_or(0.0);
            if p == 0.0 || p >= 1.0 {
                break;
            }
        }
        if p >= 1.0 {
            fixed += 1;
        }
    }
    let pf = (1.0 - (-2.0 * s).exp()) / (1.0 - (-4.0 * n as f64 * s).exp());
    let expected = reps as f64 * pf;
    let sd = (reps as f64 * pf * (1.0 - pf)).sqrt();
    assert!(
        (fixed as f64 - expected).abs() < 4.0 * sd,
        "fixed {fixed}, expected {expected:.0} ± {:.0}",
        4.0 * sd
    );
}

/// V-G5 Breeder's equation: response to truncation selection R ≈ h²·S.
#[test]
fn v_g5_breeders_equation() {
    let (a, table) = arch(11, 1.0);
    let t = Trait::BodyMass as usize;
    let mut rng = Rng::new(5);
    let n = 4000;
    let pop: Vec<Genome> = (0..n).map(|_| a.founder_genome(&mut rng)).collect();
    let genetic: Vec<f32> = pop.iter().map(|g| a.genetic_values(&table, g)[t]).collect();
    let pheno: Vec<f32> = genetic
        .iter()
        .map(|g| g + rng.normal_f32(0.0, a.env_sd[t]))
        .collect();
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let var = |v: &[f32]| {
        let m = mean(v);
        v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32
    };
    let h2 = var(&genetic) / var(&pheno);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| pheno[j].partial_cmp(&pheno[i]).unwrap());
    let parents: Vec<Genome> = order[..n / 5].iter().map(|&i| pop[i].clone()).collect();
    let s = mean(&order[..n / 5].iter().map(|&i| pheno[i]).collect::<Vec<_>>()) - mean(&pheno);
    let kids = next_generation(&a, &parents, n, &mut rng);
    let kid_mean = mean(
        &kids
            .iter()
            .map(|g| a.genetic_values(&table, g)[t])
            .collect::<Vec<_>>(),
    );
    let r = kid_mean - mean(&pheno);
    let realised = r / s;
    assert!(
        (realised - h2).abs() < 0.12,
        "realised heritability {realised:.3} vs h² {h2:.3}"
    );
    assert!(
        (0.35..0.65).contains(&h2),
        "founder h² {h2:.3} outside 0.35–0.65"
    );
}

/// V-G6 Recombination fraction between two loci matches Haldane's map function.
#[test]
fn v_g6_recombination_haldane() {
    let (a, _) = arch(13, 1.0);
    // Pick the two most distant loci on the longest chromosome.
    let c = (0..a.chrom_len.len())
        .max_by(|&x, &y| a.chrom_len[x].partial_cmp(&a.chrom_len[y]).unwrap())
        .unwrap();
    let (s, e) = a.chrom_range[c];
    let (l1, l2) = (s, e - 1);
    let d = (a.loci[l2].pos - a.loci[l1].pos) as f64;
    let mut g = Genome {
        hap: [vec![0; a.n_loci()], vec![1; a.n_loci()]],
    };
    g.hap[0][l1] = 10;
    g.hap[0][l2] = 20;
    g.hap[1][l1] = 11;
    g.hap[1][l2] = 21;
    let mut rng = Rng::new(3);
    let n = 40_000;
    let mut rec = 0;
    for _ in 0..n {
        let gam = a.meiosis(&g, &mut rng);
        let parental = (gam[l1] == 10 && gam[l2] == 20) || (gam[l1] == 11 && gam[l2] == 21);
        if !parental {
            rec += 1;
        }
    }
    let r = rec as f64 / n as f64;
    let haldane = (1.0 - (-2.0 * d).exp()) / 2.0;
    assert!(
        (r - haldane).abs() < 0.015,
        "r = {r:.3}, Haldane({d:.2} M) = {haldane:.3}"
    );
}

fn blank(seed: u64) -> Simulation {
    Simulation::new(NewWorld {
        template: "blank".into(),
        seed,
        gen: None,
        founders: None,
        realism: None,
    })
    .unwrap()
}

/// TRD TR-D1: identical inputs give identical histories; different seeds diverge.
#[test]
fn determinism_same_seed_same_history() {
    let run = |seed| {
        let mut s = blank(seed);
        s.run_steps(24 * 20);
        s.set_mode(Mode::Population).unwrap();
        s.run_years(5.0);
        s.state_hash()
    };
    assert_eq!(run(4), run(4));
    assert_ne!(run(4), run(5));
}

/// Saving and loading reproduces the same future.
#[test]
fn save_load_round_trip_continues_identically() {
    let mut a = blank(8);
    a.run_steps(24 * 10);
    let bytes = a.save_bytes().unwrap();
    let mut b = Simulation::load_bytes(&bytes).unwrap();
    assert_eq!(a.state_hash(), b.state_hash());
    a.run_steps(48);
    b.run_steps(48);
    assert_eq!(a.state_hash(), b.state_hash());
}

/// V-P1 Individual ↔ Population transitions conserve abundance exactly and keep
/// mean genetic values and neutral diversity within tolerance (TRD TR-X4).
#[test]
fn v_p1_mode_transition_conservation() {
    let mut s = blank(21);
    s.run_steps(24 * 30);
    let n_before = s.species_counts();
    let mean_genetic = |s: &Simulation| -> f32 {
        let a = &s.archs[0];
        let v: Vec<f32> = s
            .animals
            .iter()
            .map(|x| a.genetic_values(&s.alleles, &x.genome)[0])
            .collect();
        v.iter().sum::<f32>() / v.len() as f32
    };
    let het_ind = |s: &Simulation| -> f32 {
        s.animals
            .iter()
            .map(|x| s.archs[0].neutral_heterozygosity(&x.genome))
            .sum::<f32>()
            / s.animals.len() as f32
    };
    let g_before = mean_genetic(&s);
    let h_before = het_ind(&s);
    s.set_mode(Mode::Population).unwrap();
    assert_eq!(s.species_counts(), n_before, "I → P changed abundance");
    let a = &s.archs[0];
    let total: f32 = s.demes.iter().map(|d| d.total() as f32).sum();
    let g_demes: f32 = s
        .demes
        .iter()
        .map(|d| d.mean_latent(a, &s.alleles)[0] * d.total() as f32)
        .sum::<f32>()
        / total;
    assert!(
        (g_demes - g_before).abs() < 0.02,
        "mean genetic body mass {g_before:.4} → {g_demes:.4}"
    );
    s.set_mode(Mode::Individual).unwrap();
    assert_eq!(s.species_counts(), n_before, "P → I changed abundance");
    let g_after = mean_genetic(&s);
    assert!(
        (g_after - g_before).abs() < 0.03,
        "after round trip {g_before:.4} → {g_after:.4}"
    );
    let h_after = het_ind(&s);
    assert!(
        (h_after - h_before).abs() / h_before < 0.1,
        "neutral heterozygosity {h_before:.3} → {h_after:.3}"
    );
}

/// Focus region: only part of the world becomes individuals; abundance is conserved.
#[test]
fn focus_region_conserves_abundance() {
    let mut s = blank(31);
    s.run_steps(24 * 30);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(3.0);
    let before = s.population();
    let n = s.focus_region(0, 0, 15, 15).unwrap();
    assert_eq!(s.population(), before);
    assert_eq!(s.animals.len() as u64, n);
    s.set_mode(Mode::Population).unwrap();
    assert_eq!(s.population(), before);
}

/// Population-mode world with a north–south temperature gradient.
fn gradient_world(seed: u64, founders: Vec<FounderGroup>) -> Simulation {
    Simulation::new(NewWorld {
        template: "blank".into(),
        seed,
        gen: Some(WorldGenParams {
            width: 64,
            height: 48,
            shape: LandShape::Flat,
            lat_north: 70.0,
            lat_south: 15.0,
            land_fraction: 1.0,
            ..WorldGenParams::default()
        }),
        founders: Some(founders),
        realism: None,
    })
    .unwrap()
}

fn latitudinal_means(s: &Simulation, species: u32, t: Trait) -> (f32, f32) {
    let pw = s.patches_w();
    let ph = s.patches_h() as u32;
    let (mut n_sum, mut n_w, mut s_sum, mut s_w) = (0.0, 0.0, 0.0, 0.0);
    for d in s.demes.iter().filter(|d| d.species == species) {
        let (m, _) = d.mean_phenotype(&s.archs[d.arch as usize], &s.alleles);
        let row = d.patch / pw as u32;
        let w = d.total() as f32;
        if row < ph / 3 {
            n_sum += m[t as usize] * w;
            n_w += w;
        } else if row >= 2 * ph / 3 {
            s_sum += m[t as usize] * w;
            s_w += w;
        }
    }
    (n_sum / n_w.max(1.0), s_sum / s_w.max(1.0))
}

/// V-B1 / V-B2 Bergmann-type and insulation clines: cold (northern) populations
/// evolve thicker insulation than warm (southern) ones from the same founders.
#[test]
#[ignore]
fn v_b1_cold_adaptation_cline() {
    let mut wins = 0;
    let seeds = 5;
    for seed in 0..seeds {
        let mut s = gradient_world(
            50 + seed,
            vec![FounderGroup {
                species: "hare".into(),
                count: 400,
                diversity: 0.8,
                region: Some((0.5, 0.5, 0.2)),
                delay_years: 0.0,
            }],
        );
        s.set_mode(Mode::Population).unwrap();
        s.run_years(1500.0);
        let (north, south) = latitudinal_means(&s, 0, Trait::Insulation);
        eprintln!("seed {seed}: insulation north {north:.3} south {south:.3}");
        if north > south {
            wins += 1;
        }
    }
    assert!(
        wins >= 4,
        "cold populations were better insulated in only {wins}/{seeds} seeds"
    );
}

/// V-S1 Allopatric speciation: two populations separated by an impassable barrier
/// accumulate incompatibilities and are detected as separate species.
#[test]
#[ignore]
fn v_s1_allopatric_speciation() {
    let mut s = Simulation::new(NewWorld {
        template: "blank".into(),
        seed: 77,
        gen: None,
        founders: Some(vec![FounderGroup {
            species: "mouse".into(),
            count: 400,
            diversity: 0.8,
            region: Some((0.15, 0.5, 0.1)),
            delay_years: 0.0,
        }]),
        realism: None,
    })
    .unwrap();
    // Raise the sea in a vertical band to split the world in two.
    let w = s.world.w;
    for y in 0..s.world.h {
        // Two straits: one mid-map and one at the east–west wrap seam.
        for x in (w / 2 - 2..w / 2 + 2).chain(0..2).chain(w - 2..w) {
            let i = y * w + x;
            s.world.water[i] = evo_engine::world::Water::Ocean;
            s.world.elevation[i] = -100.0;
        }
    }
    s.world.refresh_biomes();
    s.set_mode(Mode::Population).unwrap();
    s.run_years(50.0);
    // Introduce the same founders on the far side too, from the same lineage, by
    // letting the founder deme raft once: copy each western deme to the east.
    let pw = s.patches_w() as u32;
    let east: Vec<_> = s
        .demes
        .iter()
        .filter(|d| d.patch % pw < pw / 2 - 1)
        .map(|d| {
            let mut c = d.clone();
            c.patch = d.patch + pw / 2 + 1;
            c
        })
        .collect();
    s.demes.extend(east);
    s.demes.sort_by_key(|d| (d.species, d.patch));
    // Small isolated populations (the island case) and fast mutation so the test is
    // quick; speciation in large populations takes far longer, as in nature.
    s.params.habitat_m2_per_tile = 12.0;
    s.params.mutation_multiplier = 60.0;
    let mut years = 0;
    while s.species.alive().count() < 2 && years < 20_000 {
        s.run_years(500.0);
        years += 500;
    }
    let alive = s.species.alive().count();
    eprintln!("speciation after ≤ {years} years");
    eprintln!(
        "species alive: {alive}; events: {:?}",
        s.events
            .iter()
            .filter(|e| e.kind == evo_engine::record::EventKind::Speciation)
            .map(|e| &e.text)
            .collect::<Vec<_>>()
    );
    assert!(alive >= 2, "no speciation across the barrier");
}

/// Founders are created with their starting age even at year 0, so Population mode
/// sorts adults into adult age classes instead of treating everyone as a newborn.
#[test]
fn founders_keep_their_age_at_world_start() {
    let s = blank(41);
    assert_eq!(s.minutes, 0);
    let dm = s.params.day_minutes;
    let adults: Vec<_> = s.animals.iter().filter(|a| a.dev_days > 30.0).collect();
    assert!(!adults.is_empty());
    for a in adults {
        assert!((a.age_days(s.minutes, dm) - a.dev_days).abs() < 1.0);
    }
}

/// Animals that leave a focus region merge back into the frozen demes of the patch
/// they ended up in rather than creating a second deme for that (species, patch).
#[test]
fn leaving_focus_region_does_not_duplicate_demes() {
    let mut s = blank(31);
    s.run_steps(24 * 30);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(3.0);
    let before = s.population();
    s.focus_region(0, 0, 15, 15).unwrap();
    s.run_steps(24 * 60);
    s.set_mode(Mode::Population).unwrap();
    let mut keys: Vec<_> = s.demes.iter().map(|d| (d.species, d.patch)).collect();
    let n = keys.len();
    keys.dedup();
    assert_eq!(keys.len(), n, "duplicate (species, patch) demes");
    assert!(s.population() > 0 && before > 0);
}

/// Interventions reach the whole world while a focus region is active.
#[test]
fn plague_reaches_frozen_demes_during_focus() {
    let mut s = blank(32);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(2.0);
    s.focus_region(0, 0, 7, 7).unwrap();
    assert!(!s.frozen.is_empty());
    for id in 0..s.species.list.len() as u32 {
        s.apply(evo_engine::sim::Intervention::Plague {
            species: id,
            mortality: 1.0,
        })
        .unwrap();
    }
    assert_eq!(s.population(), 0);
    assert!(s
        .apply(evo_engine::sim::Intervention::Plague {
            species: 999,
            mortality: 0.5,
        })
        .is_err());
}

/// With two daughters, the oldest split is the root of the clade and every tip is
/// the same age as the parent lineage.
#[test]
fn newick_nests_multiple_daughters_by_age() {
    let mut s = blank(33);
    let mut p = s.species.list[0].clone();
    p.id = 0;
    p.parent = None;
    p.extinct_year = None;
    p.name = "P p".into();
    p.origin_year = s.year() - 300.0;
    let mut a = p.clone();
    a.id = 1;
    a.parent = Some(0);
    a.name = "A a".into();
    a.origin_year = s.year() - 200.0;
    let mut b = a.clone();
    b.id = 2;
    b.name = "B b".into();
    b.origin_year = s.year() - 100.0;
    s.species.list = vec![p, a, b];
    let nwk = s.newick();
    assert!(
        nwk.contains("((P_p:100.0,B_b:100.0):100.0,A_a:200.0):100.0"),
        "{nwk}"
    );
}

/// Custom generator settings (FR-W1) override the template, and nonsense settings are
/// rejected instead of producing a broken world.
#[test]
fn custom_world_settings_apply_and_are_validated() {
    let world = |gen: WorldGenParams| {
        Simulation::new(NewWorld {
            template: "random".into(),
            seed: 4,
            gen: Some(gen),
            founders: Some(vec![]),
            realism: None,
        })
    };
    let gen = WorldGenParams {
        width: 64,
        height: 32,
        land_fraction: 0.7,
        ..WorldGenParams::default()
    };
    let s = world(gen.clone()).unwrap();
    assert_eq!((s.world.w, s.world.h), (64, 32));
    let land = s.world.land_tiles() as f64 / (64.0 * 32.0);
    assert!((0.55..0.85).contains(&land), "land fraction {land}");
    for bad in [
        WorldGenParams {
            lat_north: -10.0,
            lat_south: 10.0,
            ..gen.clone()
        },
        WorldGenParams {
            land_fraction: 1.5,
            ..gen.clone()
        },
        WorldGenParams {
            axial_tilt: f64::NAN,
            ..gen.clone()
        },
    ] {
        assert!(world(bad).is_err());
    }
}

/// A focus region dragged across the east–west seam covers the patches on both sides of
/// it, not the inverted rectangle between them.
#[test]
fn focus_region_wraps_across_the_seam() {
    let mut s = blank(33);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(2.0);
    let ps = s.params.patch_size as u32;
    let (w, h) = (s.world.w as u32, s.world.h as u32);
    let pw = w / ps;
    let expected: u64 = s
        .demes
        .iter()
        .filter(|d| {
            let px = d.patch % pw;
            px == pw - 1 || px == 0
        })
        .map(|d| d.total() as u64)
        .sum();
    assert!(expected > 0, "no animals next to the seam");
    let before = s.population();
    let n = s.focus_region(w - ps, 0, w + ps - 1, h - 1).unwrap();
    assert_eq!(n, expected);
    assert_eq!(s.population(), before);
    let f = s.focus.unwrap();
    assert_eq!((f.0, f.2), (w - ps, w + ps));
}

/// A founder release that finds no habitat leaves no phantom species behind.
#[test]
fn failed_founder_release_creates_no_species() {
    let mut s = blank(34);
    let species = s.species.list.len();
    let archs = s.archs.len();
    let r = s.add_founders(&FounderGroup {
        species: "hare".into(),
        count: 50,
        diversity: 0.8,
        region: Some((0.5, 0.5, 0.0)),
        delay_years: 0.0,
    });
    assert!(r.is_err());
    assert_eq!(s.species.list.len(), species);
    assert_eq!(s.archs.len(), archs);
}

/// Oversized or malformed world settings are rejected instead of panicking.
#[test]
fn bad_world_settings_are_rejected() {
    let make = |f: &dyn Fn(&mut WorldGenParams)| {
        let mut gen = WorldGenParams::default();
        f(&mut gen);
        Simulation::new(NewWorld {
            template: "blank".into(),
            seed: 1,
            gen: Some(gen),
            founders: Some(vec![]),
            realism: None,
        })
    };
    assert!(make(&|g| {
        g.width = 1 << 32;
        g.height = 1 << 32;
    })
    .is_err());
    assert!(make(&|g| g.plates = usize::MAX).is_err());
    assert!(make(&|g| g.mountain_height_m = f64::INFINITY).is_err());
}

/// A damaged save is rejected on load rather than panicking on the first step.
#[test]
fn damaged_save_is_rejected() {
    let mut s = blank(35);
    s.params.tick_minutes = 0;
    assert!(Simulation::load_bytes(&s.save_bytes().unwrap()).is_err());
    let mut s = blank(35);
    s.set_mode(Mode::Population).unwrap();
    s.demes[0].patch = u32::MAX;
    assert!(Simulation::load_bytes(&s.save_bytes().unwrap()).is_err());
}

/// Focus coordinates outside the map are clamped to it.
#[test]
fn focus_region_clamps_to_the_map() {
    let mut s = blank(36);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(1.0);
    s.focus_region(0, 0, u32::MAX, u32::MAX).unwrap();
    let f = s.focus.unwrap();
    assert!(f.0 < f.2 && f.1 < f.3);
    assert!(f.2 <= s.world.w as u32 && f.3 <= s.world.h as u32);
}

/// The hydrology layers the 3D view draws water from: every river runs downhill
/// into a river, lake or the sea, and a lake's surface is above its bed.
#[test]
fn river_and_lake_layers_are_consistent() {
    let s = Simulation::new(NewWorld {
        template: "earth".into(),
        seed: 37,
        gen: None,
        founders: Some(vec![]),
        realism: None,
    })
    .unwrap();
    let water = s.layer("water");
    let flow = s.layer("river_flow");
    let down = s.layer("downstream");
    let level = s.layer("water_level");
    let elevation = s.layer("elevation");
    let mut rivers = 0;
    for i in 0..water.len() {
        if water[i] == 3.0 {
            rivers += 1;
            let d = down[i];
            assert!(d >= 0.0, "river tile {i} has no outlet");
            let d = d as usize;
            assert!(water[d] != 0.0, "river tile {i} drains onto dry land");
            assert!(flow[d] >= flow[i]);
        }
        if water[i] == 2.0 {
            assert!(level[i] > elevation[i]);
        }
    }
    assert!(rivers > 0);
}

/// FR-O1 allele-frequency layer: the listed alleles are polymorphic, the map holds
/// frequencies where the species lives (and -1 elsewhere), and an allele keeps its
/// identity and overall frequency across a switch to Population mode.
#[test]
fn allele_frequency_layer_tracks_alleles_across_modes() {
    let mut s = blank(12);
    s.run_steps(24 * 10);
    let species = s.animals[0].species;
    let list = s.allele_list(species, 40);
    let rows = list.as_array().unwrap();
    assert!(!rows.is_empty(), "no polymorphic alleles listed");
    assert_eq!(rows[0]["kind"], "trait", "trait loci come first");
    for r in rows {
        let p = r["freq"].as_f64().unwrap();
        assert!((0.01..=0.99).contains(&p), "allele frequency {p} listed");
    }
    let uid = rows[0]["uid"].as_u64().unwrap();
    let p0 = rows[0]["freq"].as_f64().unwrap();
    let check_map = |s: &Simulation| {
        let m = s.allele_frequency(species, uid);
        assert!(m.iter().all(|&v| v == -1.0 || (0.0..=1.0).contains(&v)));
        assert!(m.iter().any(|&v| v >= 0.0), "species not on the map");
        for (i, &v) in m.iter().enumerate() {
            if v >= 0.0 {
                assert!(s.world.is_land(i), "frequency on an ocean tile");
            }
        }
    };
    check_map(&s);
    s.set_mode(Mode::Population).unwrap();
    check_map(&s);
    let after = s.allele_list(species, 1000);
    let p1 = after
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["uid"].as_u64() == Some(uid))
        .map(|r| r["freq"].as_f64().unwrap())
        .expect("allele lost in the mode switch");
    assert!((p1 - p0).abs() < 0.05, "allele frequency {p0:.3} → {p1:.3}");
    assert!(s
        .allele_frequency(species, u64::MAX)
        .iter()
        .all(|&v| v == -1.0));
}

/// A run with interventions, mode switches, a focus region and a founder release
/// replays from its log to the same state (PRD FR-P2), also after a save and load
/// mid-run and through the JSON replay script.
#[test]
fn replay_log_reproduces_the_run() {
    use evo_engine::sim::Intervention;
    use evo_engine::{Replay, ReplayScript};
    let mut s = blank(41);
    s.run_steps(24 * 5);
    s.apply(Intervention::ClimateShift { delta_c: -2.0 })
        .unwrap();
    s.run_steps(7);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(3.0);
    s.apply(Intervention::Meteor {
        fx: 0.3,
        fy: 0.5,
        radius: 0.1,
    })
    .unwrap();
    // A refused intervention changes nothing and is not logged.
    assert!(s
        .apply(Intervention::ClimateShift { delta_c: 40.0 })
        .is_err());
    s.run_years(2.0);
    s.focus_region(0, 0, 15, 15).unwrap();
    s.run_steps(30);
    s.apply(Intervention::AddFounders {
        group: FounderGroup {
            species: "hare".into(),
            count: 20,
            diversity: 0.5,
            region: None,
            delay_years: 0.0,
        },
    })
    .unwrap();
    s.run_steps(30);
    // Save and load mid-run: the log travels in the save.
    let mut s = Simulation::load_bytes(&s.save_bytes().unwrap()).unwrap();
    s.set_mode(Mode::Population).unwrap();
    s.run_years(2.0);
    s.apply(Intervention::Plague {
        species: 0,
        mortality: 0.3,
    })
    .unwrap();
    s.run_years(1.0);
    assert_eq!(s.replay.entries.len(), 7);

    let script = s.replay_script().unwrap();
    let json = serde_json::to_string(&script).unwrap();
    let r = Replay::run(serde_json::from_str::<ReplayScript>(&json).unwrap()).unwrap();
    assert!(r.matches());
    assert_eq!(r.sim.state_hash(), s.state_hash());
    assert_eq!(r.sim.minutes, s.minutes);
    assert_eq!(r.sim.replay.entries.len(), s.replay.entries.len());
    assert_eq!(r.sim.events.len(), s.events.len());

    // A tampered log no longer matches.
    let mut bad = script.clone();
    bad.entries.remove(0);
    assert!(!Replay::run(bad).unwrap().matches());
}

/// Saves in the version 1 format, with or without allele uids, load and keep
/// running; they have no replay log.
#[test]
fn version_1_saves_load_and_continue() {
    let mut s = blank(43);
    s.run_steps(24 * 3);
    s.set_mode(Mode::Population).unwrap();
    s.run_years(2.0);
    for legacy in [false, true] {
        let mut old = Simulation::load_bytes(&s.save_bytes_v1(legacy)).unwrap();
        assert_eq!(old.model_version, evo_engine::sim::MODEL_VERSION);
        assert_eq!(old.alleles.len(), s.alleles.len());
        assert!(old.replay.origin.is_none());
        assert!(old.replay_script().is_err());
        if legacy {
            for (i, a) in old.alleles.alleles.iter().enumerate() {
                assert_eq!(a.uid, i as u64);
            }
            assert_eq!(old.alleles.next_uid, old.alleles.len() as u64);
        } else {
            assert_eq!(old.state_hash(), s.state_hash());
        }
        old.run_years(3.0);
        assert!(old.year() > 4.9);
        // Saved again, it uses the current format.
        let again = Simulation::load_bytes(&old.save_bytes().unwrap()).unwrap();
        assert_eq!(again.state_hash(), old.state_hash());
    }
}

/// A real save from the first engine version (before allele uids and
/// `dmi_rate_factor`): `evo run --template blank --seed 3 --years 2` at a56431d.
#[test]
fn earliest_save_format_loads_and_continues() {
    let bytes = include_bytes!("fixtures/v1_pre_uid.evo");
    let mut s = Simulation::load_bytes(bytes).unwrap();
    assert!((s.year() - 2.0).abs() < 1e-9);
    assert_eq!(s.species.list.len(), 1);
    assert_eq!(s.alleles.next_uid, s.alleles.len() as u64);
    let n = s.population();
    assert!(n > 1000);
    s.run_years(2.0);
    assert!(s.population() > 0);
    assert!(Simulation::load_bytes(&s.save_bytes().unwrap()).is_ok());
}
