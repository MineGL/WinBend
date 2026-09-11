//! Live monitor capture through Windows.Graphics.Capture, delivered as D3D11 textures.
use windows::core::{Interface, Result};
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::device::Gpu;

pub struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    /// Our own copy of the newest frame (the pool recycles its surfaces).
    pub texture: ID3D11Texture2D,
    pub srv: ID3D11ShaderResourceView,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
}

impl Capture {
    pub fn for_monitor(gpu: &Gpu, hmon: HMONITOR) -> Result<Self> {
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(hmon)? };
        let size = item.Size()?;
        let width = size.Width.max(1) as u32;
        let height = size.Height.max(1) as u32;

        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&gpu.dxgi)? };
        let d3d_device: IDirect3DDevice = inspectable.cast()?;

        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &d3d_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            SizeInt32 { Width: width as i32, Height: height as i32 },
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        // Best effort: no yellow border, no cursor. These need newer Windows builds.
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);

        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut texture))? };
        let texture = texture.unwrap();
        let mut srv = None;
        unsafe { gpu.device.CreateShaderResourceView(&texture, None, Some(&mut srv))? };
        let srv = srv.unwrap();

        session.StartCapture()?;
        Ok(Self { pool, session, texture, srv, width, height, frames: 0 })
    }

    /// Drain pending frames and copy the newest into `texture`. Returns true if updated.
    pub fn poll(&mut self, gpu: &Gpu) -> bool {
        let mut newest = None;
        loop {
            match self.pool.TryGetNextFrame() {
                Ok(f) => newest = Some(f),
                Err(_) => break,
            }
        }
        let Some(frame) = newest else { return false };
        let ok = (|| -> Result<()> {
            let surface = frame.Surface()?;
            let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
            let src: ID3D11Texture2D = unsafe { access.GetInterface()? };
            unsafe { gpu.ctx.CopyResource(&self.texture, &src) };
            Ok(())
        })()
        .is_ok();
        let _ = frame.Close();
        if ok {
            self.frames += 1;
        }
        ok
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
