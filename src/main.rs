#![allow(unused)]

use std::{
    fs,
    hint::black_box,
    io::{Cursor, Read, Write},
    path::PathBuf,
    time::Instant,
};

use byteorder_lite::{BigEndian, LittleEndian, ReadBytesExt};
use clap::{arg, command, Parser, ValueEnum};
use image::{DynamicImage, ImageFormat};
use rand::prelude::*;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Measure the performance of encoding or decoding a given corpus"
)]
struct Args {
    #[arg(value_enum, index = 1)]
    mode: Mode,

    #[arg(value_enum, index = 2)]
    corpus: Corpus,

    #[arg(long)]
    rust_only: bool,
}

/// The mode to run the benchmark in
#[derive(ValueEnum, Clone, Debug)]
enum Mode {
    /// Measure the performance of encoding
    Encode,
    /// Measure the performance of decoding PNGs
    DecodePng,
    /// Measure the performance of decoding WebP images
    DecodeWebp,
    /// Measure the performance of decoding QOI images
    DecodeQoi,
    /// Extract raw
    ExtractRaw,
    /// Compress raw
    Deflate,
}

/// The corpus to choose from
#[derive(ValueEnum, Clone, Debug)]
enum Corpus {
    /// The QOI Benchmark corpus
    QoiBench,
    CwebpQoiBench,
    Raw,
}
impl Corpus {
    fn get_corpus(&self) -> Vec<PathBuf> {
        let directory = match self {
            Corpus::QoiBench => "corpus/qoi_benchmark_suite",
            Corpus::CwebpQoiBench => "corpus/cwebp_qoi_bench",
            Corpus::Raw => "corpus/raw",
        };

        let mut paths = Vec::new();
        for entry in WalkDir::new(directory) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                paths.push(entry.path().to_owned());
            }
        }

        paths.shuffle(&mut rand::thread_rng());
        paths
    }
}

fn main() {
    let args = Args::parse();

    let corpus = args.corpus.get_corpus();

    match args.mode {
        Mode::Encode => {
            println!("Running encoding benchmark with corpus: {:?}", args.corpus);

            let (bandwidth, compression_ratio) = zune_qoi_encode(&corpus);
            println!(
                "zune-qoi:      {:>6.1} MP/s  {:02.2}%",
                bandwidth,
                compression_ratio * 100.0
            );

            let (bandwidth, compression_ratio) = zune_png_encode(&corpus);
            println!(
                "zune-png:      {:>6.1} MP/s  {:02.2}%",
                bandwidth,
                compression_ratio * 100.0
            );

            let (bandwidth, compression_ratio) = mtpng_encode(&corpus);
            println!(
                "mtpng:         {:>6.1} MP/s  {:02.2}%",
                bandwidth,
                compression_ratio * 100.0
            );

            let (bandwidth, compression_ratio) = image_rs_encode(&corpus, ImageFormat::Qoi);
            println!(
                "image-rs QOI:  {:>6.1} MP/s  {:02.2}%",
                bandwidth,
                compression_ratio * 100.0
            );

            let (bandwidth, compression_ratio) = image_rs_encode(&corpus, ImageFormat::Png);
            println!(
                "image-rs PNG:  {:>6.1} MP/s  {:02.2}%",
                bandwidth,
                compression_ratio * 100.0
            );

            let (bandwidth, compression_ratio) = image_rs_encode(&corpus, ImageFormat::WebP);
            println!(
                "image-rs WebP: {:>6.1} MP/s  {:02.2}%",
                bandwidth,
                compression_ratio * 100.0
            );
        }
        Mode::DecodePng => measure_decode(&corpus, ImageFormat::Png, args.rust_only),
        Mode::DecodeWebp => measure_decode(&corpus, ImageFormat::WebP, args.rust_only),
        Mode::DecodeQoi => measure_decode(&corpus, ImageFormat::Qoi, args.rust_only),
        Mode::ExtractRaw => extract_raw(&corpus),
        Mode::Deflate => deflate(&corpus, args.rust_only),
    }
    innumerable::print_counts();
}

