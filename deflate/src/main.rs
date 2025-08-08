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

    harness::run(Corpus::Raw, true, impls);
}
