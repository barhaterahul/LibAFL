/*!
 * # `LibAFL` tools
 *
 * Taking care of the `LibAFL` repository since 2024
 */
#![forbid(unexpected_cfgs)]
#![allow(incomplete_features)]
#![warn(clippy::cargo)]
#![allow(ambiguous_glob_reexports)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![allow(
    clippy::unreadable_literal,
    clippy::type_repetition_in_bounds,
    clippy::missing_errors_doc,
    clippy::cast_possible_truncation,
    clippy::used_underscore_binding,
    clippy::ptr_as_ptr,
    clippy::missing_panics_doc,
    clippy::missing_docs_in_private_items,
    clippy::module_name_repetitions,
    clippy::ptr_cast_constness,
    clippy::unsafe_derive_deserialize,
    clippy::similar_names,
    clippy::too_many_lines
)]
#![cfg_attr(not(test), warn(
    missing_debug_implementations,
    missing_docs,
    //trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    //unused_results
))]
#![cfg_attr(test, deny(
    missing_debug_implementations,
    missing_docs,
    //trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_must_use,
    //unused_results
))]
#![cfg_attr(
    test,
    deny(
        bad_style,
        dead_code,
        improper_ctypes,
        non_shorthand_field_patterns,
        no_mangle_generic_items,
        overflowing_literals,
        path_statements,
        patterns_in_fns_without_body,
        unconditional_recursion,
        unused,
        unused_allocation,
        unused_comparisons,
        unused_parens,
        while_true
    )
)]
// Till they fix this buggy lint in clippy
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::borrow_deref_ref)]

use std::{
    fs::read_to_string,
    io,
    io::ErrorKind,
    path::{Path, PathBuf},
    str::from_utf8,
};

use clap::Parser;
use colored::Colorize;
use regex::{Regex, RegexSet};
use tokio::{process::Command, task::JoinSet};
use walkdir::{DirEntry, WalkDir};
use which::which;

const REF_LLVM_VERSION: u32 = 20;

fn is_workspace_toml(path: &Path) -> bool {
    for line in read_to_string(path).unwrap().lines() {
        if line.eq("[workspace]") {
            return true;
        }
    }

    false
}

fn is_binary_crate(crate_path: &Path) -> Result<bool, io::Error> {
    if !crate_path.is_dir() {
        return Err(io::Error::new(
            ErrorKind::NotADirectory,
            "Should be a directory.",
        ));
    }

    let main_path = crate_path.to_path_buf().join("src/main.rs");

    Ok(main_path.is_file())
}

async fn run_cargo_generate_lockfile(cargo_file_path: PathBuf, verbose: bool) -> io::Result<()> {
    // Make sure we parse the correct file
    assert_eq!(
        cargo_file_path.file_name().unwrap().to_str().unwrap(),
        "Cargo.toml"
    );

    let mut cargo_file_dir = cargo_file_path.clone();
    cargo_file_dir.pop();

    if !is_binary_crate(cargo_file_dir.as_path())? {
        if verbose {
            println!(
                "[*] \tSkipping Lockfile for {}...",
                cargo_file_path.as_path().display()
            );
        }
        return Ok(());
    }

    let mut gen_lockfile_cmd = Command::new("cargo");

    gen_lockfile_cmd
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(cargo_file_path.as_path());

    if verbose {
        println!(
            "[*] Generating Lockfile for {}...",
            cargo_file_path.as_path().display()
        );
    }

    let res = gen_lockfile_cmd.output().await?;

    if !res.status.success() {
        let stdout = from_utf8(&res.stdout).unwrap();
        let stderr = from_utf8(&res.stderr).unwrap();
        return Err(io::Error::other(format!(
            "Cargo generate-lockfile failed. Run cargo generate-lockfile for {}.\nstdout: {stdout}\nstderr: {stderr}\ncommand: {gen_lockfile_cmd:?}",
            cargo_file_path.display()
        )));
    }

    Ok(())
}

