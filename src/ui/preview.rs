//! Small GPU preview with independent render targets; shares the app's desktop capture.
use crate::{
    app::fold_params,
    config::Config,
    gfx::{capture::Capture, device::Gpu, renderer::Renderer},
};
use windows::core::{Error, Result, HRESULT};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

pub struct Preview {
    renderer: Renderer,
    target: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    width: u32,
    height: u32,
}
impl Preview {
    pub fn new(gpu: &Gpu, cap: &Capture) -> Result<Self> {
        let width = 512;
        let height =
            ((width as f64 * cap.height as f64 / cap.width as f64).round() as u32).clamp(1, 960);
        let desc = |usage, bind, cpu| D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: usage,
            BindFlags: bind,
            CPUAccessFlags: cpu,
            MiscFlags: 0,
        };
        let mut target = None;
        let mut staging = None;
        unsafe {
            gpu.device.CreateTexture2D(
                &desc(D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET.0 as u32, 0),
                None,
                Some(&mut target),
            )?;
            gpu.device.CreateTexture2D(
                &desc(D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ.0 as u32),
                None,
                Some(&mut staging),
            )?;
        }
        Ok(Self {
            renderer: Renderer::new(gpu)?,
            target: target.ok_or_else(failed)?,
            staging: staging.ok_or_else(failed)?,
            width,
            height,
        })
    }
    pub fn render(&mut self, gpu: &Gpu, cap: &Capture, cfg: &Config, t: f32) -> Result<String> {
        let mut params = fold_params(
            &cfg.style_params(),
            t,
            cfg.max_tilt_deg,
            cfg.background_rgba(),
        );
        params.blur_step *= self.width as f32 / cap.width as f32;
        self.renderer.render(
            gpu,
            &cap.srv,
            &self.target,
            self.width,
            self.height,
            &params,
        )?;
        unsafe {
            gpu.ctx.CopyResource(&self.staging, &self.target);
        }
        let rgba = crate::gfx::readback_rgba(gpu, &self.staging, self.width, self.height)?;
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, self.width, self.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            // Sub filtering roughly halves screenshot PNGs for a few extra milliseconds.
            enc.set_filter(png::FilterType::Sub);
            enc.write_header()
                .map_err(|_| failed())?
                .write_image_data(&rgba)
                .map_err(|_| failed())?;
        }
        Ok(format!("data:image/png;base64,{}", base64(&bytes)))
    }
}
fn failed() -> Error {
    Error::from_hresult(HRESULT(0x80004005u32 as i32))
}
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | c.get(2).copied().unwrap_or(0) as u32;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
#[cfg(test)]
mod tests {
    #[test]
    fn rfc4648() {
        for (a, b) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(a.as_bytes()), b);
        }
    }
}
