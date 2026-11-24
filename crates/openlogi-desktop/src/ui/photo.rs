//! Device photos decoded at the size they are drawn at.
//!
//! The asset depot ships product renders up to ~2000 px on a side. `img(path)`
//! decodes a render at full size, and GPUI keeps every decoded image for the
//! life of the app, once on the CPU and once in the GPU atlas: about 46 MB for
//! one MX Master 3S and one MX Keys, more than the rest of the app together.
//!
//! [`fitted`] decodes a render once at the box it is drawn in times the
//! window's scale factor — never above the file's own size — and keeps at most
//! [`SIZES_PER_PHOTO`] sizes of one photo, dropping the least recently drawn
//! one, texture included, when a third appears.

use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, Asset, Global, ImageCacheError, ImageSource, RenderImage, Window};
use image::imageops::FilterType;

/// How many sizes of one photo stay decoded. A photo shows at one size in a
/// view; two cover a window moving between monitors of different scale.
const SIZES_PER_PHOTO: usize = 2;

/// The edges a decoded size snaps up to, in device pixels. Steps of about
/// ×1.4 keep a window resize from decoding a new size every frame, and cost at
/// most that much over the exact box.
const EDGES: [u32; 12] = [
    96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096,
];

/// A device photo at `path`, drawn in a box of `width` × `height` logical
/// pixels, decoded to fit that box on the current window's display.
pub(crate) fn fitted(path: impl Into<PathBuf>, width: f32, height: f32) -> ImageSource {
    let path: Arc<Path> = Arc::from(path.into());
    ImageSource::Custom(Arc::new(move |window: &mut Window, cx: &mut App| {
        let scale = window.scale_factor();
        let source = PhotoSource {
            path: Arc::clone(&path),
            max_width: snap(width * scale),
            max_height: snap(height * scale),
        };
        retain(&source, window, cx);
        window.use_asset::<FittedPhoto>(&source, cx)
    }))
}

/// One decoded size of one photo: the cache key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PhotoSource {
    path: Arc<Path>,
    /// The box the photo is fitted into, in device pixels.
    max_width: u32,
    max_height: u32,
}

/// The smallest of [`EDGES`] at least `device_pixels`, or the largest.
fn snap(device_pixels: f32) -> u32 {
    EDGES
        .into_iter()
        .find(|&edge| f64::from(edge) >= f64::from(device_pixels))
        .unwrap_or(EDGES[EDGES.len() - 1])
}

/// The decoded sizes of each photo, most recently drawn first.
#[derive(Default)]
struct DecodedSizes(Vec<PhotoSource>);

impl Global for DecodedSizes {}

impl DecodedSizes {
    /// Record that `source` was drawn, and answer the size it pushes out of
    /// its photo's [`SIZES_PER_PHOTO`], if any.
    fn drawn(&mut self, source: &PhotoSource) -> Option<PhotoSource> {
        if let Some(at) = self.0.iter().position(|kept| kept == source) {
            let kept = self.0.remove(at);
            self.0.insert(0, kept);
            return None;
        }
        self.0.insert(0, source.clone());
        let same_photo: Vec<usize> = self
            .0
            .iter()
            .enumerate()
            .filter(|(_, kept)| kept.path == source.path)
            .map(|(at, _)| at)
            .collect();
        let &oldest = same_photo.get(SIZES_PER_PHOTO)?;
        Some(self.0.remove(oldest))
    }
}

/// Note `source` as drawn, and release the size it pushes out.
fn retain(source: &PhotoSource, window: &mut Window, cx: &mut App) {
    let evicted = cx.default_global::<DecodedSizes>().drawn(source);
    if let Some(evicted) = evicted {
        if let Some(Ok(image)) = cx.fetch_asset::<FittedPhoto>(&evicted) {
            cx.drop_image(image, Some(window));
        }
        cx.remove_asset::<FittedPhoto>(&evicted);
    }
}

/// Loads a [`PhotoSource`] off the UI thread.
enum FittedPhoto {}

impl Asset for FittedPhoto {
    type Source = PhotoSource;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    #[expect(
        clippy::manual_async_fn,
        reason = "an `async fn` would borrow `cx` into the future, which must be 'static"
    )]
    fn load(
        source: Self::Source,
        _cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move { decode(&source).map_err(|error| ImageCacheError::Other(Arc::new(error))) }
    }
}

fn decode(source: &PhotoSource) -> anyhow::Result<Arc<RenderImage>> {
    let bytes = std::fs::read(&source.path)?;
    let photo = image::load_from_memory(&bytes)?.into_rgba8();
    let (width, height) = fit(photo.dimensions(), (source.max_width, source.max_height));
    let mut photo = if (width, height) == photo.dimensions() {
        photo
    } else {
        shrink(photo, width, height)
    };
    // GPUI's frames hold BGRA, as its own decoder leaves them.
    for pixel in photo.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new(vec![image::Frame::new(photo)])))
}

