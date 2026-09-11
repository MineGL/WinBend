use windows::core::{Interface, Result};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Foundation::HMODULE;

pub struct Gpu {
    pub device: ID3D11Device,
    pub ctx: ID3D11DeviceContext,
    pub dxgi: IDXGIDevice,
}

impl Gpu {
    pub fn new() -> Result<Self> {
        let mut device = None;
        let mut ctx = None;
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        let levels = [
            D3D_FEATURE_LEVEL_11_1,
            D3D_FEATURE_LEVEL_11_0,
            D3D_FEATURE_LEVEL_10_1,
            D3D_FEATURE_LEVEL_10_0,
        ];
        let mut r = unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                flags,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
        };
        if r.is_err() {
            r = unsafe {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    HMODULE::default(),
                    flags,
                    Some(&levels),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut ctx),
                )
            };
        }
        r?;
        let device = device.unwrap();
        let ctx = ctx.unwrap();
        let dxgi: IDXGIDevice = device.cast()?;
        Ok(Self { device, ctx, dxgi })
    }
}
