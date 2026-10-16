//! `evo` — headless runner (TRD TR-API3).
//!
//! ```text
//! evo run --template earth --seed 42 --years 500 --mode auto \
//!         --out run.evo --csv metrics/ --newick tree.nwk
//! evo list
//! evo info run.evo
//! ```

use evo_engine::{Mode, NewWorld, Realism, Simulation};
use std::collections::HashMap;
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage:
  evo list                                   list templates and species
  evo run [options]                          run a simulation headless
      --template <key|random>  (default earth)
      --seed <u64>             (default 1)
      --years <f64>            (default 100)
      --mode auto|individual|population   (default auto)
      --individual-years <f64> years to watch individually before switching in auto mode (default 1)
      --strict                 literature mutation rates instead of accelerated
      --load <file.evo>        continue a saved run instead of creating one
      --out <file.evo>         save the final state
      --csv <dir>              write per-species metrics CSV files
      --newick <file>          write the phylogeny
      --quiet                  no progress output
  evo info <file.evo>        summarise a save file"
    );
    std::process::exit(2)
}

fn parse(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let k = args[i].trim_start_matches("--").to_string();
        if i + 1 < args.len() && !args[i + 1].starts_with("--") {
            m.insert(k, args[i + 1].clone());
            i += 2;
        } else {
            m.insert(k, "true".into());
            i += 1;
        }
    }
    m
}

fn summary(sim: &Simulation) {
    let st = sim.status();
    println!(
        "year {:.1} | mode {} | population {} | species alive {} / total {} | alleles {}",
        sim.year(),
        st["mode"].as_str().unwrap_or(""),
        sim.population(),
        st["species_alive"],
        st["species_total"],
        st["alleles"]
    );
    let counts = sim.species_counts();
    for s in &sim.species.list {
        let n = counts.get(&s.id).copied().unwrap_or(0);
        let mean = sim
            .metrics
            .series
            .get(&s.id)
            .and_then(|v| v.last())
            .map(|l| l.mean.clone());
        let status = match s.extinct_year {
            Some(y) => format!("extinct at year {y:.0}"),
            None => format!("n = {n}"),
        };
        print!("  [{}] {} ({}) — {}", s.id, s.common_name, s.name, status);
        if let (Some(m), Some(o)) = (mean, s.origin_mean) {
            if s.extinct_year.is_none() {
                print!(
                    " | mass {:.2} kg (origin {:.2}) | insulation {:.2} ({:.2}) | coat {:.2} ({:.2})",
                    m[0], o[0], m[1], o[1], m[6], o[6]
                );
            }
        }
        println!();
    }
    println!("  deaths: {}", st["deaths"]);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { usage() };
    match cmd.as_str() {
        "list" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&Simulation::catalog()).unwrap()
            );
        }
        "info" => {
            let Some(path) = args.get(1) else { usage() };
            let bytes = std::fs::read(path).unwrap_or_else(|e| {
                eprintln!("cannot read {path}: {e}");
                std::process::exit(1)
            });
            let sim = Simulation::load_bytes(&bytes).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            });
            summary(&sim);
        }
        "run" => {
            let o = parse(&args[1..]);
            let seed: u64 = o.get("seed").and_then(|s| s.parse().ok()).unwrap_or(1);
            let years: f64 = o.get("years").and_then(|s| s.parse().ok()).unwrap_or(100.0);
            let quiet = o.contains_key("quiet");
            let mut sim = if let Some(path) = o.get("load") {
                let bytes = std::fs::read(path).expect("read save");
                Simulation::load_bytes(&bytes).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1)
                })
            } else {
                Simulation::new(NewWorld {
                    template: o.get("template").cloned().unwrap_or_else(|| "earth".into()),
                    seed,
                    gen: None,
                    founders: None,
                    realism: Some(if o.contains_key("strict") {
                        Realism::Strict
                    } else {
                        Realism::Accelerated
                    }),
                })
                .unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1)
                })
            };
            let mode = o.get("mode").map(|s| s.as_str()).unwrap_or("auto");
            let watch: f64 = o
                .get("individual-years")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            let start = Instant::now();
            let end_year = sim.year() + years;
            let mut last_report = sim.year();
            if mode == "population" {
                let _ = sim.set_mode(Mode::Population);
            }
            while sim.year() < end_year {
                if mode == "auto" && sim.mode == Mode::Individual && sim.year() >= watch {
                    let _ = sim.set_mode(Mode::Population);
                }
                sim.step();
                if o.contains_key("debug") && sim.mode == Mode::Population {
                    let mut by: std::collections::BTreeMap<
                        u32,
                        (u32, evo_engine::population::Annual),
                    > = Default::default();
                    for (sp, _, n, r) in &sim.diagnostics {
                        let e = by.entry(*sp).or_insert((0, *r));
                        if *n > e.0 {
                            *e = (*n, *r);
                        }
                    }
                    println!("  totals y{:.0}: {:?}", sim.year(), sim.species_counts());
                    for (sp, (n, r)) in by {
                        println!("  y{:.0} sp{} largest deme n={} hazard {:.2} [starve {:.2} water {:.2} pred {:.2} heat {:.2} cold {:.2}] food {:.2} fec {:.2} juv {:.2}",
                            sim.year(), sp, n, r.hazard, r.starvation, r.dehydration, r.predation, r.heat, r.cold, r.food_ratio, r.fecundity, r.juvenile_survival);
                    }
                }
                if !quiet && sim.year() - last_report >= (years / 10.0).max(1.0) {
                    last_report = sim.year();
                    println!(
                        "… year {:.1}  population {}  species {}  ({:.1}s)",
                        sim.year(),
                        sim.population(),
                        sim.species.alive().count(),
                        start.elapsed().as_secs_f64()
                    );
                }
                if sim.population() == 0 {
                    println!("All life has died out at year {:.1}.", sim.year());
                    break;
                }
            }
            if !quiet {
                println!("\nFinished in {:.1}s\n", start.elapsed().as_secs_f64());
                summary(&sim);
                println!("\nChronicle:");
                for e in &sim.events {
                    println!("  year {:>8.1}  {:?}: {}", e.year, e.kind, e.text);
                }
            }
            if let Some(dir) = o.get("csv") {
                std::fs::create_dir_all(dir).expect("create csv dir");
                for s in &sim.species.list {
                    let mut out = String::from("year,n,het");
                    for t in evo_engine::traits::Trait::ALL {
                        out.push_str(&format!(",{}", t.info().key));
                    }
                    out.push('\n');
                    for smp in sim
                        .metrics
                        .series
                        .get(&s.id)
                        .map(|v| v.as_slice())
                        .unwrap_or(&[])
                    {
                        out.push_str(&format!("{:.4},{},{:.4}", smp.year, smp.n, smp.het));
                        for m in &smp.mean {
                            out.push_str(&format!(",{m:.5}"));
                        }
                        out.push('\n');
                    }
                    let path = format!("{dir}/species_{}_{}.csv", s.id, s.name.replace(' ', "_"));
                    std::fs::write(&path, out).expect("write csv");
                }
            }
            if let Some(path) = o.get("newick") {
                std::fs::write(path, sim.newick()).expect("write newick");
            }
            if let Some(path) = o.get("out") {
                std::fs::write(path, sim.save_bytes().expect("serialize")).expect("write save");
                if !quiet {
                    println!("\nSaved to {path}");
                }
            }
        }
        _ => usage(),
    }
}
