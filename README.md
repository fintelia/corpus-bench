# Corpus Bench

A rust utility to measure the encoding and decoding performance of various image
formats and libraries.

```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph -c "record -F 10000 --call-graph=fp -g" -- decode qoi-bench
```


Generate webp images from png images:
```
find qoi_benchmark_suite -name "*.png" | parallel -eta cwebp -exact -lossless {} -o cwebp_qoi_bench/{#}-{%}.webp
```




## Compression performance notes

```
Encoder            Speed           Ratio
----------         -----------    ------
zlib-rs[1]         289.3 MiB/s    36.06%
zlib-rs[2]         155.7 MiB/s    25.30%

miniz_oxide[1]     301.3 MiB/s    28.35%
miniz_oxide[2]     111.9 MiB/s    25.30%
miniz_oxide[3]      79.6 MiB/s    24.38%

fdeflate:           99.7 MiB/s    24.79%
fdeflate:          159.7 MiB/s    24.63%
fdeflate:          163.9 MiB/s    24.20%
```