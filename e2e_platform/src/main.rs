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
            let filter = args.get(2).map(String::as_str);
            run_tests(filter);
        }
        "list" => {
            list_tests();
        }
        _ => {
            eprintln!("Usage: pikarust-e2e [run [filter] | list]");
            process::exit(1);
        }
    }
}

/// Run E2E tests and exit with appropriate code.
fn run_tests(filter: Option<&str>) {
    let project_root = find_project_root();
    let config = E2eConfig::from_project_root(&project_root);

    log::info!("project root: {}", project_root.display());
    log::info!("pikarust bin: {}", config.pikarust_bin.display());
    log::info!("pikafish bin: {}", config.pikafish_bin.display());

    let report = runner::run_all(&config, filter);
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
