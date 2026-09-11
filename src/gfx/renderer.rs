//! Direct3D 11 renderer for the fold effect: optional separable blur at quarter
//! resolution, then a bent, perspective-projected panel drawn into a 4x MSAA
//! target and resolved into the destination texture.
use windows::core::{Result, PCSTR};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::device::Gpu;

const SHADER_SRC: &str = include_str!("shaders.hlsl");
const ROWS: u32 = 240; // keep in sync with shaders.hlsl
const FORMAT: DXGI_FORMAT = DXGI_FORMAT_B8G8R8A8_UNORM;

#[derive(Clone, Copy, Debug)]
pub struct FoldParams {
    pub theta: f32,
    pub bend: f32,
    pub cam_dist: f32,
    pub shade: f32,
    pub blur_mix: f32,
    pub blur_step: f32,
    pub vignette: f32,
    pub sheen: f32,
    pub bg: [f32; 4],
    /// 1 = single hinged panel; 2..8 = accordion with that many panels.
    pub panels: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CbParams {
    theta: f32,
    bend: f32,
    cam_dist: f32,
    aspect: f32,
    shade: f32,
    blur_mix: f32,
    vignette: f32,
    sheen: f32,
    texel: [f32; 2],
    blur_dir: [f32; 2],
    bg: [f32; 4],
    panels: f32,
    _pad: [f32; 3],
}

struct RenderTex {
    rtv: ID3D11RenderTargetView,
    srv: ID3D11ShaderResourceView,
}

struct Targets {
    width: u32,
    height: u32,
    msaa_tex: ID3D11Texture2D,
    msaa_rtv: ID3D11RenderTargetView,
    msaa_count: u32,
    blur_a: RenderTex,
    blur_b: RenderTex,
    bw: u32,
    bh: u32,
}

pub struct Renderer {
    vs_fold: ID3D11VertexShader,
    ps_fold: ID3D11PixelShader,
    vs_full: ID3D11VertexShader,
    ps_copy: ID3D11PixelShader,
    ps_blur: ID3D11PixelShader,
    cbuf: ID3D11Buffer,
    sampler: ID3D11SamplerState,
    raster: ID3D11RasterizerState,
    targets: Option<Targets>,
}

fn compile(entry: &str, target: &str) -> Result<ID3DBlob> {
    let entry_c = format!("{entry}\0");
    let target_c = format!("{target}\0");
    let mut code = None;
    let mut errors = None;
    let r = unsafe {
        D3DCompile(
            SHADER_SRC.as_ptr() as _,
            SHADER_SRC.len(),
            PCSTR(b"shaders.hlsl\0".as_ptr()),
            None,
            None,
            PCSTR(entry_c.as_ptr()),
            PCSTR(target_c.as_ptr()),
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = r {
        if let Some(blob) = errors {
            let msg = unsafe {
                std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize())
            };
            eprintln!("shader compile error in {entry}:\n{}", String::from_utf8_lossy(msg));
        }
        return Err(e);
    }
    Ok(code.unwrap())
}

fn blob_bytes(b: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize()) }
}

