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
    #[cfg(feature = "extract-raw")]
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
        #[cfg(feature = "extract-raw")]
        Mode::ExtractRaw => extract_raw(),
        Mode::GenerateCompressed => generate_compressed(),
        Mode::Deflate => deflate(args.rust_only),
        Mode::Inflate => inflate(args.rust_only),
    }
    innumerable::print_counts();
}

#[cfg(feature = "extract-raw")]
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

    // let corpus = &corpus[..10];

    let run_corpus = |corpus: &[PathBuf], name: &str, f: Box<dyn Fn(&[u8]) -> Vec<u8>>| {
        let mut total_bytes = Vec::new();
        let mut compressed_bytes = Vec::new();
        let mut total_time = Vec::new();

        let bar = indicatif::ProgressBar::new(corpus.len() as u64);
        for path in corpus {
            if let Ok(mut bytes) = fs::read(path) {
                let uncompressed = fdeflate::decompress_to_vec(&bytes).unwrap();
                let start = Instant::now();
                let compressed = f(&uncompressed);
                let duration = start.elapsed().as_nanos();

                assert_eq!(
                    uncompressed,
                    fdeflate::decompress_to_vec(&compressed).unwrap()
                );

                total_bytes.push(uncompressed.len());
                compressed_bytes.push(compressed.len());
                total_time.push(duration);
            }
            bar.inc(1);
        }
        bar.finish_and_clear();

        let ratios: Vec<_> = compressed_bytes
            .iter()
            .zip(total_bytes.iter())
            .map(|(&x, &y)| 100.0 * x as f64 / y as f64)
            .collect();
        let speeds: Vec<_> = total_time
            .iter()
            .zip(total_bytes.iter())
            .map(|(&x, &y)| (y as f64 / (1 << 20) as f64) / (x as f64 * 1e-9))
            .collect();

        println!(
            "{name: <12}{:>6.1} MiB/s    {:02.2}%",
            geometric_mean(&speeds),
            geometric_mean(&ratios)
        );
    };

    for j in 3..=3 {
        run_corpus(
            &corpus,
            &format!("fdeflate[{j}]:"),
            Box::new(move |uncompressed| {
                fdeflate::compress_to_vec_with_level(uncompressed, j as u8)
            }),
        );
    }

    if !rust_only {
        for j in 1..=1 {
            run_corpus(
                &corpus,
                &format!("miniz_oxide[{j}]:"),
                Box::new(move |uncompressed| {
                    let mut encoder = flate2::write::ZlibEncoder::new(
                        Vec::new(),
                        flate2::Compression::new(j as u32),
                    );
                    encoder.write_all(&uncompressed).unwrap();
                    encoder.flush_finish().unwrap()
                }),
            );
        }
        // run_corpus(
        //     &corpus,
        //     "zopfli:",
        //     Box::new(|uncompressed| {
        //         let mut zopfli_compressed = Vec::new();
        //         zopfli::compress(
        //             zopfli::Options {
        //                 iteration_count: NonZeroU64::new(1).unwrap(),
        //                 ..Default::default()
        //             },
        //             zopfli::Format::Zlib,
        //             &*uncompressed,
        //             &mut zopfli_compressed,
        //         )
        //         .unwrap();
        //         zopfli_compressed
        //     }),
        // );
    }
}
