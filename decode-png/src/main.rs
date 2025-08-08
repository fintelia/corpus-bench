use std::{
    ffi::{c_int, c_void},
    io::Cursor,
};

use image::{ColorType, ImageDecoder, ImageReader};

unsafe extern "C" {
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

fn main() {
    harness::decode(
        harness::Corpus::QoiBench,
        vec![
            (
                "image-png".into(),
                Box::new(move |bytes: &[u8]| {
                    let _ = image::load_from_memory(bytes).unwrap();
                }),
            ),
            (
                "wuffs-png".into(),
                Box::new(move |bytes: &[u8]| {
                    let desired_channels = {
                        let decoder = ImageReader::new(Cursor::new(bytes))
                            .with_guessed_format()
                            .unwrap()
                            .into_decoder()
                            .unwrap();
                        match decoder.color_type() {
                            ColorType::L8 | ColorType::L16 => 1,
                            ColorType::La8 | ColorType::La16 => 2,
                            ColorType::Rgb8 | ColorType::Rgb16 => 3,
                            ColorType::Rgba8 | ColorType::Rgba16 => 4,
                            _ => 0,
                        }
                    };

                    let mut width = 0;
                    let mut height = 0;
                    let mut channels = 0;
                    unsafe {
                        let decoded = wuffs_load_from_memory(
                            bytes.as_ptr(),
                            bytes.len() as c_int,
                            &mut width as *mut _,
                            &mut height as *mut _,
                            &mut channels as *mut _,
                            desired_channels,
                        );
                        libc::free(decoded as *mut c_void);
                    }
                }),
            ),
            (
                "zune-png".into(),
                Box::new(move |bytes: &[u8]| {
                    let mut decoder = zune_png::PngDecoder::new(Cursor::new(&bytes));
                    decoder.set_options(
                        zune_png::zune_core::options::DecoderOptions::new_fast()
                            .set_max_width(usize::MAX)
                            .set_max_height(usize::MAX),
                    );
                    decoder.decode().unwrap();
                }),
            ),
            (
                "libpng".into(),
                Box::new(move |bytes: &[u8]| {
                    let mut width = 0;
                    let mut height = 0;
                    unsafe {
                        let decoded = libpng_decode(
                            bytes.as_ptr(),
                            bytes.len() as c_int,
                            &mut width as *mut _,
                            &mut height as *mut _,
                        );
                        libc::free(decoded);
                    }
                }),
            ),
            (
                "stb_image".into(),
                Box::new(move |bytes: &[u8]| {
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
                        libc::free(decoded as *mut c_void);
                    }
                }),
            ),
            (
                "spng".into(),
                Box::new(move |bytes: &[u8]| {
                    let output_format = {
                        let decoder = ImageReader::new(Cursor::new(bytes))
                            .with_guessed_format()
                            .unwrap()
                            .into_decoder()
                            .unwrap();
                        match decoder.color_type() {
                            ColorType::L8 => spng::Format::G8,
                            ColorType::La8 => spng::Format::Ga8,
                            ColorType::Rgb8 => spng::Format::Rgb8,
                            ColorType::Rgba8 => spng::Format::Rgba8,
                            _ => spng::Format::Raw,
                        }
                    };

                    let decoder = spng::Decoder::new(Cursor::new(&bytes))
                        .with_context_flags(spng::ContextFlags::IGNORE_ADLER32)
                        .with_output_format(output_format);

                    let (info, mut reader) = decoder.read_info().unwrap();
                    let mut output = vec![0u8; info.buffer_size];
                    reader.next_frame(&mut output).unwrap();
                }),
            ),
        ],
    );
}
