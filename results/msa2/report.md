# intpack-bench results

Machine: `msa2` — AMD Ryzen 9 9955HX 16-Core Processor — governor `powersave` — cycles counter yes — rustc 1.95.0 (59807616e 2026-04-14) (built from a source tarball)

Units: Mi/s = million ints per second; bits/int is total encoded bytes × 8 / ints; excess = bits/int minus the uniform-model entropy bound (negative means the codec exploited clustering); ns/elem for intersect is per element of the shorter list.

## clustered-d0.01-b512

Sorted, 64 lists, 620111 ints, longest 20217, entropy bound 8.054 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +23.95 | — | 1595 | 9938 | 13804 | 14.6 | 0.2 | — | — | — | 115.4 | 5.1 | 0 | 0 | yes |
| vbyte | 8.05 | -0.01 | — | 482 | 675 | 691 | 14.9 | 2.8 | — | — | — | 6552.9 | — | 0 | 0 | yes |
| bp128 | 3.69 | -4.36 | — | 4620 | 2053 | 2066 | 14.6 | 6.2 | — | — | — | 14842.2 | — | 0 | 0 | yes |
| fastpfor128 | 0.36 | -7.69 | — | 503 | 928 | 965 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.03 | +1.98 | — | 2262 | 2056 | 2048 | — | — | — | — | — | — | — | 84 | 0 | yes |
| roaring | 16.06 | +8.01 | — | 155 | 155 | 179 | 7652.7 | 0.4 | — | — | — | 120.9 | 33.9 | 0 | 0 | yes |
| elias-fano | 10.06 | +2.00 | 1.45 | 52 | 347 | 365 | 2732.5 | 1.6 | — | — | — | 106.3 | 26.2 | 0 | 0 | yes |

## clustered-d0.01-b64

Sorted, 64 lists, 648857 ints, longest 12279, entropy bound 8.052 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +23.95 | — | 1590 | 9811 | 13061 | 14.9 | 1.5 | — | — | — | 151.6 | 5.0 | 0 | 0 | yes |
| vbyte | 8.24 | +0.19 | — | 512 | 562 | 587 | 15.0 | 3.9 | — | — | — | 8302.2 | — | 0 | 0 | yes |
| bp128 | 11.24 | +3.18 | — | 2276 | 2035 | 1977 | 14.9 | 7.2 | — | — | — | 16076.2 | — | 0 | 0 | yes |
| fastpfor128 | 1.21 | -6.84 | — | 224 | 842 | 927 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.23 | +2.18 | — | 2023 | 2070 | 2052 | — | — | — | — | — | — | — | 8 | 0 | yes |
| roaring | 16.11 | +8.06 | — | 150 | 175 | 176 | 8774.7 | 1.5 | — | — | — | 123.7 | 23.9 | 0 | 0 | yes |
| elias-fano | 10.06 | +2.01 | 1.48 | 48 | 313 | 360 | 2887.0 | 3.3 | — | — | — | 93.6 | 29.6 | 0 | 0 | yes |

Noisy (spread > 15% of median): roaring open ±17%

## clustered-d0.01-b8

Sorted, 64 lists, 641186 ints, longest 10834, entropy bound 8.075 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +23.93 | — | 1592 | 9843 | 13162 | 14.9 | 10.1 | — | — | — | 161.3 | 5.1 | 0 | 0 | yes |
| vbyte | 9.43 | +1.35 | — | 258 | 275 | 290 | 14.7 | 10.8 | — | — | — | 16640.4 | — | 0 | 0 | yes |
| bp128 | 11.14 | +3.07 | — | 2650 | 2010 | 1975 | 14.6 | 10.5 | — | — | — | 16178.0 | — | 0 | 0 | yes |
| fastpfor128 | 5.82 | -2.26 | — | 140 | 561 | 619 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 11.04 | +2.97 | — | 1975 | 2075 | 2075 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 16.11 | +8.03 | — | 149 | 174 | 176 | 8531.6 | 10.7 | — | — | — | 128.0 | 24.1 | 0 | 0 | yes |
| elias-fano | 10.11 | +2.03 | 1.50 | 46 | 311 | 351 | 2890.0 | 15.4 | — | — | — | 84.6 | 29.6 | 0 | 0 | yes |

