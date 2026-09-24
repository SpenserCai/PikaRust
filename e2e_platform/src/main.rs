use std::path::{Path, PathBuf};
use std::process;

use pikarust_e2e::config::E2eConfig;
use pikarust_e2e::report;
use pikarust_e2e::runner;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();

    let command = args.get(1).map_or("run", String::as_str);

    match command {
        "run" => {
            if args.get(2).is_some_and(|value| value == "--suite") {
                if args.len() != 4 {
                    usage();
                }
                run_tests(None, &args[3]);
            } else {
                if args.len() > 3 {
                    usage();
                }
                run_tests(args.get(2).map(String::as_str), "all");
            }
        }
        "record-search-baseline" => {
            if args.len() != 3 {
                usage();
            }
            let config = E2eConfig::from_project_root(&find_project_root());
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let checks = pikarust_e2e::checks::preflight::run_preflight(&config, false);
                if checks.iter().any(|check| !check.passed) {
                    return Err("baseline prerequisites failed".into());
                }
                let snapshot = pikarust_e2e::cases::alignment::record_search_baseline(&config)?;
                let path = PathBuf::from(&args[2]);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, serde_json::to_vec_pretty(&snapshot)?)?;
                println!(
                    "Recorded {}; review the diff before replacing the fixture",
                    path.display()
                );
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("{error}");
                process::exit(1);
            }
        }
        "list" => {
            if args.len() != 2 {
                usage();
            }
            list_tests();
        }
        _ => {
            usage();
        }
    }
}

/// Run E2E tests and exit with appropriate code.
fn run_tests(filter: Option<&str>, suite: &str) {
    let project_root = find_project_root();
    let config = E2eConfig::from_project_root(&project_root);

    log::info!("project root: {}", project_root.display());
    log::info!("pikarust bin: {}", config.pikarust_bin.display());
    log::info!("pikafish bin: {}", config.pikafish_bin.display());

    let report = runner::run_all(&config, filter, suite);
    let report_path = std::env::var_os("PIKARUST_E2E_REPORT").map_or_else(
        || project_root.join("target/e2e/report.json"),
        PathBuf::from,
    );
    if let Err(error) = report::write_json(&report, &report_path) {
        eprintln!("Cannot write {}: {error}", report_path.display());
        process::exit(1);
    }
    report::print_report(&report);

    if report::all_passed(&report) {
        process::exit(0);
    } else {
        process::exit(1);
    }
}

/// List all available test cases.
fn list_tests() {
    let cases = pikarust_e2e::cases::all_cases();
    println!("Available E2E test cases:");
    for case in &cases {
        let pikafish = if case.requires_pikafish() {
            " [requires pikafish]"
        } else {
            ""
        };
        let slow = if case.is_slow() { " [slow]" } else { "" };
        println!("  {}{pikafish}{slow}", case.name());
    }
}

/// Find the project root by looking for Cargo.toml with workspace members.
fn find_project_root() -> PathBuf {
    if let Ok(val) = std::env::var("PIKARUST_ROOT") {
        return PathBuf::from(val);
    }

    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // If we're inside e2e_platform/, go up one level
    if dir.ends_with("e2e_platform") {
        dir = dir.parent().map_or_else(|| dir.clone(), Path::to_path_buf);
    }

    // Walk up looking for the workspace Cargo.toml
    let mut candidate = dir.clone();
    for _ in 0..5 {
        let cargo_toml = candidate.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
            if content.contains("[workspace]") {
                return candidate;
            }
        }
        if let Some(parent) = candidate.parent() {
            candidate = parent.to_path_buf();
        } else {
            break;
        }
    }

    dir
}

fn usage() -> ! {
    eprintln!(
        "Usage: pikarust-e2e [run [filter] | run --suite smoke|alignment|all | list | record-search-baseline PATH]"
    );
    process::exit(2)
}
