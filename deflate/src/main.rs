use std::io::Write;

use harness::{Corpus, EncodeImplFn};

fn main() {
    let mut impls: Vec<EncodeImplFn<Vec<u8>, Vec<u8>>> = Vec::new();

    impls.push((
        "fdeflate-ultra".to_string(),
        Box::new(|bytes| fdeflate::compress_to_vec(bytes)),
    ));
    for level in 0..=9 {
        impls.push((
            format!("zlib-rs{level}"),
            Box::new(move |uncompressed| {
                let mut encoder = flate2::write::ZlibEncoder::new(
                    Vec::new(),
                    flate2::Compression::new(level as u32),
                );
                encoder.write_all(&uncompressed).unwrap();
                encoder.finish().unwrap()
            }),
        ));
    }

    for level in 0..=10 {
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

    // impls.push((
    //     "zopfli".to_string(),
    //     Box::new(|uncompressed| {
    //         let mut output = Vec::new();
    //         zopfli::compress(
    //             zopfli::Options {
    //                 // iteration_count: std::num::NonZeroU64::new(1).unwrap(),
    //                 ..Default::default()
    //             },
    //             zopfli::Format::Zlib,
    //             uncompressed,
    //             &mut output,
    //         )
    //         .unwrap();
    //         output
    //     }),
    // ));

    let prepare = Box::new(|input: &[u8]| {
        let uncompressed = fdeflate::decompress_to_vec(&input).unwrap();

        Some((
            uncompressed.len() as f64 * 1e-6,
            uncompressed.len(),
            uncompressed,
        ))
    });

    let check = Box::new(|encoded: &Vec<u8>, original: &Vec<u8>| -> bool {
        let decompressed = fdeflate::decompress_to_vec(&encoded).unwrap();
        &decompressed == original
    });

    harness::encode(Corpus::Raw, prepare, impls, check, "MiB/s");
}