## clustered-d0.1-b64

Sorted, 32 lists, 3228640 ints, longest 106950, entropy bound 4.676 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +27.32 | — | 1582 | 9203 | — | 15.0 | 4.6 | — | — | — | 220.7 | 5.5 | 0 | 0 | yes |
| vbyte | 8.15 | +3.48 | — | 541 | 605 | — | 15.7 | 4.4 | — | — | — | 78851.2 | — | 0 | 416 | yes |
| bp128 | 8.30 | +3.62 | — | 2239 | 2060 | — | 15.3 | 7.9 | — | — | — | 140996.0 | — | 0 | 420 | yes |
| fastpfor128 | 0.94 | -3.73 | — | 228 | 804 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.10 | +5.43 | — | 1866 | 2081 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 10.03 | +5.35 | — | 165 | 153 | — | 7081.8 | 3.2 | — | — | — | 107.6 | 291.9 | 0 | 0 | yes |
| elias-fano | 6.51 | +1.83 | 1.27 | 52 | 329 | — | 18094.7 | 10.5 | — | — | — | 111.9 | 34.2 | 0 | 0 | yes |

## docs.mtimes

Unsorted, 1 lists, 334524 ints, longest 334524, entropy bound 3.651 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +28.35 | — | 1617 | 8126 | — | 50.0 | — | — | — | — | — | 4.4 | 996 | 1304 | yes |
| vbyte | 31.66 | +28.01 | — | 344 | 324 | — | — | — | — | — | — | — | — | 1308 | 1312 | yes |
| bp128 | 24.67 | +21.02 | — | 5040 | 6210 | — | — | — | — | — | — | — | — | 1020 | 1312 | yes |
| fastpfor128 | 24.73 | +21.08 | — | 431 | 1358 | — | — | — | — | — | — | — | — | 1308 | 1300 | yes |
| streamvbyte | 31.60 | +27.94 | — | 1675 | 3561 | — | — | — | — | — | — | — | — | 1392 | 1304 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

Noisy (spread > 15% of median): streamvbyte enc ±18%

## docs.sizes

Unsorted, 1 lists, 334524 ints, longest 334524, entropy bound 13.028 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +18.97 | — | 1621 | 8546 | — | 70.0 | — | — | — | — | — | 4.4 | 992 | 1308 | yes |
| vbyte | 16.64 | +3.61 | — | 456 | 324 | — | — | — | — | — | — | — | — | 0 | 8 | yes |
| bp128 | 16.04 | +3.01 | — | 3469 | 5241 | — | — | — | — | — | — | — | — | 652 | 1312 | yes |
| fastpfor128 | 14.59 | +1.57 | — | 219 | 1193 | — | — | — | — | — | — | — | — | 1304 | 0 | yes |
| streamvbyte | 17.96 | +4.93 | — | 2700 | 3615 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

## geometric-p0.1

Unsorted, 64 lists, 6400000 ints, longest 100000, entropy bound 4.689 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +27.31 | — | 1594 | 8160 | — | 14.4 | — | — | — | — | — | 6.5 | 0 | 0 | yes |
| vbyte | 8.00 | +3.31 | — | 1564 | 621 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 6.21 | +1.52 | — | 4151 | 5580 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 5.51 | +0.82 | — | 314 | 2096 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +5.31 | — | 2601 | 4138 | — | — | — | — | — | — | — | — | 416 | 388 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

## geometric-p0.5

Unsorted, 64 lists, 6400000 ints, longest 100000, entropy bound 1.999 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +30.00 | — | 1593 | 8916 | — | 14.7 | — | — | — | — | — | 6.5 | 76 | 392 | yes |
| vbyte | 8.00 | +6.00 | — | 1647 | 621 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 3.70 | +1.71 | — | 3536 | 5107 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 3.18 | +1.18 | — | 293 | 2163 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +8.00 | — | 2606 | 4145 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

## periodic-s100-j0

