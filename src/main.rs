#![allow(unused)]

use std::{
    fs,
    hint::black_box,
    io::{Cursor, Read, Write},
    num::NonZeroU64,
    path::{Path, PathBuf},
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
    #[clap(subcommand)]
    mode: Mode,

    #[arg(long, global = true)]
    rust_only: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Speed {
    Fast,
    Default,
    Best,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Filter {
    None,
    Sub,
    Up,
    Average,
    Paeth,
    Adaptive,
}

/// The mode to run the benchmark in
#[derive(clap::Subcommand, Clone, Debug)]
enum Mode {
    /// Measure the performance of encoding
    Encode(CorpusSelection),
    /// Measure the performance of decoding PNGs
    DecodePng(CorpusSelection),
    /// Measure the performance of decoding WebP images
    DecodeWebp(CorpusSelection),
    /// Measure the performance of decoding QOI images
    DecodeQoi(CorpusSelection),
    DecodePngSettings {
        #[arg(value_enum)]
        corpus: Corpus,

        #[clap(short, long, value_enum, default_value_t = Speed::Fast)]
        speed: Speed,
        #[clap(short, long, value_enum, default_value_t = Filter::Adaptive)]
        filter: Filter,
    },
    /// Extract raw
    ExtractRaw,
    GenerateCompressed,
    Deflate,
    Inflate,
}

#[derive(clap::Args, Clone, Debug)]
struct CorpusSelection {
    #[arg(value_enum)]
    corpus: Corpus,
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
    fn get_recursive(path: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for entry in WalkDir::new(path) {
            let entry = entry.unwrap();
            if entry.file_type().is_file() {
                paths.push(entry.path().to_owned());
            }
        }

        paths.shuffle(&mut rand::thread_rng());
        paths
    }
    fn get_corpus(&self) -> Vec<PathBuf> {
        Self::get_recursive(Path::new(match self {
            Corpus::QoiBench => "corpus/qoi_benchmark_suite",
            Corpus::CwebpQoiBench => "corpus/cwebp_qoi_bench",
            Corpus::Raw => "corpus/raw",
        }))
    }
}

fn geometric_mean(v: &[f64]) -> f64 {
    v.iter()
        .fold(1.0, |acc, &x| acc * (x as f64).powf(1.0 / v.len() as f64))
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn main() {
    let args = Args::parse();

    match args.mode {
        Mode::Encode(c) => {
            let corpus = c.corpus.get_corpus();
            println!("Running encoding benchmark with corpus: {:?}", c.corpus);

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

            if !args.rust_only {
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
            }
        }
        Mode::DecodePng(c) => {
            measure_decode(&c.corpus.get_corpus(), ImageFormat::Png, args.rust_only)
        }
        Mode::DecodeWebp(c) => {
            measure_decode(&c.corpus.get_corpus(), ImageFormat::WebP, args.rust_only)
        }
        Mode::DecodeQoi(c) => {
            measure_decode(&c.corpus.get_corpus(), ImageFormat::Qoi, args.rust_only)
        }
        Mode::DecodePngSettings {
            corpus,
            speed,
            filter,
        } => measure_png_decode(&corpus.get_corpus(), args.rust_only, speed, filter),
        Mode::ExtractRaw => extract_raw(),
        Mode::GenerateCompressed => generate_compressed(),
        Mode::Deflate => deflate(args.rust_only),
        Mode::Inflate => inflate(args.rust_only),
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

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        bar.inc(1);
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
    bar.finish_and_clear();

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

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        bar.inc(1);
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
    bar.finish_and_clear();

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

fn measure_png_decode(corpus: &[PathBuf], rust_only: bool, speed: Speed, filter: Filter) {
    let mut total_pixels = Vec::new();
    let mut fdeflate_total_time = Vec::new();
    let mut zune_png_total_time = Vec::new();
    let mut qoi_total_time = Vec::new();

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        bar.inc(1);
        if let Ok(mut bytes) = fs::read(path) {
            let Ok(mut image) = image::load_from_memory(&bytes) else {
                continue;
            };

            let mut reencoded = Vec::new();
            let mut encoder = png::Encoder::new(&mut reencoded, image.width(), image.height());
            if image.color().has_alpha() {
                image = DynamicImage::ImageRgba8(image.to_rgba8());
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
            } else {
                image = DynamicImage::ImageRgb8(image.to_rgb8());
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
            }

            encoder.set_compression(match speed {
                Speed::Fast => png::Compression::Fast,
                Speed::Default => png::Compression::Default,
                Speed::Best => png::Compression::Best,
            });
            encoder.set_filter(match filter {
                Filter::None => png::FilterType::NoFilter,
                Filter::Sub => png::FilterType::Sub,
                Filter::Up => png::FilterType::Up,
                Filter::Average => png::FilterType::Avg,
                Filter::Paeth => png::FilterType::Paeth,
                Filter::Adaptive => png::FilterType::Paeth,
            });
            encoder.set_adaptive_filter(match filter {
                Filter::Adaptive => png::AdaptiveFilterType::Adaptive,
                _ => png::AdaptiveFilterType::NonAdaptive,
            });

            let mut encoder = encoder.write_header().unwrap();
            encoder.write_image_data(image.as_bytes()).unwrap();
            encoder.finish().unwrap();

            total_pixels.push(image.width() as usize * image.height() as usize);

            let start = Instant::now();
            image::load_from_memory(&reencoded).unwrap();
            fdeflate_total_time.push(start.elapsed().as_nanos());

            if !rust_only {
                let start = Instant::now();
                let mut decoder = zune_png::PngDecoder::new(Cursor::new(reencoded));
                decoder.set_options(
                    zune_png::zune_core::options::DecoderOptions::new_fast()
                        .set_max_width(usize::MAX)
                        .set_max_height(usize::MAX),
                );
                decoder.decode().unwrap();
                zune_png_total_time.push(start.elapsed().as_nanos());

                let mut qoi_encoded = Vec::new();
                image
                    .write_to(&mut Cursor::new(&mut qoi_encoded), ImageFormat::Qoi)
                    .unwrap();
                let start = Instant::now();
                image::load_from_memory(&qoi_encoded).unwrap();
                qoi_total_time.push(start.elapsed().as_nanos());
            }
        }
    }
    bar.finish_and_clear();

    let print_entry = |name: &str, bytes: &[usize], time: &[u128]| {
        if time.is_empty() {
            return;
        }

        let speeds: Vec<_> = time
            .iter()
            .zip(total_pixels.iter())
            .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
            .collect();
        println!(
            "{name: <18}{:>6.1} MP/s (average) {:>6.1} MP/s (geomean)",
            mean(&speeds),
            geometric_mean(&speeds),
        );
    };

    print_entry("image-png:", &total_pixels, &fdeflate_total_time);
    print_entry("zune-png:", &total_pixels, &zune_png_total_time);
    print_entry("qoi:", &total_pixels, &qoi_total_time);
}

fn extract_raw() {
    let corpus = Corpus::QoiBench.get_corpus();
    fs::create_dir_all("corpus/raw").unwrap();

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    let mut i = 0;
    for path in corpus {
        bar.inc(1);
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
    bar.finish_and_clear();
}

fn generate_compressed() {
    let corpus = Corpus::Raw.get_corpus();
    fs::create_dir_all("corpus/compressed").unwrap();

    const BACKEND_NAME: &str = "miniz_oxide";

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        bar.inc(1);
        if let Ok(mut bytes) = fs::read(&path) {
            let uncompressed = fdeflate::decompress_to_vec(&bytes).unwrap();

            for j in 1..=9 {
                let output_file = format!(
                    "corpus/compressed/{}.{BACKEND_NAME}.{j}.zlib",
                    path.file_name().unwrap().to_str().unwrap()
                );

                if !fs::exists(&output_file).unwrap() {
                    let mut output_data = Vec::new();
                    let mut encoder = flate2::write::ZlibEncoder::new(
                        &mut output_data,
                        flate2::Compression::new(j as u32),
                    );
                    encoder.write_all(&uncompressed).unwrap();
                    encoder.finish().unwrap();
                    fs::write(output_file, output_data).unwrap();
                }
            }
        }
    }
    bar.finish_and_clear();
}

fn inflate(rust_only: bool) {
    const SUFFIX: &str = "zlib-ng.9.zlib";
    let mut corpus = Corpus::get_recursive(Path::new("corpus/compressed"));
    corpus.retain(|path| path.to_str().unwrap().ends_with(SUFFIX));

    let mut total_bytes = Vec::new();
    let mut fdeflate_total_time = Vec::new();
    let mut flate2_total_time = Vec::new();
    let mut zune_inflate_total_time = Vec::new();

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        bar.inc(1);
        if let Ok(mut bytes) = fs::read(&path) {
            let start = Instant::now();
            let fdeflate_output = fdeflate::decompress_to_vec(&bytes).unwrap();
            fdeflate_total_time.push(start.elapsed().as_nanos());
            total_bytes.push(fdeflate_output.len());

            if !rust_only {
                let start = Instant::now();
                let mut encoder = flate2::read::ZlibDecoder::new(Cursor::new(&bytes));
                encoder.read_to_end(&mut Vec::new()).unwrap();
                flate2_total_time.push(start.elapsed().as_nanos());

                let start = Instant::now();
                let zune_output = zune_inflate::DeflateDecoder::new_with_options(
                    &bytes,
                    zune_inflate::DeflateOptions::default().set_confirm_checksum(true),
                )
                .decode_zlib()
                .unwrap();
                zune_inflate_total_time.push(start.elapsed().as_nanos());

                assert_eq!(fdeflate_output, zune_output);
            }
        }
    }
    bar.finish_and_clear();

    let print_entry = |name: &str, bytes: &[usize], time: &[u128]| {
        if time.is_empty() {
            return;
        }

        // for range in [
        //     0..8 * 1024,
        //     8 * 1024..64 * 1024,
        //     64 * 1024..512 * 1024,
        //     512 * 1024..1024 * 1024 * 1024,
        // ] {
        //     let speeds: Vec<_> = time
        //         .iter()
        //         .zip(total_bytes.iter())
        //         .filter(|(&x, &y)| range.contains(&y))
        //         .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
        //         .collect();

        //     println!(
        //         "{: >8} KB {name: <18}{:>6.1} MiB/s",
        //         range.start / 1024,
        //         geometric_mean(&speeds),
        //     );
        // }

        let speeds: Vec<_> = time
            .iter()
            .zip(total_bytes.iter())
            .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
            .collect();
        println!("{name: <18}{:>6.1} MiB/s", geometric_mean(&speeds),);
    };

    print_entry("fdeflate:", &total_bytes, &fdeflate_total_time);
    print_entry("flate2:", &total_bytes, &flate2_total_time);
    print_entry("zune-inflate:", &total_bytes, &zune_inflate_total_time);
}

fn deflate(rust_only: bool) {
    let corpus = Corpus::Raw.get_corpus();
    fs::create_dir_all("corpus/raw").unwrap();

    let mut total_bytes = Vec::new();
    let mut fdeflate_bytes = Vec::new();
    let mut zopfli_bytes = Vec::new();
    let mut miniz_oxide_bytes = vec![Vec::new(); 10];

    let mut fdeflate_total_time = Vec::new();
    let mut zopfli_total_time = Vec::new();
    let mut miniz_oxide_total_time = vec![Vec::new(); 10];

    // let corpus = &corpus[..10];

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        if let Ok(mut bytes) = fs::read(path) {
            let uncompressed = fdeflate::decompress_to_vec(&bytes).unwrap();

            total_bytes.push(uncompressed.len());

            let start = Instant::now();
            let fdeflate_output = fdeflate::compress_to_vec(&uncompressed);
            fdeflate_bytes.push(fdeflate_output.len()); //fdeflate::compress_to_vec(&uncompressed).len();
            fdeflate_total_time.push(start.elapsed().as_nanos());

            // assert_eq!(
            //     fdeflate::decompress_to_vec(&fdeflate_output).unwrap(),
            //     uncompressed
            // );

            if !rust_only {
                // let start = Instant::now();
                // let mut zopfli_compressed = Vec::new();
                // zopfli::compress(
                //     zopfli::Options {
                //         iteration_count: NonZeroU64::new(1).unwrap(),
                //         ..Default::default()
                //     },
                //     zopfli::Format::Zlib,
                //     &*uncompressed,
                //     &mut zopfli_compressed,
                // )
                // .unwrap();
                // zopfli_bytes.push(zopfli_compressed.len());
                // zopfli_total_time.push(start.elapsed().as_nanos());

                for j in 5..=5 {
                    let start = Instant::now();
                    let mut encoder = flate2::write::ZlibEncoder::new(
                        Vec::new(),
                        flate2::Compression::new(j as u32),
                    );
                    encoder.write_all(&uncompressed).unwrap();
                    miniz_oxide_bytes[j].push(encoder.flush_finish().unwrap().len());
                    miniz_oxide_total_time[j].push(start.elapsed().as_nanos());
                }
            }
        }
        bar.inc(1);
    }
    bar.finish_and_clear();

    let print_entry = |name: &str, bytes: &[usize], time: &[u128]| {
        if time.is_empty() {
            return;
        }

        for range in [
            0..8 * 1024,
            8 * 1024..64 * 1024,
            64 * 1024..512 * 1024,
            512 * 1024..1024 * 1024 * 1024,
        ] {
            let ratios: Vec<_> = bytes
                .iter()
                .zip(total_bytes.iter())
                .filter(|(&x, &y)| range.contains(&y))
                .map(|(&x, &y)| 100.0 * x as f64 / y as f64)
                .collect();
            let speeds: Vec<_> = time
                .iter()
                .zip(total_bytes.iter())
                .filter(|(&x, &y)| range.contains(&y))
                .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
                .collect();

            println!(
                "{: >8}KB {name: <18}{:>6.1} MiB/s    {:02.2}%",
                range.end / 1024,
                geometric_mean(&speeds),
                geometric_mean(&ratios)
            );
        }
        // let ratios: Vec<_> = bytes
        //     .iter()
        //     .zip(total_bytes.iter())
        //     .map(|(&x, &y)| 100.0 * x as f64 / y as f64)
        //     .collect();
        // let speeds: Vec<_> = time
        //     .iter()
        //     .zip(total_bytes.iter())
        //     .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
        //     .collect();

        // println!(
        //     "{name: <18}{:>6.1} MiB/s    {:02.2}%",
        //     geometric_mean(&speeds),
        //     geometric_mean(&ratios)
        // );
    };

    print_entry("fdeflate:", &fdeflate_bytes, &fdeflate_total_time);
    print_entry("zopfli:", &zopfli_bytes, &zopfli_total_time);
    for j in 0..=9 {
        print_entry(
            &format!("miniz_oxide[{}]", j),
            &miniz_oxide_bytes[j],
            &miniz_oxide_total_time[j],
        );
    }
}
