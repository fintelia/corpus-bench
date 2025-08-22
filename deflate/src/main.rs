use std::io::Write;

use harness::{Corpus, RunImplFn};

fn main() {
    let mut impls: Vec<RunImplFn> = Vec::new();

    for level in 0..=9 {
        impls.push((
            format!("fdeflate{level}"),
            Box::new(move |bytes: &[u8]| fdeflate::compress_to_vec_with_level(bytes, level)),
        ));
    }

    for level in 0..=9 {
        impls.push((
            format!("zlib-rs{level}"),
            Box::new(move |uncompressed| {
                let mut encoder = flate2::write::ZlibEncoder::new(
                    Vec::new(),
                    flate2::Compression::new(level as u32),
                );
                encoder.write_all(&uncompressed).unwrap();
                encoder.flush_finish().unwrap()
            }),
        ));
    }

    for level in 0..=9 {
        impls.push((
            format!("miniz_oxide{level}"),
            Box::new(move |uncompressed| {
                miniz_oxide::deflate::compress_to_vec_zlib(uncompressed, level)
            }),
        ));
    }

    for level in 0..=12 {
        impls.push((
            format!("libdeflate{level}"),
            Box::new(move |uncompressed| {
                let mut compressor =
                    libdeflater::Compressor::new(libdeflater::CompressionLvl::new(level).unwrap());
                let mut output = vec![0; compressor.zlib_compress_bound(uncompressed.len())];
                let output_len = compressor.zlib_compress(uncompressed, &mut output).unwrap();
                output.resize(output_len, 0);
                output
            }),
        ));
    }

    impls.push((
        "zopfli".to_string(),
        Box::new(|uncompressed| {
            let mut output = Vec::new();
            zopfli::compress(
                zopfli::Options {
                    // iteration_count: std::num::NonZeroU64::new(1).unwrap(),
                    ..Default::default()
                },
                zopfli::Format::Zlib,
                uncompressed,
                &mut output,
            )
            .unwrap();
            output
        }),
    ));

    harness::run(Corpus::Raw, true, impls);
}