Sorted, 64 lists, 640000 ints, longest 10000, entropy bound 8.079 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +23.92 | — | 1587 | 9852 | 12940 | 14.9 | 19.3 | — | — | — | 160.8 | 5.0 | 0 | 0 | yes |
| vbyte | 8.00 | -0.08 | — | 620 | 709 | 707 | 14.9 | 7.7 | — | — | — | 6716.1 | — | 0 | 0 | yes |
| bp128 | 7.06 | -1.01 | — | 3769 | 2006 | 2002 | 14.7 | 8.0 | — | — | — | 13802.6 | — | 0 | 0 | yes |
| fastpfor128 | 7.14 | -0.94 | — | 665 | 868 | 862 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +1.92 | — | 2509 | 2085 | 2075 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 16.11 | +8.03 | — | 149 | 176 | 176 | 8491.0 | 23.0 | — | — | — | 126.9 | 24.5 | 0 | 0 | yes |
| elias-fano | 10.11 | +2.03 | 1.50 | 53 | 358 | 358 | 2884.8 | 56.2 | — | — | — | 93.9 | 28.4 | 0 | 0 | yes |

## periodic-s100-j10

Sorted, 64 lists, 639999 ints, longest 10001, entropy bound 8.079 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +23.92 | — | 1591 | 9732 | 12941 | 14.9 | 29.5 | — | — | — | 161.1 | 5.0 | 0 | 0 | yes |
| vbyte | 8.00 | -0.08 | — | 620 | 709 | 704 | 14.9 | 16.7 | — | — | — | 6724.6 | — | 0 | 0 | yes |
| bp128 | 7.06 | -1.01 | — | 3766 | 2004 | 2000 | 14.5 | 17.1 | — | — | — | 15401.7 | — | 0 | 0 | yes |
| fastpfor128 | 7.14 | -0.94 | — | 662 | 858 | 859 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +1.92 | — | 2083 | 2086 | 2074 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 16.11 | +8.03 | — | 149 | 176 | 155 | 8375.1 | 49.0 | — | — | — | 126.7 | 24.8 | 0 | 0 | yes |
| elias-fano | 10.11 | +2.03 | 1.50 | 53 | 353 | 360 | 2904.7 | 76.7 | — | — | — | 94.1 | 28.1 | 0 | 0 | yes |

## runs-d0.1-r32

Sorted, 32 lists, 3190538 ints, longest 104049, entropy bound 4.694 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +27.31 | — | 1572 | 9295 | — | 15.7 | 4.7 | — | — | — | 221.2 | 5.4 | 408 | 404 | yes |
| vbyte | 8.16 | +3.47 | — | 443 | 601 | — | 15.3 | 4.4 | — | — | — | 78819.5 | — | 0 | 0 | yes |
| bp128 | 9.24 | +4.54 | — | 2238 | 2042 | — | 15.3 | 8.0 | — | — | — | 139732.4 | — | 0 | 0 | yes |
| fastpfor128 | 0.75 | -3.94 | — | 234 | 844 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.10 | +5.41 | — | 1956 | 2082 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 10.13 | +5.44 | — | 163 | 150 | — | 7053.0 | 3.1 | — | — | — | 110.9 | 291.5 | 0 | 0 | yes |
| elias-fano | 6.53 | +1.84 | 1.27 | 51 | 328 | — | 17906.8 | 9.9 | — | — | — | 106.9 | 34.8 | 0 | 0 | yes |

Noisy (spread > 15% of median): vbyte enc ±18%

## runs-d0.5-r256

