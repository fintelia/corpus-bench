#![allow(unused)]

use std::{
    collections::{BTreeMap, HashMap},
    ffi::{c_int, c_void},
    fs,
    hash::Hash,
    hint::black_box,
    io::{Cursor, Read, Write},
    num::NonZeroU64,
    path::{Path, PathBuf},
    time::Instant,
};

use byteorder_lite::{BigEndian, LittleEndian, ReadBytesExt};
use clap::{arg, command, Parser, ValueEnum};
use image::{ColorType, DynamicImage, ImageFormat};
use libc::{abs, c_char};
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

    /// Only benchmark image-rs libraries.
    #[arg(long, global = true)]
    image_rs_only: bool,
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

#[derive(clap::Args, Clone, Debug)]
struct DecodeSettings {
    #[arg(value_enum)]
    corpus: Corpus,

    #[clap(long, default_value = "false")]
    reencode: bool,

    #[clap(long, value_enum, default_value_t = Speed::Fast)]
    png_speed: Speed,
    #[clap(long, value_enum, default_value_t = Filter::Adaptive)]
    png_filter: Filter,
}

/// The mode to run the benchmark in
#[derive(clap::Subcommand, Clone, Debug)]
enum Mode {
    /// Measure the performance of encoding
    Encode(CorpusSelection),

    /// Measure the performance of decoding
    Decode(DecodeSettings),

    DecodeSingle {
        /// The path to the file to decode
        path: PathBuf,
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

            if !args.image_rs_only {
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

                // mtpng v0.4.1 breaks other benchmarks by forcing plain old zlib,
                // so we cannot configure other crates with zlib-ng while it's present.
                // TODO: re-enable.
                //
                // let (bandwidth, compression_ratio) = mtpng_encode(&corpus);
                // println!(
                //     "mtpng:         {:>6.1} MP/s  {:02.2}%",
                //     bandwidth,
                //     compression_ratio * 100.0
                // );

                let (bandwidth, compression_ratio) = image_rs_encode(&corpus, ImageFormat::Qoi);
                println!(
                    "image-rs QOI:  {:>6.1} MP/s  {:02.2}%",
                    bandwidth,
                    compression_ratio * 100.0
                );
            }
        }
        Mode::Decode(decode_settings) => {
            let corpus = decode_settings.corpus.get_corpus();
            println!(
                "Running decoding benchmark with corpus: {:?}",
                decode_settings.corpus
            );

            measure_decode(&corpus, args.image_rs_only, decode_settings);
        }
        Mode::DecodeSingle { path } => {
            println!(
                "Running decoding benchmark with single file: {}",
                path.display()
            );
            measure_decode(
                &[path],
                args.image_rs_only,
                DecodeSettings {
                    corpus: Corpus::QoiBench,
                    reencode: false,
                    png_speed: Speed::Fast,
                    png_filter: Filter::Adaptive,
                },
            );
        }
        Mode::ExtractRaw => extract_raw(),
        Mode::GenerateCompressed => generate_compressed(),
        Mode::Deflate => deflate(args.image_rs_only),
        Mode::Inflate => inflate(args.image_rs_only),
    }
    innumerable::print_counts();
}

