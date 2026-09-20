package org.apache.lucene.codecs.lucene103;

import java.io.IOException;
import java.io.PrintWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import java.util.TreeSet;
import org.apache.lucene.codecs.CodecUtil;
import org.apache.lucene.codecs.lucene103.Lucene103PostingsFormat.IntBlockTermState;
import org.apache.lucene.document.Document;
import org.apache.lucene.document.Field;
import org.apache.lucene.document.FieldType;
import org.apache.lucene.index.DirectoryReader;
import org.apache.lucene.index.IndexOptions;
import org.apache.lucene.index.IndexWriter;
import org.apache.lucene.index.IndexWriterConfig;
import org.apache.lucene.index.LeafReader;
import org.apache.lucene.index.NoMergePolicy;
import org.apache.lucene.index.Terms;
import org.apache.lucene.index.TermsEnum;
import org.apache.lucene.store.ByteBuffersDataOutput;
import org.apache.lucene.store.Directory;
import org.apache.lucene.store.FSDirectory;
import org.apache.lucene.store.IndexInput;
import org.apache.lucene.store.IOContext;
import org.apache.lucene.util.BytesRef;

/**
 * Emits byte-exact fixtures for the Rust port. One case per line, tab separated:
 *   for   <bpv>  <128 ints comma-sep>  <hex>
 *   fordelta <bpv> <128 deltas>        <hex>       (bpv = ForDeltaUtil.bitsRequired)
 *   pfor  -      <128 ints>            <hex>
 *   docs  -      <doc ids>             <hex>       (Lucene103 docs-only stream, .doc bytes of that term)
 */
public class GenFixtures {
  static String hex(byte[] b) {
    StringBuilder sb = new StringBuilder(b.length * 2);
    for (byte x : b) sb.append(String.format("%02x", x & 0xff));
    return sb.toString();
  }

