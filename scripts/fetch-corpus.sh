#!/bin/sh
# Fetch a pinned, public stand-in corpus into data/corpus-src/ so the
# `extract` streams are reproducible across machines until a real tree is
# available. Code (Linux, CPython) plus prose (enwik9 split into pages).
#
# Usage: scripts/fetch-corpus.sh [dest]   (default data/corpus-src)
set -eu
dest="${1:-data/corpus-src}"
mkdir -p "$dest"
cd "$dest"

fetch() {
  # fetch <url> <file>
  [ -f "$2" ] || curl -fL --retry 3 -o "$2" "$1"
}

# Linux 6.12 — C, headers, docs, ~1.4 GB unpacked.
if [ ! -d linux ]; then
  fetch https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.12.tar.xz linux.tar.xz
  mkdir linux && tar -xf linux.tar.xz -C linux --strip-components=1
  rm linux.tar.xz
fi

# CPython 3.13.0 — Python, C, tests, docs.
if [ ! -d cpython ]; then
  fetch https://github.com/python/cpython/archive/refs/tags/v3.13.0.tar.gz cpython.tar.gz
  mkdir cpython && tar -xzf cpython.tar.gz -C cpython --strip-components=1
  rm cpython.tar.gz
fi

# enwik9 — first 10^9 bytes of English Wikipedia XML (Hutter Prize), split
# one page per file so it behaves like a directory of documents.
if [ ! -d enwik9 ]; then
  fetch https://mattmahoney.net/dc/enwik9.zip enwik9.zip
  unzip -q enwik9.zip && rm enwik9.zip
  mkdir enwik9
  awk -v dir=enwik9 '
    /<page>/ {
      n++; d = sprintf("%s/%06d", dir, int(n / 1000)); f = sprintf("%s/%08d.xml", d, n)
      if (!(d in seen)) { seen[d] = 1; system("mkdir -p " d) }
    }
    f != "" { print > f }
    /<\/page>/ { close(f); f = "" }
  ' enwik9
  rm enwik9
fi

du -sh "$PWD"/* 2>/dev/null