fn measure_encode<F: FnMut(&mut Cursor<Vec<u8>>, &DynamicImage)>(
    corpus: &[PathBuf],
    mut f: F,
) -> (f64, f64) {
    let mut total_time = 0;
    let mut total_bytes = 0;
    let mut uncompressed_bytes = 0;
    let mut total_pixels = 0;

    for path in corpus {
        if let Ok(image) = image::open(path) {
            if image.width() > 16383 || image.height() > 16383 {
                continue;
            }

            let image: DynamicImage = if image.color().has_alpha() {
                image.to_rgba8().into()
            } else {
                image.to_rgb8().into()
            };

            let mut buffer = Cursor::new(Vec::new());

            let start = std::time::Instant::now();
            f(&mut buffer, &image);
            let elapsed = start.elapsed();

            total_time += elapsed.as_nanos();
            total_bytes += buffer.get_ref().len() as u64;
            uncompressed_bytes += image.as_bytes().len() as u64;
            total_pixels += image.width() as u64 * image.height() as u64;
        }
    }

    let bandwidth = (total_pixels as f64 / (1 << 20) as f64) / (total_time as f64 * 1e-9);
    let compression_ratio = total_bytes as f64 / uncompressed_bytes as f64;
    (bandwidth, compression_ratio)
}

fn image_rs_encode(corpus: &[PathBuf], format: ImageFormat) -> (f64, f64) {
    measure_encode(corpus, |buffer, image| {
        image.write_to(buffer, format).unwrap();
    })
}

fn mtpng_encode(corpus: &[PathBuf]) -> (f64, f64) {
    measure_encode(corpus, |buffer, image| {
        let mut options = mtpng::encoder::Options::new();
        options
            .set_compression_level(mtpng::CompressionLevel::Fast)
            .unwrap();
        let mut header = mtpng::Header::new();
        header
            .set_size(image.width() as u32, image.height() as u32)
            .unwrap();
        header
            .set_color(
                if image.color().has_alpha() {
                    mtpng::ColorType::TruecolorAlpha
                } else {
                    mtpng::ColorType::Truecolor
                },
                8,
            )
            .unwrap();

        let mut encoder = mtpng::encoder::Encoder::new(buffer, &options);
        encoder.write_header(&header).unwrap();
        encoder.write_image_rows(&image.as_bytes()).unwrap();
        encoder.finish().unwrap();
    })
}

fn zune_png_encode(corpus: &[PathBuf]) -> (f64, f64) {
    measure_encode(corpus, |buffer, image| {
        let mut encoder = zune_png::PngEncoder::new(
            image.as_bytes(),
            zune_png::zune_core::options::EncoderOptions::new(
                image.width() as usize,
                image.height() as usize,
                if image.color().has_alpha() {
                    zune_png::zune_core::colorspace::ColorSpace::RGBA
                } else {
                    zune_png::zune_core::colorspace::ColorSpace::RGB
                },
                zune_png::zune_core::bit_depth::BitDepth::Eight,
            ),
        );
        encoder.encode(buffer).unwrap();
    })
}

fn zune_qoi_encode(corpus: &[PathBuf]) -> (f64, f64) {
    measure_encode(corpus, |buffer, image| {
        let mut encoder = zune_qoi::QoiEncoder::new(
            image.as_bytes(),
            zune_qoi::zune_core::options::EncoderOptions::new(
                image.width() as usize,
                image.height() as usize,
                if image.color().has_alpha() {
                    zune_qoi::zune_core::colorspace::ColorSpace::RGBA
                } else {
                    zune_qoi::zune_core::colorspace::ColorSpace::RGB
                },
                zune_qoi::zune_core::bit_depth::BitDepth::Eight,
            ),
        );
        buffer.write_all(&encoder.encode().unwrap()).unwrap()
    })
}