  static String ints(int[] a, int n) {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < n; i++) {
      if (i > 0) sb.append(',');
      sb.append(Integer.toUnsignedString(a[i]));
    }
    return sb.toString();
  }

  static int[] rnd(Random r, int bpv) {
    int[] a = new int[128];
    long mask = (1L << bpv) - 1;
    for (int i = 0; i < 128; i++) a[i] = (int) (r.nextLong() & mask & 0x7fffffffL);
    if (bpv >= 31) for (int i = 0; i < 128; i++) a[i] = r.nextInt() & 0x7fffffff;
    return a;
  }

  public static void main(String[] args) throws Exception {
    Path outPath = Path.of(args[0]);
    Path indexDir = Path.of(args[1]);
    Random r = new Random(42);
    try (PrintWriter w = new PrintWriter(Files.newBufferedWriter(outPath))) {
      ForUtil forUtil = new ForUtil();
      ForDeltaUtil fd = new ForDeltaUtil();
      PForUtil pf = new PForUtil();
      // ForUtil: random and all-max for every bpv.
      for (int bpv = 1; bpv <= 32; bpv++) {
        for (int variant = 0; variant < 2; variant++) {
          int[] a = rnd(r, bpv);
          if (variant == 1) {
            int max = bpv >= 31 ? 0x7fffffff : (int) ((1L << bpv) - 1);
            for (int i = 0; i < 128; i++) a[i] = (i % 3 == 0) ? max : a[i];
          }
          int[] copy = a.clone();
          ByteBuffersDataOutput o = new ByteBuffersDataOutput();
          forUtil.encode(copy, bpv, o);
          w.println("for\t" + bpv + "\t" + ints(a, 128) + "\t" + hex(o.toArrayCopy()));
        }
      }
      // ForDeltaUtil: deltas >= 1 with bitsRequired = bpv.
      for (int bpv = 1; bpv <= 31; bpv++) {
        int[] a = rnd(r, bpv);
        for (int i = 0; i < 128; i++) if (a[i] == 0) a[i] = 1;
        a[5] = bpv >= 31 ? 0x7fffffff : (int) ((1L << bpv) - 1);
        int need = fd.bitsRequired(a);
        int[] copy = a.clone();
        ByteBuffersDataOutput o = new ByteBuffersDataOutput();
        fd.encodeDeltas(need, copy, o);
        w.println("fordelta\t" + need + "\t" + ints(a, 128) + "\t" + hex(o.toArrayCopy()));
      }
      // PForUtil cases.
      List<int[]> pcases = new ArrayList<>();
      int[] z = new int[128];
      pcases.add(z.clone()); // all zero
      int[] c7 = new int[128]; java.util.Arrays.fill(c7, 7); pcases.add(c7); // all equal small
      int[] c300 = new int[128]; java.util.Arrays.fill(c300, 300); pcases.add(c300); // all equal, > 8 bits
      int[] one = new int[128]; one[17] = 5; pcases.add(one); // all-equal after patching, 1 exception
      int[] one2 = new int[128]; java.util.Arrays.fill(one2, 3); one2[100] = 1000; pcases.add(one2);
      for (int bpv = 1; bpv <= 31; bpv += 3) pcases.add(rnd(r, bpv));
      for (int nex = 1; nex <= 9; nex++) { // exceptions capped at 7
        int[] a = rnd(r, 5);
        for (int k = 0; k < nex; k++) a[r.nextInt(128)] = 1000 + r.nextInt(3000);
        pcases.add(a);
      }
      { int[] a = rnd(r, 4); a[3] = 0x7fffffff; pcases.add(a); } // exception needs > 8 bits of patch
      { int[] a = rnd(r, 20); for (int k = 0; k < 3; k++) a[k * 40] = 0x7fffffff; pcases.add(a); }
      { int[] a = rnd(r, 30); pcases.add(a); }
      for (int[] a : pcases) {
        int[] copy = a.clone();
        ByteBuffersDataOutput o = new ByteBuffersDataOutput();
        pf.encode(copy, o);
        w.println("pfor\t-\t" + ints(a, 128) + "\t" + hex(o.toArrayCopy()));
      }
      // Docs-only postings streams through a real IndexWriter.
      List<int[]> lists = new ArrayList<>();
      lists.add(new int[] {3, 9});
      lists.add(seq(r, 127, 0, 20000));
      lists.add(seq(r, 128, 0, 20000));
      lists.add(seq(r, 129, 0, 20000));
      lists.add(dense(0, 128)); // one dense block, token 0
      lists.add(dense(1000, 300)); // dense block + dense-ish tail
      lists.add(every(2, 500)); // bitset-eligible
      lists.add(every(3, 4096)); // bitset, exactly 32 blocks => level1 header
      lists.add(seq(r, 4096, 0, 400000)); // FOR, exactly one level1 group
      lists.add(seq(r, 4097, 0, 400000));
      lists.add(seq(r, 5000, 0, 600000));
      lists.add(mixed(r));
      lists.add(seq(r, 10000, 0, 1000000)); // > 2 level1 groups
      int maxDoc = 0;
      for (int[] l : lists) maxDoc = Math.max(maxDoc, l[l.length - 1] + 1);
      // term name tNN sorted the same as list order
      List<TreeSet<Integer>> perDoc = new ArrayList<>();
      for (int d = 0; d < maxDoc; d++) perDoc.add(null);
      for (int t = 0; t < lists.size(); t++) {
        for (int d : lists.get(t)) {
          if (perDoc.get(d) == null) perDoc.set(d, new TreeSet<>());
          perDoc.get(d).add(t);
        }
      }
      FieldType ft = new FieldType();
      ft.setIndexOptions(IndexOptions.DOCS);
      ft.setTokenized(false);
      ft.setOmitNorms(true);
      ft.freeze();
      try (Directory dir = FSDirectory.open(indexDir)) {
        IndexWriterConfig cfg = new IndexWriterConfig(null);
        cfg.setCodec(new Lucene103Codec());
        cfg.setMergePolicy(NoMergePolicy.INSTANCE);
        cfg.setMaxBufferedDocs(IndexWriterConfig.DISABLE_AUTO_FLUSH);
        cfg.setRAMBufferSizeMB(2048);
        cfg.setUseCompoundFile(false);
        try (IndexWriter iw = new IndexWriter(dir, cfg)) {
          for (int d = 0; d < maxDoc; d++) {
            Document doc = new Document();
            TreeSet<Integer> ts = perDoc.get(d);
            if (ts != null) for (int t : ts) doc.add(new Field("f", String.format("t%02d", t), ft));
            iw.addDocument(doc);
          }
          iw.commit();
        }
        long[] starts = new long[lists.size()];
        try (DirectoryReader rd = DirectoryReader.open(dir)) {
          if (rd.leaves().size() != 1) throw new IllegalStateException("segments: " + rd.leaves().size());
          LeafReader leaf = rd.leaves().get(0).reader();
          Terms terms = leaf.terms("f");
          TermsEnum te = terms.iterator();
          for (int t = 0; t < lists.size(); t++) {
            if (!te.seekExact(new BytesRef(String.format("t%02d", t)))) throw new IllegalStateException("missing term " + t);
            if (te.docFreq() != lists.get(t).length) throw new IllegalStateException("docFreq");
            IntBlockTermState st = (IntBlockTermState) te.termState();
            starts[t] = st.docStartFP;
          }
        }
        String docFile = null;
        for (String f : dir.listAll()) if (f.endsWith(".doc")) docFile = f;
        try (IndexInput in = dir.openInput(docFile, IOContext.READONCE)) {
          long end = in.length() - CodecUtil.footerLength();
          for (int t = 0; t < lists.size(); t++) {
            long s = starts[t];
            long e = t + 1 < lists.size() ? starts[t + 1] : end;
            byte[] b = new byte[(int) (e - s)];
            in.seek(s);
            in.readBytes(b, 0, b.length);
            w.println("docs\t-\t" + ints(lists.get(t), lists.get(t).length) + "\t" + hex(b));
          }
        }
      }
    }
  }

  static int[] seq(Random r, int n, int lo, int hi) {
    TreeSet<Integer> s = new TreeSet<>();
    while (s.size() < n) s.add(lo + r.nextInt(hi - lo));
    return s.stream().mapToInt(Integer::intValue).toArray();
  }

  static int[] dense(int start, int n) {
    int[] a = new int[n];
    for (int i = 0; i < n; i++) a[i] = start + i;
    return a;
  }

  static int[] every(int k, int n) {
    int[] a = new int[n];
    for (int i = 0; i < n; i++) a[i] = 7 + i * k;
    return a;
  }

  /** Alternates dense, bitset-shaped and sparse blocks so one stream mixes all three tokens. */
  static int[] mixed(Random r) {
    List<Integer> a = new ArrayList<>();
    int d = 0;
    for (int blk = 0; blk < 40; blk++) {
      int mode = blk % 3;
      for (int i = 0; i < 128; i++) {
        if (mode == 0) d += 1;
        else if (mode == 1) d += 1 + r.nextInt(3);
        else d += 1 + r.nextInt(5000);
        a.add(d);
      }
    }
    for (int i = 0; i < 50; i++) { d += 1 + r.nextInt(100); a.add(d); }
    return a.stream().mapToInt(Integer::intValue).toArray();
  }
}
