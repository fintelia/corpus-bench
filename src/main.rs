use std::{
    io::{Cursor, Write},
    path::PathBuf,
};

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
}

/// The mode to run the benchmark in
#[derive(ValueEnum, Clone, Debug)]
enum Mode {
    /// Measure the performance of encoding
    Encode,
    /// Measure the performance of decoding
    Decode,
}

/// The corpus to choose from
#[derive(ValueEnum, Clone, Debug)]
enum Corpus {
    /// The QOI Benchmark corpus
    QoiBench,
}
impl Corpus {
    fn get_corpus(&self) -> Vec<PathBuf> {
        let directory = match self {
            Corpus::QoiBench => "corpus/qoi_benchmark_suite",
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
        Mode::Decode => {
            println!("Running decoding benchmark with corpus: {:?}", args.corpus);
        }
    }
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
