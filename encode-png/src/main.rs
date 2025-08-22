use harness::{Corpus, EncodeImplFn};
use image::{ColorType, DynamicImage, ImageEncoder};

unsafe extern "C" {
    fn libpng_encode(
        pixels: *const u8,
        w: i32,
        h: i32,
        channels: i32,
        bit_depth: i32,
        level: i32,
        out_len: *mut i32,
    ) -> *mut u8;
}

fn encode_image_rs(img: &DynamicImage, compression: png::DeflateCompression) -> Vec<u8> {
    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, img.width(), img.height());
    encoder.set_color(match img.color() {
        ColorType::L8 | ColorType::L16 => png::ColorType::Grayscale,
        ColorType::La8 | ColorType::La16 => png::ColorType::GrayscaleAlpha,
        ColorType::Rgb8 | ColorType::Rgb16 => png::ColorType::Rgb,
        ColorType::Rgba8 | ColorType::Rgba16 => png::ColorType::Rgba,
        _ => panic!("Unsupported image type for PNG encoding"),
    });
    encoder.set_depth(
        if img.color().bytes_per_pixel() > img.color().channel_count() {
            png::BitDepth::Sixteen
        } else {
            png::BitDepth::Eight
        },
    );
    encoder.set_deflate_compression(compression);
    encoder.set_filter(
        if let png::DeflateCompression::NoCompression = compression {
            png::Filter::NoFilter
        } else {
            png::Filter::Adaptive
        },
    );
    encoder
        .write_header()
        .unwrap()
        .write_image_data(img.as_bytes())
        .unwrap();
    output
}

fn main() {
    let mut impls: Vec<EncodeImplFn> = Vec::new();

    impls.push((
        format!("image-png0"),
        Box::new(move |img: &DynamicImage| {
            encode_image_rs(img, png::DeflateCompression::NoCompression)
        }),
    ));

    impls.push((
        format!("image-png-ultra"),
        Box::new(move |img: &DynamicImage| {
            encode_image_rs(img, png::DeflateCompression::FdeflateUltraFast)
        }),
    ));

    for level in 1..=9 {
        impls.push((
            format!("image-png{level}"),
            Box::new(move |img: &DynamicImage| {
                encode_image_rs(img, png::DeflateCompression::Level(level as u8))
            }),
        ));
    }

    for level in 0..=9 {
        impls.push((
            format!("libpng{level}"),
            Box::new(move |img: &DynamicImage| {
                let mut out_len = 0;
                let pixels = img.as_bytes();
                let ptr = unsafe {
                    libpng_encode(
                        pixels.as_ptr(),
                        img.width() as i32,
                        img.height() as i32,
                        img.color().channel_count() as i32,
                        if img.color().bytes_per_pixel() / img.color().channel_count() == 1 {
                            8
                        } else {
                            16
                        },
                        level,
                        &mut out_len,
                    )
                };
                unsafe { Vec::from_raw_parts(ptr, out_len as usize, out_len as usize) }
            }),
        ));
    }

    impls.push((
        format!("qoi"),
        Box::new(|img: &DynamicImage| {
            let img: DynamicImage = if img.color().has_alpha() {
                img.to_rgba8().into()
            } else {
                img.to_rgb8().into()
            };

            qoi::encode_to_vec(img.as_bytes(), img.width(), img.height())
                .unwrap()
        }),
    ));

    harness::encode(Corpus::QoiBench, impls);
}