fn make_render_tex(gpu: &Gpu, w: u32, h: u32, samples: u32) -> Result<(ID3D11Texture2D, ID3D11RenderTargetView, Option<ID3D11ShaderResourceView>)> {
    let mut bind = D3D11_BIND_RENDER_TARGET.0;
    if samples == 1 {
        bind |= D3D11_BIND_SHADER_RESOURCE.0;
    }
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: FORMAT,
        SampleDesc: DXGI_SAMPLE_DESC { Count: samples, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut tex = None;
    unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
    let tex = tex.unwrap();
    let mut rtv = None;
    unsafe { gpu.device.CreateRenderTargetView(&tex, None, Some(&mut rtv))? };
    let srv = if samples == 1 {
        let mut srv = None;
        unsafe { gpu.device.CreateShaderResourceView(&tex, None, Some(&mut srv))? };
        srv
    } else {
        None
    };
    Ok((tex, rtv.unwrap(), srv))
}

impl Renderer {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let vs_fold_b = compile("VS_Fold", "vs_4_0")?;
        let ps_fold_b = compile("PS_Fold", "ps_4_0")?;
        let vs_full_b = compile("VS_Full", "vs_4_0")?;
        let ps_copy_b = compile("PS_Copy", "ps_4_0")?;
        let ps_blur_b = compile("PS_Blur", "ps_4_0")?;

        let dev = &gpu.device;
        let mut vs_fold = None;
        let mut ps_fold = None;
        let mut vs_full = None;
        let mut ps_copy = None;
        let mut ps_blur = None;
        unsafe {
            dev.CreateVertexShader(blob_bytes(&vs_fold_b), None, Some(&mut vs_fold))?;
            dev.CreatePixelShader(blob_bytes(&ps_fold_b), None, Some(&mut ps_fold))?;
            dev.CreateVertexShader(blob_bytes(&vs_full_b), None, Some(&mut vs_full))?;
            dev.CreatePixelShader(blob_bytes(&ps_copy_b), None, Some(&mut ps_copy))?;
            dev.CreatePixelShader(blob_bytes(&ps_blur_b), None, Some(&mut ps_blur))?;
        }

        let cb_desc = D3D11_BUFFER_DESC {
            ByteWidth: std::mem::size_of::<CbParams>() as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let mut cbuf = None;
        unsafe { dev.CreateBuffer(&cb_desc, None, Some(&mut cbuf))? };

        let samp_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
        };
        let mut sampler = None;
        unsafe { dev.CreateSamplerState(&samp_desc, Some(&mut sampler))? };

        let rs_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            FrontCounterClockwise: false.into(),
            DepthBias: 0,
            DepthBiasClamp: 0.0,
            SlopeScaledDepthBias: 0.0,
            DepthClipEnable: false.into(),
            ScissorEnable: false.into(),
            MultisampleEnable: true.into(),
            AntialiasedLineEnable: false.into(),
        };
        let mut raster = None;
        unsafe { dev.CreateRasterizerState(&rs_desc, Some(&mut raster))? };

        Ok(Self {
            vs_fold: vs_fold.unwrap(),
            ps_fold: ps_fold.unwrap(),
            vs_full: vs_full.unwrap(),
            ps_copy: ps_copy.unwrap(),
            ps_blur: ps_blur.unwrap(),
            cbuf: cbuf.unwrap(),
            sampler: sampler.unwrap(),
            raster: raster.unwrap(),
            targets: None,
        })
    }

    fn ensure_targets(&mut self, gpu: &Gpu, width: u32, height: u32) -> Result<()> {
        if let Some(t) = &self.targets {
            if t.width == width && t.height == height {
                return Ok(());
            }
        }
        let mut msaa_count = 4;
        let quality = unsafe { gpu.device.CheckMultisampleQualityLevels(FORMAT, msaa_count) }.unwrap_or(0);
        if quality == 0 {
            msaa_count = 1;
        }
        let (msaa_tex, msaa_rtv, _) = make_render_tex(gpu, width, height, msaa_count)?;
        let bw = (width / 4).max(8);
        let bh = (height / 4).max(8);
        let (_ta, ra, sa) = make_render_tex(gpu, bw, bh, 1)?;
        let (_tb, rb, sb) = make_render_tex(gpu, bw, bh, 1)?;
        self.targets = Some(Targets {
            width,
            height,
            msaa_tex,
            msaa_rtv,
            msaa_count,
            blur_a: RenderTex { rtv: ra, srv: sa.unwrap() },
            blur_b: RenderTex { rtv: rb, srv: sb.unwrap() },
            bw,
            bh,
        });
        Ok(())
    }

    fn upload(&self, gpu: &Gpu, cb: &CbParams) -> Result<()> {
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.ctx.Map(&self.cbuf, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            std::ptr::copy_nonoverlapping(cb as *const CbParams as *const u8, mapped.pData as *mut u8, std::mem::size_of::<CbParams>());
            gpu.ctx.Unmap(&self.cbuf, 0);
        }
        Ok(())
    }

    fn viewport(gpu: &Gpu, w: u32, h: u32) {
        let vp = D3D11_VIEWPORT { TopLeftX: 0.0, TopLeftY: 0.0, Width: w as f32, Height: h as f32, MinDepth: 0.0, MaxDepth: 1.0 };
        unsafe { gpu.ctx.RSSetViewports(Some(&[vp])) };
    }

    /// Render one frame of the fold from `src` into `dst` (which must be `width` x `height`, BGRA8).
    pub fn render(&mut self, gpu: &Gpu, src: &ID3D11ShaderResourceView, dst: &ID3D11Texture2D, width: u32, height: u32, p: &FoldParams) -> Result<()> {
        self.ensure_targets(gpu, width, height)?;
        let t = self.targets.as_ref().unwrap();
        let ctx = &gpu.ctx;
        let aspect = width as f32 / height.max(1) as f32;
        let mut cb = CbParams {
            theta: p.theta,
            bend: p.bend,
            cam_dist: p.cam_dist,
            aspect,
            shade: p.shade,
            blur_mix: p.blur_mix,
            vignette: p.vignette,
            sheen: p.sheen,
            texel: [1.0 / t.bw as f32, 1.0 / t.bh as f32],
            blur_dir: [0.0, 0.0],
            bg: p.bg,
            panels: p.panels.round().clamp(1.0, 8.0),
            _pad: [0.0; 3],
        };

        unsafe {
            ctx.IASetInputLayout(None);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.RSSetState(&self.raster);
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            ctx.VSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.OMSetBlendState(None, None, 0xffff_ffff);
            ctx.OMSetDepthStencilState(None, 0);
        }

        // --- blur chain at quarter resolution ---
        if p.blur_mix > 0.001 {
            unsafe {
                ctx.VSSetShader(&self.vs_full, None);
                Self::viewport(gpu, t.bw, t.bh);
                // downsample
                ctx.OMSetRenderTargets(Some(&[Some(t.blur_a.rtv.clone())]), None);
                ctx.PSSetShader(&self.ps_copy, None);
                ctx.PSSetShaderResources(0, Some(&[Some(src.clone())]));
                self.upload(gpu, &cb)?;
                ctx.Draw(3, 0);
                // two separable passes with increasing step for a wide, smooth blur
                let steps = [1.0f32 * p.blur_step.max(0.5), 2.0 * p.blur_step.max(0.5)];
                for step in steps {
                    // horizontal: A -> B
                    ctx.OMSetRenderTargets(Some(&[Some(t.blur_b.rtv.clone())]), None);
                    ctx.PSSetShader(&self.ps_blur, None);
                    ctx.PSSetShaderResources(0, Some(&[Some(t.blur_a.srv.clone())]));
                    cb.blur_dir = [step, 0.0];
                    self.upload(gpu, &cb)?;
                    ctx.Draw(3, 0);
                    // vertical: B -> A
                    ctx.OMSetRenderTargets(Some(&[Some(t.blur_a.rtv.clone())]), None);
                    ctx.PSSetShaderResources(0, Some(&[Some(t.blur_b.srv.clone())]));
                    cb.blur_dir = [0.0, step];
                    self.upload(gpu, &cb)?;
                    ctx.Draw(3, 0);
                }
                ctx.PSSetShaderResources(0, Some(&[None]));
            }
        }

        // --- fold pass ---
        unsafe {
            ctx.OMSetRenderTargets(Some(&[Some(t.msaa_rtv.clone())]), None);
            ctx.ClearRenderTargetView(&t.msaa_rtv, &p.bg);
            Self::viewport(gpu, width, height);
            ctx.VSSetShader(&self.vs_fold, None);
            ctx.PSSetShader(&self.ps_fold, None);
            ctx.PSSetShaderResources(0, Some(&[Some(src.clone()), Some(t.blur_a.srv.clone())]));
            cb.blur_dir = [0.0, 0.0];
            self.upload(gpu, &cb)?;
            ctx.Draw(ROWS * 6, 0);
            ctx.PSSetShaderResources(0, Some(&[None, None]));
            ctx.OMSetRenderTargets(None, None);
            if t.msaa_count > 1 {
                ctx.ResolveSubresource(dst, 0, &t.msaa_tex, 0, FORMAT);
            } else {
                ctx.CopyResource(dst, &t.msaa_tex);
            }
        }
        Ok(())
    }
}