fn measure_decode(corpus: &[PathBuf], format: ImageFormat, rust_only: bool) {
    let mut image_total_time = 0;
    let mut libwebp_total_time = 0;
    let mut zune_png_total_time = 0;
    let mut zune_qoi_total_time = 0;
    let mut total_pixels = 0;

    for path in corpus {
        if let Ok(mut bytes) = std::fs::read(path) {
            let Ok(original_format) = image::guess_format(&bytes) else {
                continue;
            };

            if original_format != format {
                let Ok(image) = image::load_from_memory(&bytes) else {
                    continue;
                };
                if format == ImageFormat::WebP && (image.width() > 16383 || image.height() > 16383)
                {
                    continue;
                }
                let image: DynamicImage = if image.color().has_alpha() {
                    image.to_rgba8().into()
                } else {
                    image.to_rgb8().into()
                };
                let mut encoded = Vec::new();
                image
                    .write_to(&mut Cursor::new(&mut encoded), format)
                    .unwrap();
                bytes = encoded;
            }

            let start = std::time::Instant::now();
            let Ok(image) = image::load_from_memory(&bytes) else {
                continue;
            };
            image_total_time += start.elapsed().as_nanos();
            total_pixels += image.width() as u64 * image.height() as u64;

            if !rust_only {
                match format {
                    ImageFormat::Png => {
                        let start2 = std::time::Instant::now();
                        let mut decoder = zune_png::PngDecoder::new(Cursor::new(bytes));
                        decoder.set_options(
                            zune_png::zune_core::options::DecoderOptions::new_fast()
                                .set_max_width(usize::MAX)
                                .set_max_height(usize::MAX),
                        );
                        black_box(decoder.decode().unwrap());
                        zune_png_total_time += start2.elapsed().as_nanos();
                    }
                    ImageFormat::WebP => {
                        let start2 = std::time::Instant::now();
                        let decoder = webp::Decoder::new(&bytes);
                        black_box(decoder.decode().unwrap());
                        libwebp_total_time += start2.elapsed().as_nanos();
                    }
                    ImageFormat::Qoi => {
                        let start2 = std::time::Instant::now();
                        let mut decoder = zune_qoi::QoiDecoder::new_with_options(
                            bytes,
                            zune_qoi::zune_core::options::DecoderOptions::new_fast()
                                .set_max_width(usize::MAX)
                                .set_max_height(usize::MAX),
                        );
                        black_box(decoder.decode().unwrap());
                        zune_qoi_total_time += start2.elapsed().as_nanos();
                    }
                    _ => {}
                }
            }
        }
    }
    let scale = (total_pixels as f64 / (1 << 20) as f64) / 1e-9;
    println!(
        "image-rs:      {:>6.1} MP/s",
        scale / (image_total_time as f64)
    );

    if !rust_only {
        match format {
            ImageFormat::Png => {
                println!(
                    "zune-png:      {:>6.1} MP/s",
                    scale / (zune_png_total_time as f64)
                );
            }
            ImageFormat::WebP => {
                println!(
                    "libwebp:       {:>6.1} MP/s",
                    scale / (libwebp_total_time as f64)
                );
            }
            ImageFormat::Qoi => {
                println!(
                    "zune-qoi:      {:>6.1} MP/s",
                    scale / (zune_qoi_total_time as f64)
                );
            }
            _ => {}
        }
    }
}

fn extract_raw(corpus: &[PathBuf]) {
    fs::create_dir_all("corpus/raw").unwrap();

    let mut i = 0;
    for path in corpus {
        if let Ok(mut bytes) = fs::read(path) {
            let Ok(original_format) = image::guess_format(&bytes) else {
                continue;
            };

            let mut image = image::load_from_memory(&bytes).unwrap();

            let mut buffer = Cursor::new(Vec::new());
            image.write_to(&mut buffer, ImageFormat::Png).unwrap();

            buffer.set_position(33);
            let idat_size = buffer.read_u32::<BigEndian>().unwrap();
            let idat_type = buffer.read_u32::<BigEndian>().unwrap();

            assert_eq!(idat_type, u32::from_be_bytes(*b"IDAT"));

            let mut raw = vec![0; idat_size as usize];
            buffer.read_exact(&mut raw).unwrap();

            fs::write(format!("corpus/raw/{i:03}.raw"), raw).unwrap();
            i += 1;
        }
    }
}

fn deflate(corpus: &[PathBuf], rust_only: bool) {
    fs::create_dir_all("corpus/raw").unwrap();

    let mut total_bytes = 0;
    let mut fdeflate_bytes = 0;
    let mut miniz_oxide_bytes = [0; 10];

    let mut fdeflate_total_time = 0;
    let mut miniz_oxide_total_time = [0; 10];

    let mut i = 0;
    for path in corpus {
        if let Ok(mut bytes) = fs::read(path) {
            let uncompressed = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes).unwrap();

            total_bytes += uncompressed.len();

            let start = Instant::now();
            fdeflate_bytes += fdeflate::compress_to_vec(&uncompressed).len();
            fdeflate_total_time += start.elapsed().as_nanos();

            if !rust_only {
                for j in 0..=4 {
                    let start = Instant::now();
                    miniz_oxide_bytes[j] +=
                        miniz_oxide::deflate::compress_to_vec(&uncompressed, j as u8).len();
                    miniz_oxide_total_time[j] += start.elapsed().as_nanos();
                }
            }
        }
    }

    let scale = (total_bytes as f64 / (1 << 30) as f64) / 1e-9;
    println!(
        "fdeflate:         {:>6.3} GiB/s    {:02.2}%",
        scale / (fdeflate_total_time as f64),
        100.0 * fdeflate_bytes as f64 / total_bytes as f64
    );

    if !rust_only {
        for j in 0..=9 {
            println!(
                "miniz_oxide[{j}]:   {:>6.3} GiB/s    {:02.2}%",
                scale / (miniz_oxide_total_time[j] as f64),
                100.0 * miniz_oxide_bytes[j] as f64 / total_bytes as f64
            );
        }
    }
}
