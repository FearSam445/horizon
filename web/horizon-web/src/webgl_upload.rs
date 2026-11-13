//! The shared atlas's upload straight to WebGL (`texSubImage2D`), without the
//! staging copy wgpu makes: see `horizon_client::gfx::direct_upload`.
#![allow(unsafe_code)]

use glow::HasContext as _;
use wgpu::hal::api::Gles;
use wgpu::hal::gles::TextureInner;

/// Hands the renderer the direct upload.
pub fn install() {
    horizon_client::gfx::direct_upload::set_uploader(texture_with_pixels);
}

/// A texture of `desc` holding `pixels` (one level, one layer, RGBA8, tightly
/// packed), uploaded by WebGL directly; `None` for any other kind of texture.
fn texture_with_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    desc: &wgpu::TextureDescriptor<'_>,
    pixels: &[u8],
) -> Option<wgpu::Texture> {
    let (width, height) = (desc.size.width, desc.size.height);
    let simple = desc.mip_level_count == 1
        && desc.size.depth_or_array_layers == 1
        && desc.dimension == wgpu::TextureDimension::D2
        && matches!(
            desc.format,
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb
        )
        && desc.usage.contains(wgpu::TextureUsages::COPY_DST)
        && pixels.len() == width as usize * height as usize * 4
        && pixels.len() >= 4;
    if !simple {
        return None;
    }
    let texture = device.create_texture(desc);
    // wgpu clears a texture it never saw written before its first use, which
    // would wipe pixels it doesn't know of: one texel through wgpu marks the
    // level written (clearing it first), and the submit runs that now.
    queue.write_texture(
        texture.as_image_copy(),
        &pixels[..4],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([]);
    // SAFETY: the texture is this device's, alive and used by no pass yet; the GL
    // state set here (the unpack buffer and parameters, the texture binding) is
    // what wgpu sets again before each of its own uploads and draws.
    let uploaded = unsafe {
        let hal_device = device.as_hal::<Gles>()?;
        let hal_texture = texture.as_hal::<Gles>()?;
        let TextureInner::Texture { raw, target } = hal_texture.inner else {
            return None;
        };
        let gl = hal_device.context().lock();
        gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_IMAGE_HEIGHT, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
        gl.bind_texture(target, Some(raw));
        gl.tex_sub_image_2d(
            target,
            0,
            0,
            0,
            width as i32,
            height as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(pixels)),
        );
        gl.bind_texture(target, None);
        gl.get_error() == glow::NO_ERROR
    };
    if uploaded {
        Some(texture)
    } else {
        log::warn!("Horizon renderer: a direct texture upload failed; staging it instead");
        None
    }
}
