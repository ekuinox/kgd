use heif_sys::*;
use image::{DynamicImage, ImageBuffer, Rgb};
use std::path::Path;
use std::ptr;
use std::slice;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HeifError {
    #[error("Failed to allocate heif context")]
    NullContext,

    #[error("Failed to read HEIF data: error code {0}")]
    ReadData(i32),

    #[error("Failed to get primary image handle: error code {0}")]
    GetPrimaryImage(i32),

    #[error("Failed to decode image: error code {0}")]
    DecodeImage(i32),

    #[error("Failed to get image plane data")]
    GetPlaneData,

    #[error("Failed to create image buffer")]
    CreateImageBuffer,

    #[error("Failed to save image: {0}")]
    SaveImage(#[from] image::ImageError),

    #[error("Invalid path")]
    InvalidPath,

    #[error("Failed to read file: {0}")]
    ReadFile(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, HeifError>;

/// Read HEIF/HEIC data from bytes and decode to a DynamicImage.
///
/// # Arguments
/// * `bytes` - HEIF/HEIC file data as bytes
///
/// # Returns
/// A `DynamicImage` containing the decoded image data.
///
/// # Example
/// ```no_run
/// use heif::read_heif_to_dynamic_image;
///
/// let bytes = std::fs::read("input.heic").unwrap();
/// let image = read_heif_to_dynamic_image(&bytes).unwrap();
/// ```
pub fn read_heif_to_dynamic_image(bytes: &[u8]) -> Result<DynamicImage> {
    unsafe { decode_heif_bytes_inner(bytes) }
}

/// Convert a HEIF/HEIC file to JPEG format.
///
/// # Arguments
/// * `input_path` - Path to the input HEIF/HEIC file
/// * `output_path` - Path to the output JPEG file
///
/// # Example
/// ```no_run
/// use heif::heif_to_jpeg;
///
/// heif_to_jpeg("input.heic", "output.jpg").unwrap();
/// ```
pub fn heif_to_jpeg<P: AsRef<Path>, Q: AsRef<Path>>(
    input_path: P,
    output_path: Q,
) -> Result<()> {
    let bytes = std::fs::read(input_path)?;
    let image = read_heif_to_dynamic_image(&bytes)?;
    image.save(output_path)?;
    Ok(())
}

unsafe fn decode_heif_bytes_inner(bytes: &[u8]) -> Result<DynamicImage> {
    // Create context
    let ctx = unsafe { heif_context_alloc() };
    if ctx.is_null() {
        return Err(HeifError::NullContext);
    }

    // Read HEIF data from memory
    let err = unsafe {
        heif_context_read_from_memory_without_copy(
            ctx,
            bytes.as_ptr() as *const std::ffi::c_void,
            bytes.len(),
            ptr::null(),
        )
    };
    if err.code != heif_error_code_heif_error_Ok {
        unsafe { heif_context_free(ctx) };
        return Err(HeifError::ReadData(err.code as i32));
    }

    // Get primary image handle
    let mut handle: *mut heif_image_handle = ptr::null_mut();
    let err = unsafe { heif_context_get_primary_image_handle(ctx, &mut handle) };
    if err.code != heif_error_code_heif_error_Ok {
        unsafe { heif_context_free(ctx) };
        return Err(HeifError::GetPrimaryImage(err.code as i32));
    }

    // Decode image to RGB
    let mut image: *mut heif_image = ptr::null_mut();
    let err = unsafe {
        heif_decode_image(
            handle,
            &mut image,
            heif_colorspace_heif_colorspace_RGB,
            heif_chroma_heif_chroma_interleaved_RGB,
            ptr::null(),
        )
    };
    if err.code != heif_error_code_heif_error_Ok {
        unsafe {
            heif_image_handle_release(handle);
            heif_context_free(ctx);
        }
        return Err(HeifError::DecodeImage(err.code as i32));
    }

    // Get image dimensions
    let width = unsafe { heif_image_get_primary_width(image) } as u32;
    let height = unsafe { heif_image_get_primary_height(image) } as u32;

    // Get pixel data
    let mut stride: i32 = 0;
    let data = unsafe {
        heif_image_get_plane_readonly(image, heif_channel_heif_channel_interleaved, &mut stride)
    };
    if data.is_null() {
        unsafe {
            heif_image_release(image);
            heif_image_handle_release(handle);
            heif_context_free(ctx);
        }
        return Err(HeifError::GetPlaneData);
    }

    // Copy pixel data to Vec
    let stride = stride as usize;
    let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        let row_start = (y as usize) * stride;
        let row_data =
            unsafe { slice::from_raw_parts(data.add(row_start), (width * 3) as usize) };
        rgb_data.extend_from_slice(row_data);
    }

    // Cleanup libheif resources
    unsafe {
        heif_image_release(image);
        heif_image_handle_release(handle);
        heif_context_free(ctx);
    }

    // Create image buffer
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgb_data).ok_or(HeifError::CreateImageBuffer)?;

    Ok(DynamicImage::ImageRgb8(img))
}
