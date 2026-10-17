//! Shared CPU codec checks, also executed by the finite browser example.
use macroquad::prelude::{Image, ImageFormat};
use std::io::Cursor;

const RGBA: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 12, 34, 56, 255,
];

pub fn png_bytes() -> Vec<u8> {
    let pixels = image::RgbaImage::from_raw(2, 2, RGBA.to_vec()).unwrap();
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut output, ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

pub fn verify_codec_boundaries() {
    let png = png_bytes();
    for format in [None, Some(ImageFormat::Png)] {
        let decoded = Image::from_file_with_format(&png, format).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        assert_eq!(decoded.bytes(), RGBA);
    }

    // Independent uncompressed, top-left-origin, BGRA TGA fixture. Its two rows
    // and nonopaque pixels catch channel, alpha and orientation regressions.
    let mut tga = vec![0; 18];
    tga[2] = 2;
    tga[12] = 2;
    tga[14] = 2;
    tga[16] = 32;
    tga[17] = 0x28;
    tga.extend_from_slice(&[
        255, 0, 0, 0, 0, 255, 0, 128, 0, 0, 255, 255, 56, 34, 12, 255,
    ]);
    let decoded = Image::from_file_with_format(&tga, Some(ImageFormat::Tga)).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (2, 2));
    assert_eq!(
        decoded.bytes(),
        &[0, 0, 255, 0, 0, 255, 0, 128, 255, 0, 0, 255, 12, 34, 56, 255]
    );

    for (bytes, format) in [
        (&[][..], None),
        (&b"not an image"[..], None),
        (&png[..png.len() / 2], Some(ImageFormat::Png)),
        (&tga[..tga.len() - 1], Some(ImageFormat::Tga)),
        (&png[..], Some(ImageFormat::Tga)),
    ] {
        assert!(Image::from_file_with_format(bytes, format).is_err());
    }
}

#[test]
fn png_tga_decode_and_malformed_inputs() {
    verify_codec_boundaries();
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn png_export_preserves_alpha_and_flips_rows() {
    let path =
        std::env::temp_dir().join(format!("macroquad-image-export-{}.png", std::process::id()));
    let source = Image::from_rgba8(2, 2, RGBA.to_vec()).unwrap();
    source.export_png(path.to_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    let decoded = Image::from_file_with_format(&bytes, None).unwrap();
    assert_eq!(decoded.bytes(), [&RGBA[8..], &RGBA[..8]].concat());
}