Sorted, 8 lists, 3971614 ints, longest 506280, entropy bound 2.014 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +29.99 | — | 1601 | 8339 | — | 17.6 | 13.4 | — | — | — | 241.5 | 5.4 | 1668 | 1976 | yes |
| vbyte | 8.02 | +6.01 | — | 607 | 687 | — | 16.2 | 5.5 | — | — | — | 339601.7 | — | 0 | 0 | yes |
| bp128 | 3.22 | +1.21 | — | 3243 | 2075 | — | 17.5 | 8.0 | — | — | — | 659958.5 | — | 0 | 0 | yes |
| fastpfor128 | 0.22 | -1.79 | — | 530 | 974 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.01 | +8.00 | — | 2095 | 1942 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 2.11 | +0.10 | — | 196 | 199 | — | 6052.6 | 8.1 | — | — | — | 82.9 | 274.2 | 0 | 0 | yes |
| elias-fano | 4.21 | +2.19 | 1.20 | 58 | 350 | — | 70775.9 | 29.7 | — | — | — | 117.8 | 33.1 | 0 | 0 | yes |

Noisy (spread > 15% of median): vbyte enc ±24%

## trigrams.docs

Sorted, 31323 lists, 18387319 ints, longest 253461, entropy bound 5.321 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +26.68 | — | 1357 | 5772 | 12027 | 15.2 | 14.9 | 34.2 | 52.3 | 92.6 | 203.9 | 10.6 | 420 | 988 | yes |
| vbyte | 8.60 | +3.27 | — | 422 | 452 | 706 | 16.4 | 13.4 | 28.3 | 143.2 | 1166.1 | 9992.1 | — | 0 | 0 | yes |
| bp128 | 6.56 | +1.24 | — | 1421 | 1409 | 2047 | 16.3 | 14.7 | 42.3 | 296.2 | 2534.6 | 18195.3 | — | 0 | 0 | yes |
| fastpfor128 | 5.44 | +0.12 | — | 206 | 461 | 784 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.42 | +5.10 | — | 1511 | 1829 | 2065 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 9.24 | +3.92 | — | 158 | 143 | 153 | 498.4 | 18.9 | 22.9 | 24.5 | 24.0 | 141.1 | 23.5 | 0 | 420 | yes |
| elias-fano | 9.29 | +3.97 | 2.51 | 46 | 280 | 331 | 460.3 | 36.3 | 64.5 | 65.7 | 78.6 | 167.8 | 40.8 | 0 | 684 | yes |

Noisy (spread > 15% of median): raw-u32 hot ±24%

## uniform-bits-12

Unsorted, 64 lists, 6400000 ints, longest 100000, entropy bound 11.970 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +20.03 | — | 1600 | 8381 | — | 14.9 | — | — | — | — | — | 6.7 | 76 | 392 | yes |
| vbyte | 15.75 | +3.78 | — | 591 | 423 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 12.06 | +0.09 | — | 4795 | 6632 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 12.13 | +0.16 | — | 352 | 2358 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 17.50 | +5.53 | — | 2594 | 4029 | — | — | — | — | — | — | — | — | 416 | 392 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

## uniform-bits-20

Unsorted, 64 lists, 6400000 ints, longest 100000, entropy bound 16.516 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +15.48 | — | 1590 | 8503 | — | 14.9 | — | — | — | — | — | 6.5 | 76 | 392 | yes |
| vbyte | 23.87 | +7.36 | — | 432 | 335 | — | — | — | — | — | — | — | — | 296 | 392 | yes |
| bp128 | 20.06 | +3.55 | — | 4382 | 6274 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 20.13 | +3.61 | — | 318 | 1563 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 25.50 | +8.98 | — | 2223 | 3888 | — | — | — | — | — | — | — | — | 416 | 392 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

## uniform-bits-4

Unsorted, 64 lists, 6400000 ints, longest 100000, entropy bound 4.000 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +28.00 | — | 1599 | 8434 | — | 14.9 | — | — | — | — | — | 6.6 | 76 | 392 | yes |
| vbyte | 8.00 | +4.00 | — | 1648 | 620 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 4.06 | +0.06 | — | 5369 | 6859 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 4.13 | +0.13 | — | 446 | 4438 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +6.00 | — | 2606 | 4136 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

## uniform-d0.0001

