use std::{
    ffi::{c_int, c_void},
    io::Cursor,
};

use harness::EncodeImplFn;
use image::{ColorType, ImageDecoder, ImageReader};

unsafe extern "C" {
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
    let impls: Vec<EncodeImplFn<Vec<u8>, ()>> = vec![
        (
            "image-webp".into(),
            Box::new(move |bytes| {
                let _ = image::load_from_memory(bytes).unwrap();
            }),
        ),
        (
            "libwebp".into(),
            Box::new(move |bytes| {
                let decoder = webp::Decoder::new(&bytes);
                decoder.decode().unwrap();
            }),
        ),
        (
            "wuffs-webp".into(),
            Box::new(move |bytes| {
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
    ];

    let prepare = Box::new(|input: &[u8]| {
        let decoder = ImageReader::new(Cursor::new(&input))
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .ok()?;

        let size = decoder.dimensions();
        Some((
            size.0 as f64 * size.1 as f64 * 1e-6,
            input.len(),
            input.to_vec(),
        ))
    });

    let check = Box::new(|_: &(), _: &Vec<u8>| -> bool { true });

    harness::encode(
        harness::Corpus::CwebpQoiBench,
        prepare,
        impls,
        check,
        "MP/s",
    );
}
