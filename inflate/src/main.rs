use std::io::Write;

use harness::{Corpus, EncodeImplFn};

fn main() {
    let mut impls: Vec<EncodeImplFn<Vec<u8>, ()>> = Vec::new();

    impls.push((
        "fdeflate".to_string(),
        Box::new(|input| {
            fdeflate::decompress_to_vec(input).unwrap();
        }),
    ));
    impls.push((
        format!("zlib-rs"),
        Box::new(move |input| {
            let mut encoder = flate2::write::ZlibDecoder::new(Vec::new());
            encoder.write_all(&input).unwrap();
            encoder.finish().unwrap();
        }),
    ));

    impls.push((
        format!("miniz_oxide"),
        Box::new(move |input| {
            miniz_oxide::inflate::decompress_to_vec_zlib(input).unwrap();
        }),
    ));

    // impls.push((
    //     format!("libdeflate"),
    //     Box::new(move |uncompressed| {
    //         let mut decompressor = libdeflater::Decompressor::new();
    //         let mut output = vec![0; TODO];
    //     }),
    // ));

    let prepare = Box::new(|input: &[u8]| {
        let uncompressed = fdeflate::decompress_to_vec(&input).unwrap();

        Some((
            uncompressed.len() as f64 * 1e-6,
            input.len(),
            input.to_vec(),
        ))
    });

    let check = Box::new(|_encoded: &(), _original: &Vec<u8>| -> bool { true });

    harness::encode(Corpus::Raw, prepare, impls, check, "MiB/s");
}