Sorted, 64 lists, 6523 ints, longest 125, entropy bound 14.651 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +17.35 | — | 1228 | 9162 | 9019 | 15.0 | 7.3 | — | — | — | — | 4.0 | 4 | 4 | yes |
| vbyte | 17.38 | +2.73 | — | 367 | 204 | 270 | 15.0 | 8.3 | — | — | — | — | — | 4 | 4 | yes |
| bp128 | 17.38 | +2.73 | — | 450 | 398 | 398 | 14.9 | 16.0 | — | — | — | — | — | 4 | 4 | yes |
| fastpfor128 | 17.82 | +3.17 | — | 222 | 224 | 263 | — | — | — | — | — | — | — | 4 | 4 | yes |
| streamvbyte | 17.95 | +3.30 | — | 1580 | 1798 | 1935 | — | — | — | — | — | — | — | 4 | 4 | yes |
| roaring | 26.57 | +11.92 | — | 63 | 80 | 90 | 448.0 | 17.2 | — | — | — | — | 22.7 | 4 | 4 | yes |
| elias-fano | 27.39 | +12.74 | 7.65 | 34 | 209 | 232 | 256.9 | 43.6 | — | — | — | — | 22.1 | 4 | 4 | yes |

## uniform-d0.001

Sorted, 64 lists, 63755 ints, longest 1069, entropy bound 11.406 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +20.59 | — | 1566 | 16153 | 18546 | 14.9 | 19.8 | — | — | — | 92.7 | 4.0 | 4 | 4 | yes |
| vbyte | 15.05 | +3.65 | — | 290 | 295 | 292 | 15.0 | 18.2 | — | — | — | 1097.2 | — | 4 | 4 | yes |
| bp128 | 13.16 | +1.75 | — | 2318 | 1607 | 1777 | 14.4 | 19.0 | — | — | — | 1723.8 | — | 4 | 4 | yes |
| fastpfor128 | 12.68 | +1.27 | — | 296 | 590 | 605 | — | — | — | — | — | — | — | 4 | 4 | yes |
| streamvbyte | 16.22 | +4.82 | — | 2182 | 2007 | 2008 | — | — | — | — | — | — | — | 4 | 4 | yes |
| roaring | 17.09 | +5.69 | — | 107 | 149 | 152 | 1667.2 | 31.4 | — | — | — | 83.9 | 23.3 | 4 | 4 | yes |
| elias-fano | 14.54 | +3.13 | 2.11 | 40 | 292 | 330 | 688.2 | 61.6 | — | — | — | 76.2 | 26.3 | 4 | 4 | yes |

## uniform-d0.01

Sorted, 64 lists, 639614 ints, longest 10253, entropy bound 8.079 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +23.92 | — | 1587 | 9750 | 10427 | 14.6 | 26.3 | — | — | — | 162.1 | 5.0 | 0 | 0 | yes |
| vbyte | 10.21 | +2.14 | — | 201 | 203 | 211 | 15.0 | 22.2 | — | — | — | 21953.4 | — | 0 | 0 | yes |
| bp128 | 9.60 | +1.52 | — | 2428 | 1999 | 1990 | 14.4 | 21.8 | — | — | — | 16157.0 | — | 0 | 0 | yes |
| fastpfor128 | 8.91 | +0.83 | — | 269 | 821 | 891 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.62 | +2.54 | — | 2007 | 2079 | 2074 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 16.11 | +8.03 | — | 150 | 175 | 173 | 8510.2 | 41.4 | — | — | — | 129.7 | 24.1 | 0 | 0 | yes |
| elias-fano | 10.11 | +2.03 | 1.50 | 46 | 309 | 342 | 2881.0 | 62.8 | — | — | — | 98.5 | 28.4 | 0 | 0 | yes |

## uniform-d0.1

Sorted, 32 lists, 3199450 ints, longest 100774, entropy bound 4.690 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +27.31 | — | 1578 | 9265 | — | 15.7 | 31.2 | — | — | — | 223.8 | 5.4 | 392 | 396 | yes |
| vbyte | 8.00 | +3.31 | — | 620 | 707 | — | 15.7 | 21.8 | — | — | — | 67057.0 | — | 0 | 0 | yes |
| bp128 | 6.19 | +1.50 | — | 3060 | 2030 | — | 15.0 | 22.5 | — | — | — | 134366.3 | — | 0 | 0 | yes |
| fastpfor128 | 5.48 | +0.79 | — | 321 | 733 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +5.31 | — | 1964 | 2077 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 10.11 | +5.42 | — | 160 | 124 | — | 7085.8 | 16.6 | — | — | — | 81.9 | 279.2 | 0 | 0 | yes |
| elias-fano | 6.53 | +1.84 | 1.27 | 51 | 333 | — | 17944.4 | 67.9 | — | — | — | 119.2 | 34.2 | 0 | 0 | yes |

