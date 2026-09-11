// WinBend shaders. Compiled at runtime with D3DCompile (d3dcompiler_47.dll ships with Windows).

cbuffer Params : register(b0)
{
    float theta;      // tilt angle in radians at the top of the panel (0 = flat)
    float bend;       // 0 = rigid hinge, 1 = the curve spans the whole panel height
    float camDist;    // camera distance in half-heights (smaller = stronger perspective)
    float aspect;     // width / height of the panel
    float shade;      // 0..1 lambert shading strength
    float blurMix;    // 0..1 how much of the blurred copy to use
    float vignette;   // 0..1 extra darkening toward the far (top) edge
    float sheen;      // 0..1 glossy highlight strength
    float2 texel;     // 1 / blur-source size
    float2 blurDir;   // blur direction * step in texels
    float4 bgColor;   // background behind the panel
};

Texture2D    texSharp : register(t0);
Texture2D    texBlur  : register(t1);
SamplerState smpLinear : register(s0);

// ---------------------------------------------------------------------------
// Fold pass: a 2 x ROWS grid panel hinged at the bottom edge, bending away.
// ---------------------------------------------------------------------------
static const uint ROWS = 160;

struct VSOut
{
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float  ndotv : TEXCOORD1;
    float  v     : TEXCOORD2;   // 0 hinge .. 1 far edge
    float3 wpos  : TEXCOORD3;
    float3 nrm   : TEXCOORD4;
};

// Position along the bent profile for arc-length s (0 at hinge), returns (y, z) local
// to the hinge, plus the local surface angle.
void profile(float s, float H, out float y, out float z, out float phi)
{
    float L = max(bend * H, 1e-4);
    if (theta < 1e-5)
    {
        y = s; z = 0; phi = 0; return;
    }
    if (bend < 1e-3)
    {
        y = s * cos(theta); z = -s * sin(theta); phi = theta; return;
    }
    float k = theta / L;               // curvature in the bend region
    if (s <= L)
    {
        y = sin(k * s) / k;
        z = -(1.0 - cos(k * s)) / k;
        phi = k * s;
    }
    else
    {
        float ye = sin(theta) / k;
        float ze = -(1.0 - cos(theta)) / k;
        y = ye + (s - L) * cos(theta);
        z = ze - (s - L) * sin(theta);
        phi = theta;
    }
}

VSOut VS_Fold(uint id : SV_VertexID)
{
    uint strip  = id / 6;
    uint corner = id % 6;
    // two triangles per strip: (0,0) (1,0) (0,1) / (0,1) (1,0) (1,1)
    float u = (corner == 1 || corner == 4 || corner == 5) ? 1.0 : 0.0;
    float dv = (corner == 2 || corner == 3 || corner == 5) ? 1.0 : 0.0;
    float v = (strip + dv) / (float)ROWS;

    const float H = 2.0; // panel height in world units (y from -1 to 1 when flat)
    float y, z, phi;
    profile(v * H, H, y, z, phi);

    float3 p = float3((u * 2.0 - 1.0) * aspect, -1.0 + y, z);
    float3 n = float3(0.0, sin(phi), cos(phi));
    float3 cam = float3(0.0, 0.0, camDist);
    float3 toCam = normalize(cam - p);

    VSOut o;
    float w = camDist - p.z;
    o.pos = float4(p.x * camDist / aspect, p.y * camDist, 0.5 * w, w);
    o.uv = float2(u, 1.0 - v);
    o.ndotv = saturate(dot(n, toCam));
    o.v = v;
    o.wpos = p;
    o.nrm = n;
    return o;
}

float4 PS_Fold(VSOut i) : SV_Target
{
    float3 sharp = texSharp.Sample(smpLinear, i.uv).rgb;
    float3 col = sharp;
    if (blurMix > 0.001)
    {
        float3 blur = texBlur.Sample(smpLinear, i.uv).rgb;
        col = lerp(sharp, blur, blurMix);
    }

    // Lambert-ish shading from the viewer's direction: the panel darkens as it turns away.
    float lam = pow(i.ndotv, 1.6);
    float lit = lerp(1.0, lam, shade);

    // Extra darkening toward the far edge, scaled by how folded we are.
    float fold = saturate(theta / 1.4);
    float vig = 1.0 - vignette * fold * smoothstep(0.0, 1.0, i.v) * 0.9;

    // Soft glossy band that slides along the panel as it bends (the "silk" look).
    float3 lightPos = float3(0.0, 2.5, camDist * 1.5);
    float3 Lv = normalize(lightPos - i.wpos);
    float3 Vv = normalize(float3(0.0, 0.0, camDist) - i.wpos);
    float3 Hv = normalize(Lv + Vv);
    float spec = pow(saturate(dot(normalize(i.nrm), Hv)), 60.0) * sheen * fold * 0.35;

    col = col * lit * vig + spec;
    return float4(col, 1.0);
}

// ---------------------------------------------------------------------------
// Fullscreen passes
// ---------------------------------------------------------------------------
struct FSOut { float4 pos : SV_Position; float2 uv : TEXCOORD0; };

FSOut VS_Full(uint id : SV_VertexID)
{
    FSOut o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.uv = uv;
    o.pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    return o;
}

float4 PS_Copy(FSOut i) : SV_Target
{
    return texSharp.Sample(smpLinear, i.uv);
}

float4 PS_Blur(FSOut i) : SV_Target
{
    // 13-tap gaussian along blurDir (in texels)
    static const float w[7] = { 0.1964, 0.1746, 0.1210, 0.0655, 0.0276, 0.0090, 0.0023 };
    float2 step = blurDir * texel;
    float3 acc = texSharp.Sample(smpLinear, i.uv).rgb * w[0];
    [unroll]
    for (int k = 1; k < 7; ++k)
    {
        acc += texSharp.Sample(smpLinear, i.uv + step * k).rgb * w[k];
        acc += texSharp.Sample(smpLinear, i.uv - step * k).rgb * w[k];
    }
    return float4(acc, 1.0);
}