async fn run_cargo_fmt(cargo_file_path: PathBuf, is_check: bool, verbose: bool) -> io::Result<()> {
    // Make sure we parse the correct file
    assert_eq!(
        cargo_file_path.file_name().unwrap().to_str().unwrap(),
        "Cargo.toml"
    );

    if is_workspace_toml(cargo_file_path.as_path()) {
        println!("[*] Skipping {}...", cargo_file_path.as_path().display());
        return Ok(());
    }

    let task_str = if is_check { "Checking" } else { "Formatting" };

    let mut fmt_command = Command::new("cargo");

    fmt_command
        .arg("fmt")
        .arg("--manifest-path")
        .arg(cargo_file_path.as_path());

    if is_check {
        fmt_command.arg("--check");
    }

    if verbose {
        println!(
            "[*] {} {}...",
            task_str,
            cargo_file_path.as_path().display()
        );
    }

    let res = fmt_command.output().await?;

    if !res.status.success() {
        let stdout = from_utf8(&res.stdout).unwrap();
        let stderr = from_utf8(&res.stderr).unwrap();
        return Err(io::Error::other(format!(
            "Cargo fmt failed. Run cargo fmt for \"{}\".\nstdout: {stdout}\nstderr: {stderr}\ncommand: {fmt_command:?}",
            cargo_file_path.display()
        )));
    }

    Ok(())
}

async fn run_clang_fmt(
    c_file_path: PathBuf,
    clang: String,
    is_check: bool,
    verbose: bool,
) -> io::Result<()> {
    let task_str = if is_check { "Checking" } else { "Formatting" };

    let mut fmt_command = Command::new(&clang);

    fmt_command
        .arg("-i")
        .arg("--style")
        .arg("file")
        .arg(c_file_path.as_path());

    if is_check {
        fmt_command.arg("-Werror").arg("--dry-run");
    }

    fmt_command.arg(c_file_path.as_path());

    if verbose {
        println!("[*] {} {}...", task_str, c_file_path.as_path().display());
    }

    let res = fmt_command.output().await?;

    if res.status.success() {
        Ok(())
    } else {
        let stdout = from_utf8(&res.stdout).unwrap();
        let stderr = from_utf8(&res.stderr).unwrap();
        println!("{stderr}");
        Err(io::Error::other(format!(
            "{clang} failed.\nstdout:{stdout}\nstderr:{stderr}"
        )))
    }
}