## uniform-d0.5

Sorted, 8 lists, 4000553 ints, longest 501276, entropy bound 2.000 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +30.00 | — | 1575 | 7917 | — | 18.8 | 40.2 | — | — | — | 247.9 | 5.5 | 1960 | 768 | yes |
| vbyte | 8.00 | +6.00 | — | 619 | 704 | — | 15.1 | 22.5 | — | — | — | 335013.2 | — | 0 | 0 | yes |
| bp128 | 3.46 | +1.46 | — | 2476 | 2002 | — | 15.1 | 22.7 | — | — | — | 677343.5 | — | 0 | 0 | yes |
| fastpfor128 | 2.74 | +0.74 | — | 262 | 760 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.00 | +8.00 | — | 1858 | 1939 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 2.10 | +0.10 | — | 195 | 193 | — | 6011.4 | 21.7 | — | — | — | 65.7 | 269.3 | 0 | 0 | yes |
| elias-fano | 4.48 | +2.48 | 1.48 | 48 | 327 | — | 84488.1 | 64.5 | — | — | — | 106.4 | 35.3 | 0 | 0 | yes |

## words.docs

Sorted, 1442346 lists, 16641847 ints, longest 245421, entropy bound 9.090 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +22.91 | — | 572 | 916 | 17971 | 39.5 | 5.2 | 45.6 | 70.1 | 90.6 | 208.6 | 16.8 | 0 | 0 | yes |
| vbyte | 11.02 | +1.93 | — | 215 | 230 | 696 | 38.9 | 9.8 | 47.4 | 150.1 | 1119.9 | 7870.4 | — | 0 | 796 | yes |
| bp128 | 10.14 | +1.05 | — | 361 | 356 | 2059 | 39.7 | 10.6 | 59.1 | 303.0 | 2462.7 | 11473.9 | — | 0 | 424 | yes |
| fastpfor128 | 12.83 | +3.74 | — | 40 | 63 | 838 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 14.61 | +5.52 | — | 257 | 396 | 2057 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 24.94 | +15.85 | — | 85 | 82 | 152 | 151.2 | 14.0 | 33.9 | 28.7 | 22.2 | 154.0 | 15.2 | 0 | 0 | yes |
| elias-fano | 113.57 | +104.48 | 60.69 | 19 | 57 | 340 | 293.0 | 29.9 | 93.3 | 85.2 | 71.7 | 166.7 | 41.1 | 0 | 0 | yes |

Noisy (spread > 15% of median): raw-u32 open ±49%, vbyte open ±44%, bp128 open ±47%

## words.freqs

Unsorted, 1442346 lists, 16641847 ints, longest 245421, entropy bound 1.520 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +30.48 | — | 571 | 916 | 18462 | 38.6 | — | — | — | — | — | 16.6 | 0 | 0 | yes |
| vbyte | 8.01 | +6.49 | — | 565 | 444 | 622 | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 5.42 | +3.90 | — | 599 | 630 | 7792 | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 9.02 | +7.50 | — | 41 | 94 | 2077 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 12.13 | +10.61 | — | 263 | 490 | 4092 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

Noisy (spread > 15% of median): raw-u32 open ±48%, vbyte hot ±25%

## words.posdeltas

Unsorted, 1442346 lists, 49933741 ints, longest 4422556, entropy bound 6.880 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +25.12 | — | 887 | 1946 | 18411 | 40.5 | — | — | — | — | — | 16.9 | 17272 | 17272 | yes |
| vbyte | 11.24 | +4.36 | — | 261 | 224 | 215 | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 11.38 | +4.50 | — | 703 | 751 | 7165 | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 11.23 | +4.36 | — | 85 | 143 | 1518 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 13.41 | +6.53 | — | 577 | 981 | 4135 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