extern "C" {
    fn libpng_decode(
        data: *const u8,
        data_len: c_int,
        width: *mut c_int,
        height: *mut c_int,
    ) -> *mut c_void;

    fn stbi_load_from_memory(
        data: *const u8,
        data_len: c_int,
        width: *mut c_int,
        height: *mut c_int,
        channels: *mut c_int,
        desired_channels: c_int,
    ) -> *mut u8;

    fn wuffs_load_from_memory(
        data: *const u8,
        data_len: c_int,
        width: *mut c_int,
        height: *mut c_int,
        channels: *mut c_int,
        desired_channels: c_int,
    ) -> *mut u8;
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
        if format == ImageFormat::Png {
            let encoder = image::codecs::png::PngEncoder::new_with_quality(
                buffer,
                image::codecs::png::CompressionType::Default,
                image::codecs::png::FilterType::Adaptive,
            );
            image.write_with_encoder(encoder).unwrap();
            return;
        }

        image.write_to(buffer, format).unwrap();
    })
}
// mtpng v0.4.1 breaks other benchmarks by forcing plain old zlib,
// so we cannot configure other crates with zlib-ng while it's present.
// TODO: re-enable.
//
//fn mtpng_encode(corpus: &[PathBuf]) -> (f64, f64) {
    // measure_encode(corpus, |buffer, image| {
    //     let mut options = mtpng::encoder::Options::new();
    //     options
    //         .set_compression_level(mtpng::CompressionLevel::Fast)
    //         .unwrap();
    //     let mut header = mtpng::Header::new();
    //     header
    //         .set_size(image.width() as u32, image.height() as u32)
    //         .unwrap();
    //     header
    //         .set_color(
    //             if image.color().has_alpha() {
    //                 mtpng::ColorType::TruecolorAlpha
    //             } else {
    //                 mtpng::ColorType::Truecolor
    //             },
    //             8,
    //         )
    //         .unwrap();

    //     let mut encoder = mtpng::encoder::Encoder::new(buffer, &options);
    //     encoder.write_header(&header).unwrap();
    //     encoder.write_image_rows(&image.as_bytes()).unwrap();
    //     encoder.finish().unwrap();
    //})
//}

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

