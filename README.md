# Corpus Bench

A rust utility to measure the encoding and decoding performance of various image
formats and libraries. Still very much WIP and not ready for general use.

## Getting Started

### Configure corpora

1. Download and extract [qoi-benchmark suite](https://qoiformat.org/benchmark/qoi_benchmark_suite.tar) to *corpus/qoi_benchmark_suite*.
2. Populate *corpus/cwebp_qoi_bench* by converting those PNGs to WebP images (optional):
    ```
    cd corpus
    find qoi_benchmark_suite -name "*.png" | parallel -eta cwebp -exact -lossless {} -o cwebp_qoi_bench/{#}-{%}.webp
    ```

### Run benchmarks

Run PNG decoding benchmarks:
```
cargo +nightly run --release -- decode qoi-bench
```
Run WebP decoding benchmarks:
```
cargo +nightly run --release -- decode cwebp-qoi-bench
```
To drill down into the performance of a particular file, in any format:
```
cargo +nightly run --release -- decode-single /path/to/file
```
There are other benchmarks available. For all available options see:
```
cargo +nightly run --release -- --help
```

## Gathering statistics

Corpus bench integrates with the `innumerable` crate to enable easy gathering of
fine grained statistics about the encoding and decoding process. Any calls to
`innumerable::event!` will be aggregated and reported at the end of the run.

The execution times for every single file will also be exported in CSV format.

## Some helpful commands

Generate flamegraph for decoding:
```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo +nightly flamegraph -c "record -F 10000 --call-graph=fp -g" -- decode qoi-bench
```

