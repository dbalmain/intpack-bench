//! Corpus extraction: turn a directory of files into the stream types a
//! content index would actually store, so the benchmark can be re-run on
//! real data by pointing it at a real tree.

use std::path::Path;

use anyhow::Result;

use crate::stream::Dataset;

#[derive(Clone, Debug)]
pub struct Options {
    /// Deterministic term sampling keeps roughly this many ints per dataset
    /// while preserving the list-length distribution.
    pub max_ints: usize,
    pub seed: u64,
}

/// Walk `root` and produce the datasets described in the module doc.
pub fn run(root: &Path, opts: &Options) -> Result<Vec<Dataset>> {
    anyhow::bail!("extract is not implemented yet ({}, max_ints={}, seed={})", root.display(), opts.max_ints, opts.seed)
}