/// The size of an image of `natural` size fitted into `bounds`, keeping its
/// aspect, never enlarged and never below one pixel.
fn fit(natural: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
    let (width, height) = natural;
    let scale = (f64::from(bounds.0) / f64::from(width))
        .min(f64::from(bounds.1) / f64::from(height))
        .min(1.0);
    let scaled = |edge: u32| -> u32 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a positive edge scaled by at most 1 stays within u32"
        )]
        let scaled = (f64::from(edge) * scale).round() as u32;
        scaled.max(1)
    };
    (scaled(width), scaled(height))
}

/// Resize straight-alpha `photo` to `width` × `height`, filtering in
/// premultiplied alpha so the transparent background around a render does
/// not bleed a dark fringe into its edge.
fn shrink(mut photo: image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    for pixel in photo.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = premultiply(*channel, alpha);
        }
    }
    let mut photo = image::imageops::resize(&photo, width, height, FilterType::CatmullRom);
    for pixel in photo.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = unpremultiply(*channel, alpha);
        }
    }
    photo
}

fn premultiply(channel: u8, alpha: u16) -> u8 {
    // At most 255 * 255 / 255.
    u8::try_from((u16::from(channel) * alpha + 127) / 255).unwrap_or(u8::MAX)
}

fn unpremultiply(channel: u8, alpha: u16) -> u8 {
    if alpha == 0 {
        return 0;
    }
    u8::try_from((u16::from(channel) * 255 + alpha / 2) / alpha).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(path: &str, edge: u32) -> PhotoSource {
        PhotoSource {
            path: Arc::from(Path::new(path)),
            max_width: edge,
            max_height: edge,
        }
    }

    #[test]
    fn a_box_snaps_up_to_the_next_edge_and_caps_at_the_largest() {
        assert_eq!(snap(1.0), 96);
        assert_eq!(snap(96.0), 96);
        assert_eq!(snap(96.5), 128);
        // A 405 × 196 gallery photo at 200%.
        assert_eq!((snap(810.0), snap(392.0)), (1024, 512));
        assert_eq!(snap(10_000.0), 4096);
    }

    #[test]
    fn a_photo_fits_its_box_keeping_its_aspect_and_is_never_enlarged() {
        // The MX Master 3S render into a 1024 × 512 box: height-bound.
        assert_eq!(fit((1170, 1736), (1024, 512)), (345, 512));
        // The MX Keys render: width-bound.
        assert_eq!(fit((2004, 618), (1024, 512)), (1024, 316));
        // Already small enough: the file's own size.
        assert_eq!(fit((300, 200), (1024, 1024)), (300, 200));
        // A sliver never collapses to nothing.
        assert_eq!(fit((4000, 1), (96, 96)), (96, 1));
    }

    #[test]
    fn a_third_size_of_one_photo_pushes_out_the_least_recently_drawn() {
        let mut sizes = DecodedSizes::default();
        assert_eq!(sizes.drawn(&source("a.png", 512)), None);
        assert_eq!(sizes.drawn(&source("a.png", 1024)), None);
        // Another photo counts apart.
        assert_eq!(sizes.drawn(&source("b.png", 512)), None);
        // Drawing 512 again makes 1024 the least recent of `a`.
        assert_eq!(sizes.drawn(&source("a.png", 512)), None);
        assert_eq!(
            sizes.drawn(&source("a.png", 256)),
            Some(source("a.png", 1024))
        );
        assert_eq!(sizes.drawn(&source("b.png", 512)), None);
    }

    #[test]
    fn premultiplying_round_trips_opaque_and_clears_transparent() {
        for channel in [0, 1, 127, 254, 255] {
            assert_eq!(unpremultiply(premultiply(channel, 255), 255), channel);
        }
        assert_eq!(premultiply(200, 0), 0);
        assert_eq!(unpremultiply(0, 0), 0);
        // Half alpha keeps the colour within rounding.
        assert!(unpremultiply(premultiply(200, 128), 128).abs_diff(200) <= 1);
    }

    #[test]
    fn a_shrunk_photo_keeps_its_colour_at_a_transparent_edge() {
        // Opaque red on the left half, transparent black on the right.
        let photo = image::RgbaImage::from_fn(8, 2, |x, _| {
            if x < 4 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        });
        let small = shrink(photo, 4, 1);
        // The pixel straddling the edge is partly transparent but still red,
        // not darkened towards the transparent black.
        let edge = small.get_pixel(2, 0);
        assert!(edge[3] > 0 && edge[3] < 255, "{edge:?}");
        assert!(edge[0] > 240 && edge[1] < 16, "{edge:?}");
    }
}
