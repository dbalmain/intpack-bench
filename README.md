# intpack-bench

A benchmark harness for integer-sequence codecs — the posting-list, position,
frequency and doc-value streams inside a search index. It measures density
against the entropy bound, bulk encode/decode, skip-based intersection, random
access and encoder memory, on synthetic streams with controlled shape and on
streams extracted from any real directory of files.

The point is to get **goal numbers**: what the best existing codecs achieve on
streams like yours, before writing your own.

## Run it

```sh
nix run github:dbalmain/intpack-bench#all
```

That generates the synthetic datasets, runs every codec pinned to one core,
and writes `results/<hostname>/report.md` plus one JSON file per dataset.
`SCALE=4` makes the datasets bigger than L3 (DRAM-resident decode);
`BUDGET=5` spends longer per measurement for less noise; `CORE=n` picks the
pinned core.

Without nix: `cargo run --release -- gen && cargo run --release -- bench`.

To measure your own files instead of synthetic guesses:

```sh
intpack-bench extract ~/src --out data/corpus     # any directory
intpack-bench bench --data data/corpus
```

`extract` assigns document IDs in sorted-path order (as a real index would),
tokenises text files, and emits word postings, byte-trigram postings, term
frequencies, position deltas and two doc-value columns. For a reproducible
stand-in until you have a corpus of your own, `scripts/fetch-corpus.sh` pulls
Linux 6.12, CPython 3.13 and enwik9 split into pages.

## What is measured, and why

Every number is per (dataset, codec). The report has one table per dataset.

