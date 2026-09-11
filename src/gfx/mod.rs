pub mod capture;
pub mod device;
pub mod renderer;

/// Read BGRA staging rows, respecting the GPU row pitch, into tightly packed RGBA.
pub fn readback_rgba(gpu: &device::Gpu, staging: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D, w: u32, h: u32) -> windows::core::Result<Vec<u8>> {
    use windows::Win32::Graphics::Direct3D11::*;
    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    unsafe {
        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        gpu.ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut m))?;
        for (y, out) in rgba.chunks_exact_mut(w as usize * 4).enumerate() {
            let row = std::slice::from_raw_parts((m.pData as *const u8).add(y * m.RowPitch as usize), w as usize * 4);
            bgra_row(row, out);
        }
        gpu.ctx.Unmap(staging, 0);
    }
    Ok(rgba)
}
fn bgra_row(row: &[u8], out: &mut [u8]) {
    for (src, dst) in row.chunks_exact(4).zip(out.chunks_exact_mut(4)) { dst.copy_from_slice(&[src[2], src[1], src[0], 255]); }
}
#[cfg(test)] mod tests {
    #[test] fn swizzle() { let mut out = [0;8]; super::bgra_row(&[1,2,3,4,5,6,7,8], &mut out); assert_eq!(out, [3,2,1,255,7,6,5,255]); }
}