Noisy (spread > 15% of median): raw-u32 open ±40%

## zipf-dict-e1-b0

Sorted, 20000 lists, 2096061 ints, longest 200000, entropy bound 10.445 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +21.55 | — | 1428 | 7283 | 15949 | 15.9 | 8.1 | 58.2 | 82.8 | 133.0 | 144.9 | 9.4 | 0 | 0 | yes |
| vbyte | 13.65 | +3.21 | — | 221 | 238 | 694 | 16.2 | 7.3 | 47.8 | 297.6 | 1252.8 | 9696.3 | — | 0 | 0 | yes |
| bp128 | 12.60 | +2.16 | — | 500 | 436 | 2000 | 15.4 | 13.3 | 60.7 | 365.9 | 2935.2 | 9478.8 | — | 0 | 0 | yes |
| fastpfor128 | 12.61 | +2.16 | — | 146 | 199 | 878 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 14.52 | +4.07 | — | 1434 | 1529 | 2081 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 22.16 | +11.72 | — | 75 | 82 | 159 | 675.5 | 21.9 | 39.8 | 46.4 | 42.5 | 117.6 | 53.8 | 0 | 0 | yes |
| elias-fano | 22.99 | +12.55 | 7.67 | 38 | 210 | 336 | 299.7 | 51.1 | 115.1 | 120.3 | 116.4 | 98.3 | 43.3 | 0 | 0 | yes |

## zipf-dict-e1-b64

Sorted, 20000 lists, 2096061 ints, longest 200000, entropy bound 10.445 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +21.55 | — | 1433 | 7368 | 16026 | 14.9 | 1.7 | 6.9 | 23.6 | 12.0 | 132.6 | 9.3 | 0 | 0 | yes |
| vbyte | 10.78 | +0.33 | — | 295 | 338 | 590 | 16.2 | 3.7 | 17.3 | 167.6 | 1176.9 | 5231.7 | — | 0 | 0 | yes |
| bp128 | 12.49 | +2.04 | — | 575 | 501 | 1997 | 15.1 | 8.5 | 36.5 | 316.8 | 2509.4 | 9766.4 | — | 0 | 0 | yes |
| fastpfor128 | 6.89 | -3.56 | — | 139 | 206 | 907 | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 12.31 | +1.86 | — | 1440 | 1500 | 2078 | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | 20.69 | +10.24 | — | 81 | 90 | 141 | 605.3 | 4.6 | 7.4 | 19.3 | 19.8 | 115.8 | 50.9 | 0 | 0 | yes |
| elias-fano | 22.99 | +12.55 | 7.67 | 39 | 206 | 346 | 298.7 | 8.2 | 20.6 | 37.1 | 37.2 | 92.7 | 43.5 | 0 | 0 | yes |

Noisy (spread > 15% of median): vbyte open ±21%

## zipf-values-n1024-e1.2

Unsorted, 64 lists, 6400000 ints, longest 100000, entropy bound 6.108 bits/int

| codec | bits/int | excess | aux | enc Mi/s | dec Mi/s (arena) | dec Mi/s (hot) | open ns/list | ∩ 1:1 ns/e | ∩ 1:10 | ∩ 1:100 | ∩ 1:1000 | seek ns | get ns | enc peak KB | dec peak KB | stream |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|:-:|
| raw-u32 | 32.00 | +25.89 | — | 1597 | 8331 | — | 14.6 | — | — | — | — | — | 6.6 | 0 | 0 | yes |
| vbyte | 9.19 | +3.08 | — | 419 | 303 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| bp128 | 10.06 | +3.95 | — | 4679 | 6122 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| fastpfor128 | 8.63 | +2.52 | — | 174 | 1118 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| streamvbyte | 10.74 | +4.63 | — | 2528 | 4066 | — | — | — | — | — | — | — | — | 0 | 0 | yes |
| roaring | ✗ sorted-only codec |
| elias-fano | ✗ sorted-only codec |