| Column | What | Why it matters |
|---|---|---|
| bits/int | encoded bytes × 8 / ints, whole dataset | the density number |
| excess | bits/int minus the entropy bound | separates codec overhead from structure the codec exploited (negative = it found clustering the uniform model can't see) |
| aux | bits/int spent on skip/index structures, where the codec can tell | skip data is a density cost; it must pay for itself in the ∩ columns |
| enc Mi/s | bulk encode, million ints/s | segment write; the re-encode half of a merge |
| dec Mi/s (arena) | decode every list from one arena the size of the encoded dataset | DRAM-resident when the dataset is bigger than L3, cache-resident otherwise — check the size |
| dec Mi/s (hot) | one ~64K-int list decoded repeatedly | the kernel's raw speed with memory taken out |
| open ns/list | preparing a list for queries | zero for in-place formats; deserialisation for owning structures — paid per segment open, not per query |
| ∩ 1:1 … 1:1000 | leapfrog AND of list pairs at that length ratio via the codec's own `next_geq`, ns per element of the shorter list | the real search primitive; where skip structures win or lose |
| seek ns | fresh cursor, one `next_geq` to a random target in a ≥1024-int list | cost of one cold skip |
| get ns | random access by ordinal | doc values, position lookups |
| enc/dec peak KB | peak-RSS delta encoding then decoding the longest list | streaming codecs ≈ 0; partitioned EF and interpolative need the whole list (or a DP table over it) |
| stream | encoder can emit before seeing the whole list | decides whether a writer buffers whole posting lists |

Timing takes the median of repeated runs (≥ 7, up to a time budget) after one
warm-up, records the IQR/median as a noise figure, and counts CPU cycles via
`perf_event_open` where the kernel allows it (default `perf_event_paranoid`
does). The report flags any cell with noise above 15%.

### The entropy bound

For a sorted list of `n` values from a universe of `u`, the uniform-model
bound is `log2(C(u, n))` bits — the cost of naming one `n`-subset when nothing
is known about clustering. Elias-Fano sits within 2 bits/int of it by
construction. For unsorted lists the bound is the zeroth-order empirical
entropy. "Excess" is the codec's bits/int minus this. A raw-u32 control shows
what "no compression" costs against the same bound.

### Stream classes

Two classes with different codecs and different operations:

* **sorted** — strictly increasing (docIDs, positions, trigram postings).
  Delta-coded by the codec. Seek and intersection apply.
* **unsorted** — arbitrary small ints (term frequencies, position deltas,
  doc-value columns). Only bulk and random access apply.

### Datasets

Synthetic (`gen`), sweeping the two parameters that dominate sorted-list
codecs — **density** (`n/u`; Elias-Fano costs ≈ `2 + log2(u/n)`) and
**burstiness** (whether present values cluster) — plus a **Zipfian
dictionary** whose list-length distribution puts most lists under one
128-int block, so the partial-block tail path decides the average bits/int:

* `uniform-d*` — Bernoulli at density *d*; the bound is tight, excess is pure overhead
* `clustered-*` — two-state Markov source: sparse with dense patches
* `periodic-*` — constant stride ± jitter; jitter 0 is the FOR ideal the bound overstates
* `runs-*` — dense runs of consecutive ints; the bitmap/RLE ideal
* `zipf-dict-*` — 20 000 lists with Zipf lengths, uniform or bursty
* `geometric-*`, `uniform-bits-*`, `zipf-values-*` — unsorted payload models

Corpus (`extract <dir>`): `words.docs`, `trigrams.docs` (sorted),
`words.freqs`, `words.posdeltas`, `docs.sizes`, `docs.mtimes` (unsorted).
Files are visited in sorted-path order (docID = position in that order);
VCS, build and package directories are skipped, as are files over 8 MiB or
with a NUL in the first 8 KiB. Words are `[A-Za-z0-9_]+` runs, lowercased,
2–64 bytes; trigrams are overlapping raw-byte windows. Terms are sampled by
hash (`fnv1a(seed, term) % D == 0`) to cap the size at `--max-ints` while
preserving the list-length distribution; lists are never truncated, and the
three `words.*` datasets stay aligned list-for-list.

## Codecs

`intpack-bench list` prints them with capabilities. Reference: `raw-u32`
(control), `vbyte` (delta varint, linear-scan cursor — the baseline a skip
structure must beat by more than it costs). Under test: `bp128`
(SIMD bit-packing, 128-int blocks, varint tail — the Lucene/Tantivy shape),
`bp128-skip` (the same with a skip table), `fastpfor128`, `lucene-pfor`
(Lucene 10.3.1 `PForUtil` 128-blocks + vint tail, with a raw escape outside
Java's signed-positive domain), `lucene-docs` (the docs-only `Lucene103`
posting stream, two skip levels), `streamvbyte`,
`roaring`, `elias-fano`, `pef` (fixed-128 partitioned Elias-Fano with
per-partition all-ones/bitmap/EF selection).

Each adapter is the crate's own on-disk format, fixed costs included, because
that is what you would pay by adopting it. The fixed costs matter on the
Zipfian datasets, where most lists are one to three ints:

| codec | per-list overhead beyond the payload |
|---|---|
| `bp128` | 1 byte num_bits per full 128-block; tail is `vbyte` |
| `bp128-skip` | `bp128` plus 8 bytes of skip table per full 128-block (reported as `aux`) |
| `fastpfor128` | 4-byte block-count word the crate always writes |
| `lucene-pfor` | none (`n` and universe out of band); tail is Lucene vint |
| `lucene-docs` | skip prefix on every 128-block (`vlong` + `vint15` + `vlong15`) and a `vint`+`vlong` per 4096-doc group (reported as `aux`) |
| `streamvbyte` | lists padded to a multiple of 4 (one tag byte per group) |
| `roaring` | 8-byte portable header + 8 bytes per container |
| `elias-fano` | `sucds` select indices, ~130 bytes even for an empty list; the `aux` column reports them |
| `pef` | 2 header bytes, then per 128-partition: an upper-level EF entry, a 2-bit type tag and a payload offset |
| `cpp-*` | 4-byte header word (same as `fastpfor128`); plus ~0.1–0.8 µs per call to construct the C++ codec object, which dominates on short lists |

### C++ competitors

Lemire's C++ FastPFor, via the same `fastpfor` crate, behind `--features cpp`
(off by default; needs cmake and a C++14 compiler). The default build is unchanged.

```sh
cargo build --release --features cpp
nix develop -c cargo build --release --features cpp
```

`packages.cpp` / `nix run .#cpp` is that binary. Wrappers: `cpp-simdfastpfor128`,
`cpp-simdbinarypacking`, `cpp-optpfor`, `cpp-simdpfor`, `cpp-bp32`. C++ codec
objects are not thread-safe; each `encode`/`decode` constructs a fresh one
(~0.1–0.8 µs). Sorted lists use the same first-value-as-is then `gap - 1` as
`fastpfor128`. Whole-array only — no cursor or `get`.

### Adding one

Implement `codec::Codec` in `src/codec/<name>.rs` and add it to `all()` in
`src/codec/mod.rs`. Read the contract at the top of that file: the harness
stores list length and universe out of band, so a codec must not spend bytes
on them; sorted input is delta-coded by the codec; `cursor` and `get` are
optional and must be native, not emulated by decoding everything. The
`conformance` test helper covers the edge cases; every adapter runs it.

## Method notes

* Built with `-C target-cpu=native` (`.cargo/config.toml` and the flake), so
  SIMD codecs get their native width. Results are per-machine by
  construction and `results/` is keyed by hostname.
* The `all` app pins to one core with `taskset`; set the governor to
  `performance` for less noise. Boost is left on because that is how the
  index will actually run.
* Every codec is round-trip checked on a sample of lists, and every
  intersection is checked against a naive merge, before anything is timed.
* Not yet measured: genuinely cold (page-cache-evicted) decode; encode
  under `nice`; multi-threaded anything. All deliberate for a first pass.

## Layout

```
src/stream.rs    dataset container, .ipb file format, entropy bounds
src/synth.rs     synthetic generators and presets
src/extract.rs   corpus → streams
src/codec/       the contract (mod.rs) and one adapter per codec
src/bench.rs     the measurement driver
src/timer.rs     median timing, perf cycles, peak RSS
src/report.rs    markdown from results JSON
results/         committed results, one directory per machine
```

The `.ipb` format is trivial (magic, JSON header, `u32` lists, little-endian)
so a harness in another language can read the same datasets.

## Licence

MIT or Apache-2.0, at your option.
