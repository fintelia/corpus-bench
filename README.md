# Corpus Bench

A rust utility to measure the encoding and decoding performance of various image
formats and libraries. Still very much WIP and not ready for general use.

## Getting Started

### Download corpora

1. Download and extract [qoi-benchmark suite](https://qoiformat.org/benchmark/qoi_benchmark_suite.tar):
    ```
    cargo run --release -p download-and-generate
    ```

2. Populate *corpus/cwebp_qoi_bench* by converting those PNGs to WebP images (optional):
    ```
    cd corpus
    mkdir cwebp_qoi_bench
    find qoi_benchmark_suite -name "*.png" | parallel -eta cwebp -exact -lossless {} -o cwebp_qoi_bench/{#}-{%}.webp
    ```

### Run benchmarks

Run various benchmarks:
```
cargo run --release -p decode-png
cargo run --release -p encode-png
cargo run --release -p deflate
cargo run --release -p inflate
```

Run benchmarks against 10% of the corpus:
```
cargo run --release -p encode-png -- --fast
```

Filter which benchmark to run using a regex:
```
cargo run --release -p encode-png -- --filter "image-png[1-3]"
```

## Gathering statistics

Corpus bench integrates with the `innumerable` crate to enable easy gathering of
fine grained statistics about the encoding and decoding process. Any calls to
`innumerable::event!` will be aggregated and reported at the end of the run.

<!--The execution times for every single file will also be exported in CSV format.-->

## Some helpful commands

Generate flamegraph for decoding:
```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo +nightly flamegraph -c "record -F 10000 --call-graph=fp -g" -p decode-png
```
