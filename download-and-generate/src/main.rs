use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::Path,
};

use atomic_write_file::AtomicWriteFile;
use byteorder_lite::{BigEndian, ReadBytesExt};
use futures_util::StreamExt;
use image::ImageFormat;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use tar::Archive;
use tokio::runtime::Runtime;

/// Based on: https://gist.github.com/Tapanhaz/096e299bf060607b572d700e89a62529
pub fn download_file(url: &str) -> Result<Vec<u8>, String> {
    Runtime::new().unwrap().block_on(async {
        // Reqwest setup
        let res = Client::new()
            .get(url)
            .send()
            .await
            .or(Err(format!("Failed to GET from '{}'", &url)))?;
        let total_size = res
            .content_length()
            .ok_or(format!("Failed to get content length from '{}'", &url))?;

        // Indicatif setup
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
        .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})").unwrap()
        .progress_chars("#>-"));
        pb.set_message(format!("Downloading {}", url));

        let mut contents = Vec::new();
        let mut downloaded: u64 = 0;
        let mut stream = res.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.or(Err(format!("Error while downloading file")))?;
            contents.extend_from_slice(&chunk);
            let new = total_size.min(downloaded + (chunk.len() as u64));
            downloaded = new;
            pb.set_position(new);
        }

        // pb.finish_with_message(format!("Downloaded {}", url));
        pb.finish_and_clear();
        return Ok(contents);
    })
}

fn write_file(path: &Path, contents: &[u8]) {
    let mut file = AtomicWriteFile::open(path).unwrap();
    file.write_all(contents).unwrap();
    file.commit().unwrap();
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let corpus_directory = root.join("corpus");

    fs::create_dir_all(&corpus_directory).unwrap();

    let qoi_benchmark_suite_tar = corpus_directory.join("qoi_benchmark_suite.tar");
    if !qoi_benchmark_suite_tar.exists() {
        let contents =
            download_file("https://qoiformat.org/benchmark/qoi_benchmark_suite.tar").unwrap();
        write_file(&qoi_benchmark_suite_tar, &contents);
        println!("Downloaded 'QOI benchmark suite' corpus");
    }

    let qoi_corpus_directory = corpus_directory.join("qoi_benchmark_suite");
    fs::create_dir_all(&qoi_corpus_directory).unwrap();

    let raw_directory = corpus_directory.join("raw");
    fs::create_dir_all(&raw_directory).unwrap();

    let total_entries = Archive::new(File::open(&qoi_benchmark_suite_tar).unwrap())
        .entries()
        .unwrap()
        .count();

    let mut extracted_file = false;
    let mut a = Archive::new(File::open(&qoi_benchmark_suite_tar).unwrap());
    let pb = ProgressBar::new(total_entries as u64);

    for (i, file) in a.entries_with_seek().unwrap().enumerate() {
        let mut file = file.unwrap();
        if file.path().unwrap().extension().unwrap_or_default() != "png" {
            pb.inc(1);
            continue;
        }

        let png_path = qoi_corpus_directory.join(format!("{i:04}.png"));
        let raw_path = raw_directory.join(format!("{i:04}.raw"));
        if png_path.exists() && raw_path.exists() {
            pb.inc(1);
            continue;
        }

        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();

        if !png_path.exists() {
            write_file(&png_path, &contents);
            extracted_file = true;
        }

        if !raw_path.exists() {
            let image = image::load_from_memory(&contents).unwrap();

            let mut buffer = Cursor::new(Vec::new());
            image.write_to(&mut buffer, ImageFormat::Png).unwrap();

            buffer.set_position(33);
            let idat_size = buffer.read_u32::<BigEndian>().unwrap();
            let idat_type = buffer.read_u32::<BigEndian>().unwrap();

            assert_eq!(idat_type, u32::from_be_bytes(*b"IDAT"));

            let mut raw = vec![0; idat_size as usize];
            buffer.read_exact(&mut raw).unwrap();
            write_file(&raw_path, &raw);
            extracted_file = true;
        }
        pb.inc(1);
    }
    pb.finish_and_clear();
    if extracted_file {
        println!("Extracted 'QOI benchmark suite' corpus");
    }
}
