//! Large textures made with their pixels, without a staging copy where it can.
//!
//! `create_texture_with_data` (and `Queue::write_texture`) copy the pixels into
//! a staging buffer first; on WebGL that buffer is memory of the page's own, as
//! large as the texture, freed only once a later task sees the GPU's fence. For
//! the 96 MB shared atlas, made by both the loading screen's renderer and the
//! game's, that was 192 MB a page never gets back (its memory only grows). The
//! browser build sets an uploader ([`set_uploader`]) that hands the pixels to
//! WebGL directly.

use std::sync::OnceLock;

/// Makes a texture of a descriptor holding the pixels without staging them, or
/// says it can't (`None`): the caller then makes it as usual.
pub type Uploader =
    fn(&wgpu::Device, &wgpu::Queue, &wgpu::TextureDescriptor<'_>, &[u8]) -> Option<wgpu::Texture>;

static UPLOADER: OnceLock<Uploader> = OnceLock::new();

/// The browser build's direct upload, set once before the first renderer.
pub fn set_uploader(uploader: Uploader) {
    let _ = UPLOADER.set(uploader);
}

/// A texture of `desc` holding `pixels` (tightly packed rows of its one level
/// and layer): through the uploader when one is set and takes it.
pub(crate) fn texture_with_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    desc: &wgpu::TextureDescriptor<'_>,
    pixels: &[u8],
) -> wgpu::Texture {
    if let Some(texture) = UPLOADER
        .get()
        .and_then(|upload| upload(device, queue, desc, pixels))
    {
        return texture;
    }
    use wgpu::util::DeviceExt as _;
    device.create_texture_with_data(
        queue,
        desc,
        wgpu::util::TextureDataOrder::LayerMajor,
        pixels,
    )
}
