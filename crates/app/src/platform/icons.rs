use std::path::Path;

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::SHDefExtractIconW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::HSTRING;

/// Extracts the main icon of an exe at up to `size` pixels.
pub fn exe_icon(path: &Path, size: u32) -> Option<Image> {
    // SAFETY: every GDI object created here is released before returning.
    unsafe {
        let mut icon = HICON::default();
        SHDefExtractIconW(&HSTRING::from(path.as_os_str()), 0, 0, Some(&mut icon), None, size).ok().ok()?;
        if icon.is_invalid() {
            return None;
        }
        let image = icon_to_image(icon);
        let _ = DestroyIcon(icon);
        image
    }
}

unsafe fn icon_to_image(icon: HICON) -> Option<Image> {
    // SAFETY: called with a valid icon; bitmaps from GetIconInfo are deleted.
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let color = info.hbmColor;
        let mask = info.hbmMask;
        let result = if color.is_invalid() { None } else { read_bitmap(color) };
        if !color.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        if !mask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(mask.0));
        }
        let (w, h, mut bgra) = result?;
        let (pixels, _) = bgra.as_chunks_mut::<4>();
        if pixels.iter().all(|p| p[3] == 0) {
            // Icon without an alpha channel: treat as opaque.
            pixels.iter_mut().for_each(|p| p[3] = 255);
        }
        let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
        for (dst, src) in buf.make_mut_slice().iter_mut().zip(pixels.iter()) {
            *dst = Rgba8Pixel { r: src[2], g: src[1], b: src[0], a: src[3] };
        }
        Some(Image::from_rgba8(buf))
    }
}

unsafe fn read_bitmap(bmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    // SAFETY: buffer sized from the bitmap's own dimensions; DC released.
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(HGDIOBJ(bmp.0), size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth.max(0) as u32, bm.bmHeight.max(0) as u32);
        if w == 0 || h == 0 {
            return None;
        }
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(dc, bmp, 0, h, Some(pixels.as_mut_ptr().cast()), &mut bi, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        (lines == h as i32).then_some((w, h, pixels))
    }
}