/// extracts (major, minor, patch) version from `clang-format --version` output.
#[must_use]
pub fn parse_llvm_fmt_version(fmt_str: &str) -> Option<(u32, u32, u32)> {
    let re =
        Regex::new(r"clang-format version (?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)").unwrap();
    let caps = re.captures(fmt_str)?;

    Some((
        caps["major"].parse().unwrap(),
        caps["minor"].parse().unwrap(),
        caps["patch"].parse().unwrap(),
    ))
}

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    check: bool,
    #[arg(short, long)]
    generate_lockfiles: bool,
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let libafl_root_dir = match project_root::get_project_root() {
        Ok(p) => p,
        Err(_) => std::env::current_dir().expect("Failed to get LibAFL root directory."),
    };

    println!(
        "Using \"{}\" as the project root",
        libafl_root_dir.display()
    );
    let rust_excluded_directories = RegexSet::new([
        r".*target.*",
        r".*utils/noaslr.*",
        r".*docs/listings/baby_fuzzer/listing-.*",
        r".*LibAFL/Cargo.toml.*",
        r".*AFLplusplus.*",
    ])
    .expect("Could not create the regex set from the given regex");

    let c_excluded_directories = RegexSet::new([
        r".*target.*",
        r".*libpng-1\.6.*",
        r".*stb_image\.h$",
        r".*dlmalloc\.c$",
        r".*QEMU-Nyx.*",
        r".*AFLplusplus.*",
        r".*Little-CMS.*",
        r".*cms_transform_fuzzer.cc.*",
        r".*sqlite3.*",
        r".*libfuzzer_libmozjpeg.*",
    ])
    .expect("Could not create the regex set from the given regex");

    let c_file_to_format = RegexSet::new([
        r".*\.cpp$",
        r".*\.hpp$",
        r".*\.cc$",
        r".*\.cxx$",
        r".*\.c$",
        r".*\.h$",
    ])
    .expect("Could not create the regex set from the given regex");

    let rust_projects_to_handle: Vec<PathBuf> = WalkDir::new(&libafl_root_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !rust_excluded_directories.is_match(e.path().as_os_str().to_str().unwrap()))
        .filter(|e| e.file_name() == "Cargo.toml")
        .map(DirEntry::into_path)
        .collect();

    // cargo version
    println!("Using {}", get_version_string("cargo", &[]).await?);

    // rustfmt version
    println!("Using {}", get_version_string("cargo", &["fmt"]).await?);

    let mut tokio_joinset = JoinSet::new();

    if cli.generate_lockfiles {
        for project in rust_projects_to_handle {
            tokio_joinset.spawn(run_cargo_generate_lockfile(project, cli.verbose));
        }
    } else {
        // fallback is for formatting or checking

        let reference_clang_format = format!(
            "clang-format-{}",
            std::env::var("MAIN_LLVM_VERSION")
                .inspect(|e| {
                    println!(
                        "Overriding clang-format version from the default {REF_LLVM_VERSION} to {e} using env variable MAIN_LLVM_VERSION"
                    );
                })
                .unwrap_or(REF_LLVM_VERSION.to_string())
        );
        let unspecified_clang_format = "clang-format";

        let (clang, version, warning) = if which(&reference_clang_format).is_ok() {
            (
                Some(reference_clang_format.as_str()),
                Some(get_version_string(&reference_clang_format, &[]).await?),
                None,
            )
        } else if which(unspecified_clang_format).is_ok() {
            let version_str = get_version_string(unspecified_clang_format, &[]).await?;
            println!("{version_str}");
            let (major, _, _) = parse_llvm_fmt_version(&version_str).unwrap();

            if major == REF_LLVM_VERSION {
                (
                    Some(unspecified_clang_format),
                    Some(version_str.clone()),
                    None,
                )
            } else {
                (
                    Some(unspecified_clang_format),
                    Some(version_str.clone()),
                    Some(format!(
                        "using {version_str}, could provide a different result from {reference_clang_format}"
                    )),
                )
            }
        } else {
            (
                None,
                None,
                Some("clang-format not found. Skipping C formatting...".to_string()),
            )
        };

        if let Some(version) = &version {
            println!("Using {version}");
        }

        let _ = warning.map(print_warning);

        for project in rust_projects_to_handle.clone() {
            tokio_joinset.spawn(run_cargo_fmt(project, cli.check, cli.verbose));
        }

        if let Some(clang) = clang {
            let c_files_to_fmt: Vec<PathBuf> = WalkDir::new(&libafl_root_dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| {
                    !c_excluded_directories.is_match(e.path().as_os_str().to_str().unwrap())
                })
                .filter(|e| e.file_type().is_file())
                .filter(|e| c_file_to_format.is_match(e.file_name().to_str().unwrap()))
                .map(DirEntry::into_path)
                .collect();

            for c_file in c_files_to_fmt {
                tokio_joinset.spawn(run_clang_fmt(
                    c_file,
                    clang.to_string(),
                    cli.check,
                    cli.verbose,
                ));
            }
        }
    }

    while let Some(res) = tokio_joinset.join_next().await {
        match res? {
            Ok(()) => {}
            Err(err) => {
                println!("Error: {err}");
                std::process::exit(exitcode::IOERR)
            }
        }
    }

    if cli.generate_lockfiles {
        println!("[*] Lockfile generation finished successfully.");
    } else if cli.check {
        println!("[*] Check finished successfully.");
    } else {
        println!("[*] Formatting finished successfully.");
    }

    Ok(())
}

async fn get_version_string(path: &str, args: &[&str]) -> Result<String, io::Error> {
    let res = Command::new(path)
        .args(args)
        .arg("--version")
        .output()
        .await?;
    assert!(
        res.status.success(),
        "Failed to run {path} {args:?}: {res:?}"
    );
    Ok(from_utf8(&res.stdout).unwrap().replace('\n', ""))
}

#[expect(clippy::needless_pass_by_value)]
fn print_warning(warning: String) {
    println!("\n{} {}\n", "Warning:".yellow().bold(), warning);
}
