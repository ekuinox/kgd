#![allow(unsafe_op_in_unsafe_fn)]

use heif::*;
use image::{ImageBuffer, Rgb};
use std::env;
use std::ffi::CString;
use std::path::Path;
use std::ptr;
use std::slice;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <input.heic> [output.jpg]", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        let path = Path::new(input_path);
        let stem = path.file_stem().unwrap().to_str().unwrap();
        format!("{}.jpg", stem)
    };

    unsafe {
        convert_heif_to_jpg(input_path, &output_path).unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        });
    }

    println!("Converted {} -> {}", input_path, output_path);
}

unsafe fn convert_heif_to_jpg(input_path: &str, output_path: &str) -> Result<(), String> {
    let input_cstr = CString::new(input_path).map_err(|e| e.to_string())?;

    // Create context
    let ctx = heif_context_alloc();
    if ctx.is_null() {
        return Err("Failed to allocate heif context".to_string());
    }

    // Read HEIF file
    let err = heif_context_read_from_file(ctx, input_cstr.as_ptr(), ptr::null());
    if err.code != heif_error_code_heif_error_Ok {
        heif_context_free(ctx);
        return Err(format!("Failed to read HEIF file: {:?}", err.code));
    }

    // Get primary image handle
    let mut handle: *mut heif_image_handle = ptr::null_mut();
    let err = heif_context_get_primary_image_handle(ctx, &mut handle);
    if err.code != heif_error_code_heif_error_Ok {
        heif_context_free(ctx);
        return Err(format!("Failed to get primary image handle: {:?}", err.code));
    }

    // Decode image to RGB
    let mut image: *mut heif_image = ptr::null_mut();
    let err = heif_decode_image(
        handle,
        &mut image,
        heif_colorspace_heif_colorspace_RGB,
        heif_chroma_heif_chroma_interleaved_RGB,
        ptr::null(),
    );
    if err.code != heif_error_code_heif_error_Ok {
        heif_image_handle_release(handle);
        heif_context_free(ctx);
        return Err(format!("Failed to decode image: {:?}", err.code));
    }

    // Get image dimensions
    let width = heif_image_get_primary_width(image) as u32;
    let height = heif_image_get_primary_height(image) as u32;

    // Get pixel data
    let mut stride: i32 = 0;
    let data = heif_image_get_plane_readonly(
        image,
        heif_channel_heif_channel_interleaved,
        &mut stride,
    );
    if data.is_null() {
        heif_image_release(image);
        heif_image_handle_release(handle);
        heif_context_free(ctx);
        return Err("Failed to get image plane data".to_string());
    }

    // Copy pixel data to Vec
    let stride = stride as usize;
    let mut rgb_data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        let row_start = (y as usize) * stride;
        let row_data = slice::from_raw_parts(data.add(row_start), (width * 3) as usize);
        rgb_data.extend_from_slice(row_data);
    }

    // Cleanup libheif resources
    heif_image_release(image);
    heif_image_handle_release(handle);
    heif_context_free(ctx);

    // Create image buffer and save as JPEG
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, rgb_data).ok_or("Failed to create image buffer")?;

    img.save(output_path)
        .map_err(|e| format!("Failed to save JPEG: {}", e))?;

    Ok(())
}
