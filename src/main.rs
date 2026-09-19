//! `intpack-bench` — measure integer-sequence codecs on synthetic and
//! corpus-derived streams. See README.md for what is measured and why.

mod bench;
mod codec;
mod extract;
mod report;
mod stream;
mod synth;
mod timer;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::stream::Dataset;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Write the synthetic datasets into a directory.
    Gen {
        #[arg(long, default_value = "data/synthetic")]
        out: PathBuf,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Multiplies universe and list counts; 1 is a quick run.
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// Only presets whose name contains this.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Extract real streams (word/trigram postings, freqs, positions, doc
    /// values) from a directory of files.
    Extract {
        root: PathBuf,
        #[arg(long, default_value = "data/corpus")]
        out: PathBuf,
        /// Approximate ints kept per dataset via deterministic term sampling.
        #[arg(long, default_value_t = 50_000_000)]
        max_ints: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Run every codec over every `.ipb` file in a directory.
    Bench {
        #[arg(long, default_value = "data")]
        data: PathBuf,
        /// Results directory; a JSON file per dataset is written here.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Restrict to these codecs (repeatable).
        #[arg(long = "codec")]
        codecs: Vec<String>,
        /// Only datasets whose name contains this.
        #[arg(long)]
        filter: Option<String>,
        /// Seconds of repetitions per measurement.
        #[arg(long, default_value_t = 2.0)]
        budget: f64,
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
    /// Render a markdown report from a results directory.
    Report {
        results: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// List codecs and what each supports.
    List,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Gen { out, seed, scale, filter } => {
            std::fs::create_dir_all(&out)?;
            for p in synth::presets(scale) {
                if filter.as_deref().is_some_and(|f| !p.name.contains(f)) {
                    continue;
                }
                let ds = synth::generate(p.name, &p.source, seed);
                ds.validate().with_context(|| p.name.to_string())?;
                let path = out.join(format!("{}.ipb", p.name));
                ds.write(&path)?;
                eprintln!("{}: {} lists, {} ints", path.display(), ds.lists.len(), ds.total_ints());
            }
        }
        Cmd::Extract { root, out, max_ints, seed } => {
            std::fs::create_dir_all(&out)?;
            for ds in extract::run(&root, &extract::Options { max_ints, seed })? {
                ds.validate().with_context(|| ds.meta.name.clone())?;
                let path = out.join(format!("{}.ipb", ds.meta.name));
                ds.write(&path)?;
                eprintln!("{}: {} lists, {} ints", path.display(), ds.lists.len(), ds.total_ints());
            }
        }
        Cmd::Bench { data, out, codecs, filter, budget, seed } => {
            let out = out.unwrap_or_else(|| PathBuf::from("results").join(bench::machine().hostname));
            std::fs::create_dir_all(&out)?;
            let cfg = bench::Config { seed, budget: Duration::from_secs_f64(budget), ..Default::default() };
            let codecs: Vec<Box<dyn codec::Codec>> = if codecs.is_empty() {
                codec::all()
            } else {
                codecs
                    .iter()
                    .map(|n| codec::by_name(n).with_context(|| format!("unknown codec {n}")))
                    .collect::<Result<_>>()?
            };
            let machine = bench::machine();
            std::fs::write(out.join("machine.json"), serde_json::to_string_pretty(&machine)?)?;
            let mut files = ipb_files(&data)?;
            files.retain(|p| filter.as_deref().is_none_or(|f| p.to_string_lossy().contains(f)));
            if files.is_empty() {
                bail!("no .ipb files under {}", data.display());
            }
            for path in files {
                let ds = Dataset::read(&path)?;
                ds.validate().with_context(|| path.display().to_string())?;
                eprintln!("== {} ({} lists, {} ints)", ds.meta.name, ds.lists.len(), ds.total_ints());
                let mut records = Vec::new();
                for c in &codecs {
                    let r = bench::run(c.as_ref(), &ds, &cfg);
                    match &r.error {
                        Some(e) => eprintln!("   {:<14} skipped: {e}", r.codec),
                        None => eprintln!(
                            "   {:<14} {:6.2} bits/int  enc {:>6} Mi/s  dec {:>6} Mi/s",
                            r.codec,
                            r.bits_per_int,
                            r.encode.map_or(0.0, |s| r.ints as f64 / s.ns * 1e3).round(),
                            r.decode_arena.map_or(0.0, |s| r.ints as f64 / s.ns * 1e3).round()
                        ),
                    }
                    records.push(r);
                }
                std::fs::write(out.join(format!("{}.json", ds.meta.name)), serde_json::to_string_pretty(&records)?)?;
            }
            let md = report::render(&machine, &load_records(&out)?);
            std::fs::write(out.join("report.md"), &md)?;
            eprintln!("wrote {}", out.join("report.md").display());
        }
        Cmd::Report { results, out } => {
            let machine: bench::Machine =
                serde_json::from_str(&std::fs::read_to_string(results.join("machine.json"))?)?;
            let md = report::render(&machine, &load_records(&results)?);
            match out {
                Some(p) => std::fs::write(p, md)?,
                None => print!("{md}"),
            }
        }
        Cmd::List => {
            for c in codec::all() {
                let caps = c.caps();
                println!(
                    "{:<14} sorted_only={} streaming={} seek={} get={}",
                    c.name(),
                    caps.sorted_only,
                    caps.streaming_encoder,
                    caps.seek,
                    caps.random_access
                );
            }
        }
    }
    Ok(())
}

fn ipb_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "ipb"))
        .map(|e| e.into_path())
        .collect();
    files.sort();
    Ok(files)
}

fn load_records(dir: &Path) -> Result<Vec<bench::Record>> {
    let mut all = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && p.file_name().is_some_and(|n| n != "machine.json"))
        .collect();
    files.sort();
    for f in files {
        let recs: Vec<bench::Record> = serde_json::from_str(&std::fs::read_to_string(&f)?)?;
        all.extend(recs);
    }
    Ok(all)
}
