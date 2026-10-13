//! Ikona aplikace (fialové T) vložená přímo v binárce.
pub fn rgba() -> (Vec<u8>, u32, u32) {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    let (w, h) = img.dimensions();
    (img.into_raw(), w, h)
}
