use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use rand::prelude::*;
use regex::Regex;
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
fn mean_ratio(n: &[f64], d: &[f64]) -> f64 {
    mean(n) / mean(d)
}
fn geometric_mean_ratio(n: &[f64], d: &[f64]) -> f64 {
    let exponent = 1.0 / n.len() as f64;
    n.iter()
        .zip(d)
        .fold(1.0, |acc, (&n, &d)| acc * (n / d).powf(exponent))
}

struct Filter {
    done: bool,
    single: bool,
    regex: Option<Regex>,
}
impl Filter {
    fn load() -> Self {
        let args = std::env::args().collect::<Vec<_>>();
        if args.iter().any(|a| a == "--single") {
            return Filter {
                done: false,
                single: true,
                regex: None,
            };
        }

        if let Some(i) = args.iter().position(|x| x == "--filter") {
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                return Filter {
                    done: false,
                    single: false,
                    regex: Some(Regex::new(&args[i + 1]).expect("Invalid regex pattern")),
                };
            }
        }
        Filter {
            done: false,
            single: false,
            regex: None,
        }
    }

    fn skip(&mut self, name: &str) -> bool {
        if self.done {
            return true;
        }

        if self.single {
            self.done = true;
            return false;
        }

        if let Some(ref regex) = self.regex {
            return !regex.is_match(name);
        }

        false
    }
}

pub trait ToCompressedSize {
    fn to_compressed_size(&self) -> Option<usize>;
}
impl ToCompressedSize for &[u8] {
    fn to_compressed_size(&self) -> Option<usize> {
        Some(self.len())
    }
}
impl ToCompressedSize for Vec<u8> {
    fn to_compressed_size(&self) -> Option<usize> {
        Some(self.len())
    }
}

impl ToCompressedSize for () {
    fn to_compressed_size(&self) -> Option<usize> {
        None
    }
}

pub type PrepareFn<T> = Box<dyn FnMut(&[u8]) -> Option<(f64, usize, T)>>;
pub type EncodeImplFn<T, U> = (String, Box<dyn FnMut(&T) -> U>);
pub type CheckFn<T, U> = Box<dyn FnMut(&U, &T) -> bool>;
pub fn encode<T, U: ToCompressedSize>(
    corpus: Corpus,
    mut prepare: PrepareFn<T>,
    impls: Vec<EncodeImplFn<T, U>>,
    mut check: CheckFn<T, U>,
    bandwidth_unit: &'static str,
) {
    let mut filter = Filter::load();

    let fast = std::env::args().any(|a| a == "--fast");

    handle_ctrlc();
    let corpus_files = corpus.get_corpus();
    'outer: for (name, mut impl_fn) in impls {
        if filter.skip(&name) {
            continue;
        }

        let bar = indicatif::ProgressBar::new(corpus_files.len() as u64);

        let mut speeds = Vec::new();
        let mut compressed_bytes = Vec::new();
        let mut total_bytes = Vec::new();
        for path in &corpus_files {
            if EXIT.load(std::sync::atomic::Ordering::SeqCst) {
                bar.finish_and_clear();
                break 'outer;
            }

            if fast && crc32fast::hash(path.to_string_lossy().as_bytes()) > u32::MAX / 10 {
                bar.inc(1);
                continue;
            }

            let input = fs::read(&path).unwrap();
            let Some((size, bytes, img)) = prepare(&input) else {
                continue;
            };

            let start = std::time::Instant::now();
            let output = impl_fn(&img);
            speeds.push(size / start.elapsed().as_secs_f64());
            total_bytes.push(bytes as f64);
            if let Some(bytes) = output.to_compressed_size() {
                compressed_bytes.push(bytes as f64);
            }

            check(&output, &img);

            bar.inc(1);
        }
        bar.finish_and_clear();

        let name = format!("{name}:");
        if compressed_bytes.is_empty() {
            println!(
                "{name: <18}{:>7.1} {bandwidth_unit} (average) {:>7.1} {bandwidth_unit} (geomean)",
                mean(&speeds),
                geometric_mean(&speeds),
            );
        } else {
            println!(
                "{name: <18}{:>7.1} {bandwidth_unit} (average) {:>7.1} {bandwidth_unit} (geomean)    {:6.2}% (average) {:6.2}% (geomean)",
                mean(&speeds),
                geometric_mean(&speeds),
                mean_ratio(&compressed_bytes, &total_bytes) * 100.0,
                geometric_mean_ratio(&compressed_bytes, &total_bytes) * 100.0,
            );
        }
    }

    innumerable::print_counts();
}
