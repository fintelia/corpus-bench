use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use image::{ColorType, ImageDecoder, ImageReader};
use rand::prelude::*;
use walkdir::WalkDir;

/// The corpus to choose from
#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum Corpus {
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

        paths.shuffle(&mut rand::rng());
        paths
    }
    fn get_corpus(&self) -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        Self::get_recursive(&root.join(Path::new(match self {
            Corpus::QoiBench => "corpus/qoi_benchmark_suite",
            Corpus::CwebpQoiBench => "corpus/cwebp_qoi_bench",
            Corpus::Raw => "corpus/raw",
        })))
    }
}

static EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn handle_ctrlc() {
    ctrlc::set_handler(move || {
        if EXIT.swap(true, std::sync::atomic::Ordering::SeqCst) {
            std::process::exit(0);
        }
    })
    .expect("Error setting Ctrl-C handler");
}

fn geometric_mean(v: &[f64]) -> f64 {
    v.iter()
        .fold(1.0, |acc, &x| acc * (x as f64).powf(1.0 / v.len() as f64))
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

// pub type EncodeFn<T> = (&'static str, Box<dyn FnMut(&T) -> Vec<u8>>);
// pub fn encode<T, F>(
//     corpus: Corpus,
//     size_unit: &'static str,
//     mut load: F,
//     mut impls: Vec<EncodeFn<T>>,
// ) where
//     F: FnMut(&[u8]) -> Option<(T, f64)>,
// {
//     handle_ctrlc();
//     let corpus = corpus.get_corpus();
//     let bar = indicatif::ProgressBar::new(corpus.len() as u64);
//     for (name, impl_fn) in impls.iter_mut() {
//         bar.reset();

//         let mut speeds = Vec::new();
//         let mut ratios = Vec::new();
//         for path in &corpus {
//             if EXIT.load(std::sync::atomic::Ordering::SeqCst) {
//                 bar.finish_and_clear();
//                 return;
//             }

//             let bytes = fs::read(&path).unwrap();
//             if let Some((data, size)) = load(&bytes) {
//                 let start = std::time::Instant::now();
//                 let output = impl_fn(&data);
//                 speeds.push(size / start.elapsed().as_secs_f64());
//                 ratios.push(output.len() as f64 / bytes.len() as f64);
//             }

//             bar.inc(1);
//         }
//         bar.finish_and_clear();

//         println!(
//             "{name: <12}{:>6.1} {size_unit}/s    {:02.2}%",
//             geometric_mean(&speeds),
//             geometric_mean(&ratios)
//         );
//     }
// }

pub type RunImplFn = (String, Box<dyn FnMut(&[u8]) -> Vec<u8>>);

pub fn run(corpus: Corpus, print_ratio: bool, mut impls: Vec<RunImplFn>) {
    let args = std::env::args().collect::<Vec<_>>();
    let range = if let Some(i) = args.iter().position(|x| x == "--single") {
        if i + 1 < args.len() && !args[i + 1].starts_with('-') {
            let j = impls
                .iter()
                .position(|(name, _)| name == &args[i + 1])
                .expect("Invalid implementation name");
            j..=j
        } else {
            0..=0
        }
    } else {
        0..=impls.len() - 1
    };

    handle_ctrlc();
    let corpus_files = corpus.get_corpus();
    'outer: for (name, impl_fn) in &mut impls[range] {
        let bar = indicatif::ProgressBar::new(corpus_files.len() as u64);

        let mut speeds = Vec::new();
        let mut ratios = Vec::new();
        for path in &corpus_files {
            if EXIT.load(std::sync::atomic::Ordering::SeqCst) {
                bar.finish_and_clear();
                break 'outer;
            }

            let mut input = fs::read(&path).unwrap();
            if corpus == Corpus::Raw {
                input = fdeflate::decompress_to_vec(&input).unwrap();
            }

            let start = std::time::Instant::now();
            let output = impl_fn(&input);
            speeds.push(input.len() as f64 / (start.elapsed().as_secs_f64() * 1024.0 * 1024.0));

            if print_ratio {
                ratios.push(100.0 * output.len() as f64 / input.len() as f64);
            }

            bar.inc(1);
        }
        bar.finish_and_clear();

        let name = format!("{name}:");
        if print_ratio {
            println!(
                "{name: <12}{:>6.1} MiB/s    {:02.2}%",
                geometric_mean(&speeds),
                geometric_mean(&ratios)
            );
        } else {
            println!("{name: <12}{:>6.1} MiB/s", geometric_mean(&speeds));
        }
    }

    innumerable::print_counts();
}

pub type DecodeImplFn = (String, Box<dyn FnMut(&[u8])>);

pub fn decode(corpus: Corpus, mut impls: Vec<DecodeImplFn>) {
    let args = std::env::args().collect::<Vec<_>>();
    let range = if let Some(i) = args.iter().position(|x| x == "--single") {
        if i + 1 < args.len() && !args[i + 1].starts_with('-') {
            let j = impls
                .iter()
                .position(|(name, _)| name == &args[i + 1])
                .expect("Invalid implementation name");
            j..=j
        } else {
            0..=0
        }
    } else {
        0..=impls.len() - 1
    };

    handle_ctrlc();
    let corpus_files = corpus.get_corpus();
    'outer: for (name, impl_fn) in &mut impls[range] {
        let bar = indicatif::ProgressBar::new(corpus_files.len() as u64);

        let mut speeds = Vec::new();
        for path in &corpus_files {
            if EXIT.load(std::sync::atomic::Ordering::SeqCst) {
                bar.finish_and_clear();
                break 'outer;
            }

            let input = fs::read(&path).unwrap();
            let size = {
                let Ok(decoder) = ImageReader::new(Cursor::new(&input))
                    .with_guessed_format()
                    .unwrap()
                    .into_decoder()
                else {
                    continue;
                };

                if let ColorType::La8 | ColorType::La16 = decoder.color_type() {
                    continue;
                }

                decoder.dimensions()
            };
            // let size = reader.into_dimensions().unwrap();

            let start = std::time::Instant::now();
            impl_fn(&input);
            speeds.push(size.0 as f64 * size.1 as f64 * 1e-6 / start.elapsed().as_secs_f64());

            bar.inc(1);
        }
        bar.finish_and_clear();

        let name = format!("{name}:");
        println!(
            "{name: <18}{:>7.2} MP/s (average) {:>7.2} MP/s (geomean)",
            mean(&speeds),
            geometric_mean(&speeds),
        );
    }

    innumerable::print_counts();
}
