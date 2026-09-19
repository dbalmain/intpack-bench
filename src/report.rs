//! Markdown report from a directory of result JSON files.

use std::fmt::Write as _;

use crate::bench::{Machine, Record};

fn mi_s(s: Option<&crate::timer::Sample>, ints: usize) -> String {
    s.map_or("—".into(), |s| format!("{:.0}", ints as f64 / s.ns * 1e3))
}

fn ns_per(s: Option<&crate::timer::Sample>, n: usize) -> String {
    s.filter(|_| n > 0).map_or("—".into(), |s| format!("{:.1}", s.ns / n as f64))
}

fn opt_f(v: Option<f64>) -> String {
    v.map_or("—".into(), |v| format!("{v:.2}"))
}

pub fn render(machine: &Machine, records: &[Record]) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "# intpack-bench results\n");
    let _ = writeln!(
        md,
        "Machine: `{}` — {} — governor `{}` — cycles counter {} — {}\n",
        machine.hostname,
        machine.cpu,
        machine.governor.as_deref().unwrap_or("?"),
        if machine.cycles_counter { "yes" } else { "no" },
        machine.rustc
    );
    let _ = writeln!(
        md,
        "Units: Mi/s = million ints per second; bits/int is total encoded bytes × 8 / ints; \
         excess = bits/int minus the uniform-model entropy bound (negative means the codec \
         exploited clustering); ns/elem for intersect is per element of the shorter list.\n"
    );

    let mut datasets: Vec<&str> = records.iter().map(|r| r.dataset.as_str()).collect();
    datasets.dedup();
    for ds in datasets {
        let rows: Vec<&Record> = records.iter().filter(|r| r.dataset == ds).collect();
        let Some(first) = rows.iter().find(|r| r.error.is_none()).or(rows.first()) else { continue };
        let _ = writeln!(
            md,
            "## {ds}\n\n{:?}, {} lists, {} ints, longest {}, entropy bound {:.3} bits/int\n",
            first.kind, first.lists, first.ints, first.longest_list, first.entropy_bits_per_int
        );
        let _ = writeln!(
            md,
            "| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |"
        );
        let _ = writeln!(md, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|");
        for r in &rows {
            if let Some(e) = &r.error {
                let _ = writeln!(md, "| {} | ✗ {} |", r.codec, e.replace('|', "/"));
                continue;
            }
            let ix = |ratio: u32| {
                r.intersect
                    .iter()
                    .find(|i| i.ratio == ratio)
                    .map_or("—".into(), |i| format!("{:.1}", i.sample.ns / i.short_ints as f64))
            };
            let _ = writeln!(
                md,
                "| {} | {:.2} | {:+.2} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                r.codec,
                r.bits_per_int,
                r.excess_bits_per_int,
                opt_f(r.aux_bits_per_int),
                mi_s(r.encode.as_ref(), r.ints),
                mi_s(r.decode_arena.as_ref(), r.ints),
                mi_s(r.decode_hot.as_ref(), r.decode_hot_ints),
                ix(1),
                ix(10),
                ix(100),
                ix(1000),
                ns_per(r.seek.as_ref(), r.seek_probes),
                ns_per(r.get.as_ref(), r.get_probes),
                r.encode_peak_kb.map_or("—".into(), |k| k.to_string()),
                r.decode_peak_kb.map_or("—".into(), |k| k.to_string()),
                if r.streaming_encoder { "yes" } else { "no" },
            );
        }
        let _ = writeln!(md);
        let noisy: Vec<String> = rows
            .iter()
            .filter(|r| r.error.is_none())
            .flat_map(|r| {
                [("enc", r.encode), ("dec", r.decode_arena), ("hot", r.decode_hot), ("seek", r.seek), ("get", r.get)]
                    .into_iter()
                    .filter_map(move |(k, s)| {
                        s.filter(|s| s.ns_spread > 0.15)
                            .map(|s| format!("{} {k} ±{:.0}%", r.codec, s.ns_spread * 100.0))
                    })
            })
            .collect();
        if !noisy.is_empty() {
            let _ = writeln!(md, "Noisy (spread > 15% of median): {}\n", noisy.join(", "));
        }
    }
    md
}
