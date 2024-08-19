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