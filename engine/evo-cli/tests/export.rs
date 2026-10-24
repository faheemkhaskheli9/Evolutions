//! The browser app's data export (PRD FR-O11) calls the same engine writers as the CLI,
//! so for one save both must produce byte-identical files.

use evo_engine::{Intervention, NewWorld, Simulation};
use std::path::PathBuf;
use std::process::Command;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("evo-export-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn cli_export_matches_engine_export_for_a_save() {
    let mut sim = Simulation::new(NewWorld {
        template: "blank".into(),
        seed: 3,
        gen: None,
        founders: None,
        realism: None,
    })
    .unwrap();
    sim.run_steps(24 * 30);
    sim.apply(Intervention::ClimateShift { delta_c: -1.5 })
        .unwrap();
    sim.run_steps(24 * 30);
    let dir = temp_dir("match");
    let save = dir.join("run.evo");
    std::fs::write(&save, sim.save_bytes().unwrap()).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_evo"))
        .args(["run", "--quiet", "--years", "0", "--load"])
        .arg(&save)
        .arg("--csv")
        .arg(dir.join("csv"))
        .arg("--chronicle")
        .arg(dir.join("chronicle.csv"))
        .arg("--newick")
        .arg(dir.join("tree.nwk"))
        .arg("--settings")
        .arg(dir.join("settings.json"))
        .status()
        .unwrap();
    assert!(status.success());

    // What the app does: load the same bytes and call the export methods.
    let app = Simulation::load_bytes(&std::fs::read(&save).unwrap()).unwrap();
    let files = app.species_csv_files();
    assert!(!files.is_empty());
    for (name, text) in &files {
        let cli = std::fs::read_to_string(dir.join("csv").join(name)).unwrap();
        assert_eq!(&cli, text, "{name}");
    }
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
    assert_eq!(read("chronicle.csv"), app.chronicle_csv());
    assert_eq!(read("tree.nwk"), app.newick());
    assert_eq!(
        read("settings.json"),
        serde_json::to_string_pretty(&app.settings_json()).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_files_have_the_documented_columns() {
    let mut sim = Simulation::new(NewWorld {
        template: "blank".into(),
        seed: 1,
        gen: None,
        founders: None,
        realism: None,
    })
    .unwrap();
    sim.run_steps(24);
    let (name, csv) = &sim.species_csv_files()[0];
    assert!(name.starts_with("species_0_") && name.ends_with(".csv"));
    let header: Vec<&str> = csv.lines().next().unwrap().split(',').collect();
    let n = evo_engine::traits::Trait::ALL.len();
    assert_eq!(header.len(), 3 + 2 * n);
    assert_eq!(&header[..3], ["year", "n", "het"]);
    assert_eq!(header[3], "body_mass");
    assert_eq!(header[3 + n], "body_mass_sd");
    for row in csv.lines().skip(1) {
        assert_eq!(row.split(',').count(), header.len());
    }

    let chron = sim.chronicle_csv();
    assert!(chron.starts_with("year,kind,species,species_name,text\n"));
    // Event texts contain commas, so they are quoted.
    assert!(chron
        .lines()
        .any(|l| l.contains(",Info,,,\"World 'Blank Slate' created")));
    assert!(chron.lines().any(|l| l.contains(",Founders,0,")));

    let settings = sim.settings_json();
    assert_eq!(settings["format"], "evolutions-settings");
    assert_eq!(settings["template"], "blank");
    assert_eq!(settings["seed"], 1);
    assert!(settings["gen"]["width"].as_u64().unwrap() > 0);
    assert!(!settings["founders"].as_array().unwrap().is_empty());
}