fn measure_decode(corpus: &[PathBuf], rust_only: bool, decode_settings: DecodeSettings) {
    let mut image_total_time: HashMap<ImageFormat, Vec<u128>> = HashMap::new();
    let mut wuffs_total_time: HashMap<ImageFormat, Vec<u128>> = HashMap::new();
    let mut libwebp_total_time = Vec::new();
    let mut libpng_total_time = Vec::new();
    let mut spng_total_time = Vec::new();
    let mut stbi_total_time = Vec::new();
    let mut zune_png_total_time = Vec::new();
    let mut zune_qoi_total_time = Vec::new();
    let mut total_pixels = Vec::new();
    let mut names = Vec::new();

    let reencode = decode_settings.reencode;

    let bar = indicatif::ProgressBar::new(corpus.len() as u64);
    for path in corpus {
        bar.inc(1);
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(original_format) = image::guess_format(&bytes) else {
            continue;
        };

        let start = std::time::Instant::now();
        // let Ok(image) = image::load_from_memory(&bytes) else {
        //     continue;
        // };
        let image = image::load_from_memory(&bytes).unwrap();
        if let ColorType::La8 | ColorType::La16 = image.color() {
            // TODO: wuffs doesn't support LA
            continue;
        }
        if !reencode {
            image_total_time
                .entry(original_format)
                .or_default()
                .push(start.elapsed().as_nanos());
        }
        total_pixels.push(image.width() as u64 * image.height() as u64);
        names.push(path.to_str().unwrap().to_string());

        // PNG
        let mut reencoded_png = Vec::new();
        let png_bytes = if reencode && original_format != ImageFormat::Png {
            let mut image = image.clone();
            let mut encoder = png::Encoder::new(&mut reencoded_png, image.width(), image.height());
            if image.color().has_alpha() {
                image = DynamicImage::ImageRgba8(image.to_rgba8());
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
            } else {
                image = DynamicImage::ImageRgb8(image.to_rgb8());
                encoder.set_color(png::ColorType::Rgb);
                encoder.set_depth(png::BitDepth::Eight);
            }

            encoder.set_compression(match decode_settings.png_speed {
                Speed::Fast => png::Compression::Fast,
                Speed::Default => png::Compression::Default,
                Speed::Best => png::Compression::Best,
            });
            encoder.set_filter(match decode_settings.png_filter {
                Filter::None => png::FilterType::NoFilter,
                Filter::Sub => png::FilterType::Sub,
                Filter::Up => png::FilterType::Up,
                Filter::Average => png::FilterType::Avg,
                Filter::Paeth => png::FilterType::Paeth,
                Filter::Adaptive => png::FilterType::Paeth,
            });
            encoder.set_adaptive_filter(match decode_settings.png_filter {
                Filter::Adaptive => png::AdaptiveFilterType::Adaptive,
                _ => png::AdaptiveFilterType::NonAdaptive,
            });

            let mut encoder = encoder.write_header().unwrap();
            encoder.write_image_data(image.as_bytes()).unwrap();
            encoder.finish().unwrap();
            Some(&*reencoded_png)
        } else if original_format == ImageFormat::Png {
            Some(&*bytes)
        } else {
            None
        };
        if !rust_only {
            if let Some(png_bytes) = png_bytes {
                let start = std::time::Instant::now();
                let mut decoder = zune_png::PngDecoder::new(Cursor::new(&bytes));
                decoder.set_options(
                    zune_png::zune_core::options::DecoderOptions::new_fast()
                        .set_max_width(usize::MAX)
                        .set_max_height(usize::MAX),
                );
                black_box(decoder.decode().unwrap());
                zune_png_total_time.push(start.elapsed().as_nanos());

                let start = std::time::Instant::now();
                let mut width = 0;
                let mut height = 0;
                unsafe {
                    let decoded = libpng_decode(
                        bytes.as_ptr(),
                        bytes.len() as c_int,
                        &mut width as *mut _,
                        &mut height as *mut _,
                    );
                    libpng_total_time.push(start.elapsed().as_nanos());
                    assert_eq!(width, image.width() as i32);
                    assert_eq!(height, image.height() as i32);
                    libc::free(decoded);
                }

                let mut output = vec![0; image.as_bytes().len()];
                let mut start = std::time::Instant::now();
                let mut decoder = spng::Decoder::new(Cursor::new(&bytes))
                    .with_context_flags(spng::ContextFlags::IGNORE_ADLER32)
                    .with_output_format(match image.color() {
                        ColorType::L8 => spng::Format::G8,
                        ColorType::La8 => spng::Format::Ga8,
                        ColorType::Rgb8 => spng::Format::Rgb8,
                        ColorType::Rgba8 => spng::Format::Rgba8,
                        _ => spng::Format::Raw,
                    });
                let (info, mut reader) = decoder.read_info().unwrap();
                assert_eq!(info.width, image.width());
                assert_eq!(info.height, image.height());
                reader.next_frame(&mut output).unwrap();
                spng_total_time.push(start.elapsed().as_nanos());

                let start = std::time::Instant::now();
                let mut width = 0;
                let mut height = 0;
                let mut channels = 0;
                unsafe {
                    let decoded = stbi_load_from_memory(
                        bytes.as_ptr(),
                        bytes.len() as c_int,
                        &mut width as *mut _,
                        &mut height as *mut _,
                        &mut channels as *mut _,
                        0,
                    );
                    stbi_total_time.push(start.elapsed().as_nanos());
                    assert_eq!(width, image.width() as i32);
                    assert_eq!(height, image.height() as i32);
                    libc::free(decoded as *mut c_void);
                };
            }
        }

        // WEBP
        let mut reencoded_webp = Vec::new();
        let webp_bytes = if reencode
            && original_format != ImageFormat::WebP
            && image.width() < 16384
            && image.height() < 16384
        {
            let mut image = image.clone();
            match image.color() {
                ColorType::L16 => image = DynamicImage::ImageLuma8(image.to_luma8()),
                ColorType::La16 => image = DynamicImage::ImageLumaA8(image.to_luma_alpha8()),
                ColorType::Rgb16 => image = DynamicImage::ImageRgb8(image.to_rgb8()),
                ColorType::Rgba16 => image = DynamicImage::ImageRgba8(image.to_rgba8()),
                _ => {}
            }
            image
                .write_to(&mut Cursor::new(&mut reencoded_webp), ImageFormat::WebP)
                .unwrap();
            Some(&*reencoded_webp)
        } else if original_format == ImageFormat::WebP {
            Some(&*bytes)
        } else {
            None
        };
        if !rust_only {
            if let Some(webp_bytes) = webp_bytes {
                let start = std::time::Instant::now();
                let decoder = webp::Decoder::new(&webp_bytes);
                black_box(decoder.decode().unwrap());
                libwebp_total_time.push(start.elapsed().as_nanos());
            }
        }

        // QOI
        let mut reencoded_qoi = Vec::new();
        let qoi_bytes = if reencode && original_format != ImageFormat::Qoi {
            let mut image = image.clone();
            if image.color().has_alpha() {
                image = DynamicImage::ImageRgba8(image.to_rgba8());
            } else {
                image = DynamicImage::ImageRgb8(image.to_rgb8());
            }
            image
                .write_to(&mut Cursor::new(&mut reencoded_qoi), ImageFormat::Qoi)
                .unwrap();
            Some(&*reencoded_qoi)
        } else if original_format == ImageFormat::Qoi {
            Some(&*bytes)
        } else {
            None
        };
        if !rust_only {
            if let Some(qoi_bytes) = qoi_bytes {
                let start = std::time::Instant::now();
                let mut decoder = zune_qoi::QoiDecoder::new_with_options(
                    qoi_bytes,
                    zune_qoi::zune_core::options::DecoderOptions::new_fast()
                        .set_max_width(usize::MAX)
                        .set_max_height(usize::MAX),
                );
                black_box(decoder.decode().unwrap());
                zune_qoi_total_time.push(start.elapsed().as_nanos());
            }
        }

        // Collect wuffs times
        if !rust_only {
            for (format, bytes) in [
                (ImageFormat::Png, png_bytes),
                (ImageFormat::WebP, webp_bytes),
                (ImageFormat::Qoi, qoi_bytes),
            ] {
                let Some(bytes) = bytes else {
                    continue;
                };

                let start = std::time::Instant::now();
                let mut width = 0;
                let mut height = 0;
                let mut channels = 0;
                let desired_channels = match image.color() {
                    ColorType::L8 | ColorType::L16 => 1,
                    ColorType::La8 | ColorType::La16 => 2,
                    ColorType::Rgb8 | ColorType::Rgb16 => 3,
                    ColorType::Rgba8 | ColorType::Rgba16 => 4,
                    _ => 0,
                };
                unsafe {
                    let decoded = wuffs_load_from_memory(
                        bytes.as_ptr(),
                        bytes.len() as c_int,
                        &mut width as *mut _,
                        &mut height as *mut _,
                        &mut channels as *mut _,
                        desired_channels,
                    );
                    if decoded == std::ptr::null_mut() {
                        bar.println(format!(
                            "Wuffs failed to decode '{}' ({format:?})",
                            path.display()
                        ));
                        wuffs_total_time.entry(format).or_default().push(0);
                        continue;
                    }
                    wuffs_total_time
                        .entry(format)
                        .or_default()
                        .push(start.elapsed().as_nanos());
                    assert_eq!(width, image.width() as i32);
                    assert_eq!(height, image.height() as i32);
                    libc::free(decoded as *mut c_void);
                };
            }
        }

        // Collect image-rs times
        if reencode {
            for (format, bytes) in [
                (ImageFormat::Png, png_bytes),
                (ImageFormat::WebP, webp_bytes),
                (ImageFormat::Qoi, qoi_bytes),
            ] {
                let Some(bytes) = bytes else {
                    continue;
                };

                let start = std::time::Instant::now();
                image::load_from_memory(&bytes).unwrap();
                image_total_time
                    .entry(format)
                    .or_default()
                    .push(start.elapsed().as_nanos());
            }
        }
    }
    bar.finish_and_clear();

    let mut measurements = BTreeMap::new();
    let mut print_entry = |name: &str, time: &[u128]| {
        if time.is_empty() {
            return;
        }

        let speeds: Vec<_> = time
            .iter()
            .zip(total_pixels.iter())
            .filter(|(&x, y)| x > 0)
            .map(|(&x, &y)| (y as f64 / 1000_000f64) / (x as f64 * 1e-9))
            .collect();
        println!(
            "{name: <18}{:>6.3} MP/s (average) {:>6.3} MP/s (geomean)",
            mean(&speeds),
            geometric_mean(&speeds),
        );
        measurements.insert(name[..name.len() - 1].to_string(), time.to_vec());
    };

    // PNG results
    if image_total_time.contains_key(&ImageFormat::Png) {
        print_entry("image-rs PNG:", &image_total_time[&ImageFormat::Png]);
    }
    print_entry("zune-png:", &zune_png_total_time);
    if wuffs_total_time.contains_key(&ImageFormat::Png) {
        print_entry("wuffs PNG:", &wuffs_total_time[&ImageFormat::Png]);
    }
    print_entry("libpng:", &libpng_total_time);
    print_entry("spng:", &spng_total_time);
    print_entry("stb_image PNG:", &stbi_total_time);

    // WebP results
    if reencode && !rust_only {
        println!();
    }
    if image_total_time.contains_key(&ImageFormat::WebP) {
        print_entry("image-rs WebP:", &image_total_time[&ImageFormat::WebP]);
    }
    if wuffs_total_time.contains_key(&ImageFormat::WebP) {
        print_entry("wuffs WebP:", &wuffs_total_time[&ImageFormat::WebP]);
    }
    print_entry("libwebp:", &libwebp_total_time);

    // QOI results
    if reencode && !rust_only {
        println!();
    }
    if image_total_time.contains_key(&ImageFormat::Qoi) {
        print_entry("image-rs QOI:", &image_total_time[&ImageFormat::Qoi]);
    }
    if wuffs_total_time.contains_key(&ImageFormat::Qoi) {
        print_entry("wuffs QOI:", &wuffs_total_time[&ImageFormat::Qoi]);
    }
    print_entry("zune-qoi:", &zune_qoi_total_time);

    // Write CSV
    let mut csv_output = String::new();
    csv_output.push_str("name,total_pixels,");
    csv_output.push_str(&measurements.keys().cloned().collect::<Vec<_>>().join(","));
    csv_output.push('\n');
    for (i, (name, &num_pixels)) in names.iter_mut().zip(total_pixels.iter()).enumerate() {
        csv_output.push_str(&format!("{},{num_pixels},", name.replace(',', "_")));
        for time in measurements.values() {
            csv_output.push_str(&format!("{:.4},", time[i] as f64 * 1e-6));
        }
        csv_output.push('\n');
    }
    fs::write("measurements.csv", csv_output).unwrap();
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

                if !Path::new(&output_file).exists() {
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

                for j in 5..=7 {
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

        // for range in [
        //     0..8 * 1024,
        //     8 * 1024..64 * 1024,
        //     64 * 1024..512 * 1024,
        //     512 * 1024..1024 * 1024 * 1024,
        // ] {
        //     let ratios: Vec<_> = bytes
        //         .iter()
        //         .zip(total_bytes.iter())
        //         .filter(|(&x, &y)| range.contains(&y))
        //         .map(|(&x, &y)| 100.0 * x as f64 / y as f64)
        //         .collect();
        //     let speeds: Vec<_> = time
        //         .iter()
        //         .zip(total_bytes.iter())
        //         .filter(|(&x, &y)| range.contains(&y))
        //         .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
        //         .collect();

        //     println!(
        //         "{: >8}KB {name: <18}{:>6.1} MiB/s    {:02.2}%",
        //         range.end / 1024,
        //         geometric_mean(&speeds),
        //         geometric_mean(&ratios)
        //     );
        // }
        let ratios: Vec<_> = bytes
            .iter()
            .zip(total_bytes.iter())
            .map(|(&x, &y)| 100.0 * x as f64 / y as f64)
            .collect();
        let speeds: Vec<_> = time
            .iter()
            .zip(total_bytes.iter())
            .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
            .collect();

        println!(
            "{name: <18}{:>6.1} MiB/s    {:02.2}%",
            geometric_mean(&speeds),
            geometric_mean(&ratios)
        );
    };

    print_entry("fdeflate:", &fdeflate_bytes, &fdeflate_total_time);
    print_entry("zopfli:", &zopfli_bytes, &zopfli_total_time);
    for j in 0..=9 {
        print_entry(
            &format!("flate2[{}]", j),
            &miniz_oxide_bytes[j],
            &miniz_oxide_total_time[j],
        );
    }
}
