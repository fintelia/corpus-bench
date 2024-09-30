# Corpus Bench

A rust utility to measure the encoding and decoding performance of various image
formats and libraries. Still very much WIP and not ready for general use.

## Getting Started

### Configure corpuses

1. Download and extract qoi-benchmark suite to *corpus/qoi_benchmark_suite*.
2. Populate *corpus/cwebp_qoi_bench* by converting those PNGs to WebP images (optional):
    ```
    cd corpus
    find qoi_benchmark_suite -name "*.png" | parallel -eta cwebp -exact -lossless {} -o cwebp_qoi_bench/{#}-{%}.webp
    ```

### Run benchmarks

Running with `--help` will show all available options.

[TODO: Explain how to run the benchmarks here]


## Gathering statistics

Corpus bench integrates with the `innumerable` crate to enable easy gathering of
fine grained statistics about the encoding and decoding process. Any calls to
`innumerable::event!` will be aggregated and reported at the end of the run.

## Some helpful commands

Generate flamegraph for decoding:
```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo flamegraph -c "record -F 10000 --call-graph=fp -g" -- decode qoi-bench
```

