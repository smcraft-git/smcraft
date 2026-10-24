import CMmk
import Metal
import simd

/// Draws the engine's mesh draws (NSMBU / 3D World models and background scenes, mmk.h "meshes")
/// with Metal: GPU skinning from `mmk_mesh_pose`'s matrices, the camera of the scene they belong
/// to (`mmk_engine_scene3d`: the play field's, orthographic or 3D World's slight perspective, or
/// the background scene's), the environment's lights and fogs, and each shape's material look
/// (`MmkMeshShape`: self-lit, fog, alpha test, blending, faces, texture SRT, vertex colours).
/// Mirrors the portable shell's `mesh.rs` / `render.rs`.
///
/// Depth: play-field draws come in sprite order (`MmkMeshDraw.order`); consecutive draws of one
/// layer form a depth group with real depth between them, and every later group sits entirely in
/// front of the earlier ones, so meshes and sprites still stack like painter's order between
/// layers. Sprites never write depth. The background scene keeps its own depth (own pass).
@MainActor
final class MeshRenderer {
    let device: MTLDevice
    private let assets: OpaquePointer
    /// Pipelines by blending (0 opaque, 1 alpha, 2 additive).
    private let pipelines: [MTLRenderPipelineState]
    private let depthWrite: MTLDepthStencilState
    private let depthTest: MTLDepthStencilState
    private var samplers: [UInt32: MTLSamplerState] = [:]
    private lazy var white: MTLTexture? = {
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm_srgb, width: 1, height: 1, mipmapped: false)
        let t = device.makeTexture(descriptor: d)
        var px: [UInt8] = [200, 200, 200, 255]
        t?.replace(region: MTLRegionMake2D(0, 0, 1, 1), mipmapLevel: 0, withBytes: &px, bytesPerRow: 4)
        return t
    }()
    private lazy var noColors: MTLBuffer? = device.makeBuffer(length: 16)
    /// MMK_MESH_DEBUG=1: log what each frame poses.
    static let debug = ProcessInfo.processInfo.environment["MMK_MESH_DEBUG"] == "1"
    /// Texture index (mmk_assets_texture) → GPU texture.
    var texture: (UInt32) -> MTLTexture? = { _ in nil }

    private struct GPUModel {
        let vertices: MTLBuffer
        let indices: MTLBuffer
        let colors: MTLBuffer?
        let bones: Int
        let shapes: [MmkMeshShape]
    }
    private var models: [Int64: GPUModel] = [:]
    private var missing = Set<Int64>()

    /// One posed draw of this frame.
    struct Posed {
        var draw: MmkMeshDraw
        var skinOffset: Int        // bytes into the frame's skin buffer
        var textures: [UInt32]
        var visible: [UInt8]
        var group: Int
    }

    /// Everything one frame's draws need: poses taken, skin matrices in one buffer.
    struct Frame {
        var posed: [Posed] = []
        var skin: MTLBuffer?
        var groups = 0
    }

    /// The camera and environment a set of draws is seen with.
    struct Camera {
        var viewProj: simd_float4x4
        /// Play field: depth squeezed into per-layer slices.
        var layered: Bool
        /// Underground night: drawn upside down.
        var flipY: Bool
        /// `MeshRenderer.lightRowCount` rows (mmk.h MmkScene3d light uniforms).
        var light: [SIMD4<Float>]
    }

    /// Matches `U` in the shader.
    private struct Uniforms {
        var vp: simd_float4x4
        var m0: SIMD4<Float>, m1: SIMD4<Float>, m2: SIMD4<Float>
        var depth: SIMD4<Float>     // group centre, slice, layered, y flip
        var srt: SIMD4<Float>       // scale u, scale v, rotate, translate u
        var misc: SIMD4<Float>      // translate v, vertex-colour ratio, alpha ref, has colours
        var tint: SIMD4<Float>      // albedo tint rgb, emission strength
        var formA: SIMD4<Float> = SIMD4(0, 0, 0, 0)   // combiner add: k rgb, colour.rgb factor
        var formM: SIMD4<Float> = SIMD4(1, 1, 1, 0)   // combiner mul: k rgb, colour.rgb factor
        var formX: SIMD4<Float> = .zero  // add / mul colour.a factors, 1 = clamp add / mul
        var flags: UInt32           // MMK_MESH_* bits (bit 0 = card, bit 30 = clamp the combiner)
        var pad0: UInt32 = 0, pad1: UInt32 = 0, pad2: UInt32 = 0
    }

    private static let shaderSource = """
    #include <metal_stdlib>
    using namespace metal;
    struct V { packed_float3 pos; packed_float3 nrm; float2 uv; ushort4 joints; packed_float4 weights; };
    struct U { float4x4 vp; float4 m0, m1, m2; float4 depth; float4 srt; float4 misc; float4 tint;
               float4 formA, formM, formX; uint flags; uint p0, p1, p2; };
    struct O { float4 pos [[position]]; float2 uv; float3 n; float3 world; float4 color; float3 fadd; float3 fmul;
               float4 vc; };
    vertex O mesh_vertex(const device V* v [[buffer(0)]], const device float4* skin [[buffer(1)]],
                         constant U& u [[buffer(2)]], const device uchar4* colors [[buffer(3)]],
                         uint vid [[vertex_id]]) {
        V x = v[vid];
        float4 P = float4(float3(x.pos), 1);
        float3 N = float3(x.nrm);
        float3 p = 0, n = 0;
        bool any = false;
        for (int k = 0; k < 4; k++) {
            float w = x.weights[k];
            if (w <= 0) continue;
            uint b = uint(x.joints[k]) * 3;
            float4 r0 = skin[b], r1 = skin[b + 1], r2 = skin[b + 2];
            p += w * float3(dot(r0, P), dot(r1, P), dot(r2, P));
            n += w * float3(dot(r0.xyz, N), dot(r1.xyz, N), dot(r2.xyz, N));
            any = true;
        }
        if (!any) { p = P.xyz; n = N; }
        float4 Q = float4(p, 1);
        float3 wp = float3(dot(u.m0, Q), dot(u.m1, Q), dot(u.m2, Q));
        float4 clip = u.vp * float4(wp, 1);
        if (u.depth.z > 0.5 && abs(clip.w) > 1e-6) {
            float zn = clamp(clip.z / clip.w, 0.0, 1.0);
            clip.z = (u.depth.x + (zn - 0.5) * u.depth.y * 0.98) * clip.w;
        }
        clip.y *= u.depth.w;
        O o;
        o.pos = clip;
        // world-mapped textures: (x, -y) / 64 + (0, 1/2) (MMK_MESH_WORLD_UV)
        float2 uv = ((u.flags & (1u << 16)) != 0 ? float2(wp.x, -wp.y) / 64.0 + float2(0, 0.5) : x.uv) * u.srt.xy;
        float s = sin(u.srt.z), c = cos(u.srt.z);
        o.uv = float2(c * uv.x - s * uv.y + u.srt.w, s * uv.x + c * uv.y + u.misc.x);
        o.n = float3(dot(u.m0.xyz, n), dot(u.m1.xyz, n), dot(u.m2.xyz, n));
        o.world = wp;
        float4 vc = u.misc.w > 0.5 ? float4(colors[vid]) / 255.0 : float4(1);
        o.color = float4(mix(float3(1), vc.rgb, u.misc.y) * u.tint.rgb, (u.flags & (1u << 26)) != 0 ? 1.0 : vc.a);
        o.vc = vc;
        // the albedo combiner's terms (MmkMeshShape.form)
        float3 fa = u.formA.xyz + u.formA.w * vc.rgb + u.formX.x * vc.a;
        float3 fm = u.formM.xyz + u.formM.w * vc.rgb + u.formX.y * vc.a;
        o.fadd = u.formX.z > 0.5 ? saturate(fa) : fa;
        o.fmul = u.formX.w > 0.5 ? saturate(fm) : fm;
        return o;
    }
    // the light map (render.rs MESH_SHADER): per light mix(back-side colour, colour,
    // (n.l / 2 + 1/2)^pow); the hemisphere mix(ground, sky, (n.up / 2 + 1/2)^pow)
    float3 lighting(float3 n, constant float4* L) {
        float3 c = L[13].xyz;
        if (L[10].w > 0.5) {
            float k = dot(n, L[12].xyz) * 0.5 + 0.5;
            c += mix(L[11].xyz, L[10].xyz, pow(k, max(L[11].w, 1.0)));
        }
        for (int i = 0; i < 3; i++) {
            float4 d = L[1 + 3 * i];
            if (d.w > 0.5) {
                float k = dot(n, -d.xyz) * 0.5 + 0.5;
                c += mix(L[3 + 3 * i].xyz, L[2 + 3 * i].xyz, pow(k, max(L[2 + 3 * i].w, 1.0)));
            }
        }
        return c;
    }
    // one fog (rows r .. r + 2: start, end, density power, height; colour, strength; direction)
    // applied as calc (0 blended towards its colour, 1 added, 2 multiplied) - render.rs MESH_SHADER
    float3 fog_one(float3 c, float3 world, int r, uint calc, constant float4* L) {
        float4 p = L[r], col = L[r + 1], dir = L[r + 2];
        if (dir.w < 0.5) return c;
        float d = p.w > 0.5 ? -dot(world, dir.xyz) : dot(world - L[0].xyz, dir.xyz);
        float span = p.y - p.x;
        float t = abs(span) > 1e-4 ? clamp((d - p.x) / span, 0.0, 1.0) : 0.0;
        float f = pow(t, p.z) * col.a;
        if (calc == 1) return c + col.rgb * f;
        if (calc == 2) return c * mix(float3(1), col.rgb, f);
        return mix(c, col.rgb, f);
    }
    // the fogs a material switches on, in the uber shader's order: the height fog, the distance
    // fog, the second distance fog, the second height fog
    float3 fogged(float3 c, float3 world, uint flags, constant float4* L) {
        uint calc = flags >> 18;
        float3 r = c;
        if ((flags & (1u << 15)) != 0) r = fog_one(r, world, 20, (calc >> 4) & 3, L);
        if ((flags & 4) != 0) r = fog_one(r, world, 14, calc & 3, L);
        if ((flags & (1u << 14)) != 0) r = fog_one(r, world, 17, (calc >> 2) & 3, L);
        if ((flags & (1u << 17)) != 0) r = fog_one(r, world, 36, (calc >> 6) & 3, L);
        return r;
    }
    // texture matrix k of the sky (rows 30-35: the cloud layer, the horizon layer, the base)
    float2 sky_uv(int k, float2 uv, constant float4* L) {
        float4 a = L[30 + 2 * k], b = L[31 + 2 * k];
        return float2(dot(a.xy, uv) + a.z, dot(b.xy, uv) + b.z);
    }
    // the colour combiner of NSMBU's materials (render.rs MESH_SHADER; layout: mmk_host::mesh::tev_of)
    float2 tev_uv(int k, float2 uv, constant float4* T) {
        float4 a = T[16 + 2 * k], b = T[17 + 2 * k];
        return float2(dot(a.xy, uv) + a.z, dot(b.xy, uv) + b.z);
    }
    struct TevIn { float4 t, s, r0, r1, vc; };
    float3 tev_rgb(float code, TevIn x) {
        switch (int(code + 0.5)) {
            case 1: return float3(1); case 2: return float3(0.5);
            case 3: return x.t.rgb; case 4: return float3(x.t.a);
            case 5: return x.s.rgb; case 6: return float3(x.s.a);
            case 7: return x.r0.rgb; case 8: return float3(x.r0.a);
            case 9: return x.r1.rgb; case 10: return float3(x.r1.a);
            case 11: return x.vc.rgb; case 12: return float3(x.vc.a);
            default: return float3(0);
        }
    }
    float luma(float3 c) { return dot(c, float3(0.299, 0.587, 0.114)); }
    float tev_alpha(float code, TevIn x) {
        switch (int(code + 0.5)) {
            case 1: return 1; case 2: return 0.5;
            case 3: return luma(x.t.rgb); case 4: return x.t.a;
            case 5: return luma(x.s.rgb); case 6: return x.s.a;
            case 7: return luma(x.r0.rgb); case 8: return x.r0.a;
            case 9: return luma(x.r1.rgb); case 10: return x.r1.a;
            case 11: return luma(x.vc.rgb); case 12: return x.vc.a;
            default: return 0;
        }
    }
    // a stage's colour by kind: 0 the albedo, 1 the constant, 2 the vertex colour, 3 / 4 a register
    float4 tev_source(float kind, float4 k, float4 albedo, float4 vc, float4 r0, float4 r1) {
        switch (int(kind + 0.5)) {
            case 1: return k; case 2: return vc; case 3: return r0; case 4: return r1;
            default: return albedo;
        }
    }
    // the combiner's lit colour and alpha; tx: albedo, _a1, gsys_user0, indirect
    float4 tev_color(thread const float4* tx, float4 vc, constant float4* T) {
        float4 r0 = T[8], r1 = T[9];
        float4 on = T[4], kind = T[5], so = T[6], sa = T[7];
        for (int st = 0; st < 2; st++) {
            float4 n0 = r0, n1 = r1;
            int c = st;
            if (on[c] >= 0) {
                TevIn x = { tx[clamp(int(on[c] + 0.5), 0, 3)], tev_source(kind[c], T[10 + c], tx[0], vc, r0, r1), r0, r1, vc };
                float4 q = T[c];
                float3 v = tev_rgb(q.w, x) + mix(tev_rgb(q.x, x), tev_rgb(q.y, x), tev_rgb(q.z, x));
                float3 o = saturate(v * so[2 * c] + so[2 * c + 1]);
                if (st == 0) n0.rgb = o; else n1.rgb = o;
            }
            int a = 2 + st;
            if (on[a] >= 0) {
                TevIn x = { tx[clamp(int(on[a] + 0.5), 0, 3)], tev_source(kind[a], T[10 + a], tx[0], vc, r0, r1), r0, r1, vc };
                float4 q = T[a];
                float v = tev_alpha(q.w, x) + mix(tev_alpha(q.x, x), tev_alpha(q.y, x), tev_alpha(q.z, x));
                float o = saturate(v * sa[2 * st] + sa[2 * st + 1]);
                if (st == 0) n0.a = o; else n1.a = o;
            }
            r0 = n0;
            r1 = n1;
        }
        float4 f = T[14], k = T[15];
        float3 rgb = tev_source(f.x, k, tx[0], vc, r0, r1).rgb;
        float alpha = tev_source(f.y, k, tx[0], vc, r0, r1).a;
        return float4(rgb, alpha * f.w);
    }
    fragment float4 mesh_fragment(O in [[stage_in]], texture2d<float> t [[texture(0)]],
                                  texture2d<float> extra [[texture(1)]],
                                  texture2d<float> extra2 [[texture(2)]],
                                  texture2d<float> extra3 [[texture(3)]],
                                  sampler s [[sampler(0)]], constant U& u [[buffer(0)]],
                                  constant float4* L [[buffer(1)]], constant float4* T [[buffer(2)]]) {
        uint mode = (u.flags >> 12) & 3;
        bool sky = mode == 3 && L[24].w > 0.5;
        if (T[14].z > 0.5) {
            // the colour combiner: each texture through its own matrix, offset by the indirect
            // texture's blue / alpha (scaled) where the material says so
            float4 ind = extra3.sample(s, tev_uv(3, in.uv, T));
            float2 off = ind.ba * float2(T[22].w, T[23].w);
            float4 tx[4] = { t.sample(s, tev_uv(0, in.uv, T) + off * T[16].w),
                             extra2.sample(s, tev_uv(1, in.uv, T) + off * T[18].w),
                             extra.sample(s, tev_uv(2, in.uv, T) + off * T[20].w), ind };
            float4 c = tev_color(tx, in.vc, T) * in.color;
            if ((u.flags & 8) != 0 && c.a < u.misc.z) discard_fragment();
            float3 rgb = c.rgb;
            if ((u.flags & 2) == 0) rgb *= lighting(normalize(in.n), L) * L[23].x;
            rgb = clamp(rgb, 0.0, 2.0);
            rgb = fogged(rgb, in.world, u.flags, L);
            return float4(rgb, c.a);
        }
        float4 tc = t.sample(s, sky ? sky_uv(0, in.uv, L) : in.uv);
        float4 x = extra.sample(s, sky ? sky_uv(2, in.uv, L) : in.uv);
        // the albedo combiner: (albedo + add) * mul, clamped with flag bit 30
        float3 albedo = (tc.rgb + in.fadd) * in.fmul;
        if ((u.flags & (1u << 30)) != 0) albedo = saturate(albedo);
        float4 c = float4(albedo, tc.a) * in.color;
        if (mode == 2) c.a *= x.a;
        if (sky) {
            // the uber shader's sky (render.rs MESH_SHADER): the base (_s0) colours constant
            // colour 5 -> 4 with an HDR boost in its alpha; the horizon layer (_n0) colours 3 -> 2
            // and covers by alpha^5; the clouds (_a0) colour 1 -> 0 and cover by
            // alpha^2 sin(pi (alpha - 1/2))
            float4 h = extra2.sample(s, sky_uv(1, in.uv, L));
            float3 base = mix(L[29].rgb, L[28].rgb, x.rgb) * (1.0 + 1024.0 * pow(x.a * 0.99975, 4.0));
            float3 horizon = mix(L[27].rgb, L[26].rgb, h.rgb);
            float ha = pow(h.a, 5.0);
            float w = (1.0 - ha) * (1.0 - pow(tc.a, 5.0));
            float3 under = mix(base * w, horizon, ha) * in.color.rgb;
            float3 cloud = mix(L[25].rgb, L[24].rgb, tc.rgb);
            c = float4(mix(under, cloud, tc.a * tc.a * sin(M_PI_F * (tc.a - 0.5))), 1);
        } else if (mode == 3) {
            c = float4(mix(x.rgb, c.rgb, c.a) * in.color.rgb, 1);
        }
        // cards (pre-rendered sprites) are cut out by alpha
        if ((u.flags & 1) != 0) {
            if (c.a < 0.1) discard_fragment();
            return c;
        }
        if ((u.flags & 8) != 0 && c.a < u.misc.z) discard_fragment();
        float3 rgb = c.rgb;
        if ((u.flags & 2) == 0) rgb *= lighting(normalize(in.n), L) * L[23].x;
        if (mode == 1) rgb += x.rgb * u.tint.w;
        // the uber shader clamps the lit colour to 0 .. 2 before its fogs
        rgb = clamp(rgb, 0.0, 2.0);
        rgb = fogged(rgb, in.world, u.flags, L);
        return float4(rgb, c.a);
    }
    """

    init(device: MTLDevice, assets: OpaquePointer, colorFormat: MTLPixelFormat, depthFormat: MTLPixelFormat) throws {
        self.device = device
        self.assets = assets
        let lib = try device.makeLibrary(source: Self.shaderSource, options: nil)
        func make(_ blend: Int) throws -> MTLRenderPipelineState {
            let desc = MTLRenderPipelineDescriptor()
            desc.vertexFunction = lib.makeFunction(name: "mesh_vertex")
            desc.fragmentFunction = lib.makeFunction(name: "mesh_fragment")
            desc.colorAttachments[0].pixelFormat = colorFormat
            desc.depthAttachmentPixelFormat = depthFormat
            let ca = desc.colorAttachments[0]!
            ca.isBlendingEnabled = true
            ca.sourceRGBBlendFactor = .sourceAlpha
            ca.destinationRGBBlendFactor = blend == 2 ? .one : .oneMinusSourceAlpha
            ca.sourceAlphaBlendFactor = .one
            ca.destinationAlphaBlendFactor = .oneMinusSourceAlpha
            return try device.makeRenderPipelineState(descriptor: desc)
        }
        pipelines = try [make(0), make(1), make(2)]
        let ds = MTLDepthStencilDescriptor()
        ds.depthCompareFunction = .lessEqual
        ds.isDepthWriteEnabled = true
        depthWrite = device.makeDepthStencilState(descriptor: ds)!
        ds.isDepthWriteEnabled = false
        depthTest = device.makeDepthStencilState(descriptor: ds)!
    }

    /// Depth state for sprites drawn in the same pass: no test, no write.
    static func spriteDepthState(_ device: MTLDevice) -> MTLDepthStencilState? {
        let ds = MTLDepthStencilDescriptor()
        ds.depthCompareFunction = .always
        ds.isDepthWriteEnabled = false
        return device.makeDepthStencilState(descriptor: ds)
    }

    private func model(_ mesh: Int32, _ index: UInt32) -> GPUModel? {
        let key = Int64(mesh) << 32 | Int64(index)
        if let m = models[key] { return m }
        if missing.contains(key) { return nil }
        var info = MmkMeshModel()
        guard mmk_mesh_model(assets, mesh, Int(index), &info), info.vertex_count > 0, info.index_count > 0,
              let v = device.makeBuffer(bytes: info.vertices, length: info.vertex_count * MemoryLayout<MmkMeshVertex>.stride),
              let i = device.makeBuffer(bytes: info.indices, length: info.index_count * 4)
        else {
            missing.insert(key)
            return nil
        }
        let colors = info.colors.flatMap { device.makeBuffer(bytes: $0, length: info.vertex_count * 4) }
        let shapes: [MmkMeshShape] = (0..<Int(info.shape_count)).map { s in
            var sh = MmkMeshShape()
            _ = mmk_mesh_shape(assets, mesh, Int(index), s, &sh)
            return sh
        }
        if Self.debug, let vp = info.vertices, let ip = info.indices {
            let v0 = vp[0]
            FileHandle.standardError.write("model \(mesh)/\(index): \(info.vertex_count) verts, \(info.index_count) indices (first \(ip[0]) \(ip[1]) \(ip[2])), v0 pos \(v0.position) w \(v0.weights) j \(v0.joints), shapes \(shapes.map { ($0.first_index, $0.index_count, $0.flags) })\n".data(using: .utf8)!)
        }
        let m = GPUModel(vertices: v, indices: i, colors: colors, bones: Int(info.bone_count), shapes: shapes)
        models[key] = m
        return m
    }

    /// Pose every draw and gather the skin matrices (call once per frame, before encoding).
    func prepare(_ draws: UnsafeBufferPointer<MmkMeshDraw>) -> Frame {
        var frame = Frame()
        var floats: [Float] = []
        var lastLayer: UInt32?
        for d in draws {
            guard let m = model(d.mesh, d.model) else { continue }
            var pose = MmkMeshPose()
            guard mmk_mesh_pose(assets, d.mesh, Int(d.model), d.anim, d.time, d.variant, &pose),
                  Int(pose.bone_count) == m.bones, let skin = pose.skin
            else { continue }
            if lastLayer != d.layer {
                frame.groups += 1
                lastLayer = d.layer
            }
            let offset = floats.count * 4
            floats.append(contentsOf: UnsafeBufferPointer(start: skin, count: m.bones * 12))
            let n = Int(pose.shape_count)
            frame.posed.append(Posed(
                draw: d, skinOffset: offset,
                textures: Array(UnsafeBufferPointer(start: pose.shape_texture, count: n)),
                visible: Array(UnsafeBufferPointer(start: pose.shape_visible, count: n)),
                group: frame.groups - 1))
        }
        if !floats.isEmpty {
            frame.skin = device.makeBuffer(bytes: floats, length: floats.count * 4)
        }
        if Self.debug {
            FileHandle.standardError.write("meshes: \(draws.count) draws, \(frame.posed.count) posed, \(frame.groups) groups, skin[0..12] \(floats.prefix(12)), textures \(frame.posed.first?.textures ?? []), visible \(frame.posed.first?.visible ?? [])\n".data(using: .utf8)!)
        }
        return frame
    }

    private func sampler(_ flags: UInt32) -> MTLSamplerState? {
        let key = (flags >> 8) & 0xF
        if let s = samplers[key] { return s }
        func mode(_ w: UInt32) -> MTLSamplerAddressMode {
            switch w { case 1: .mirrorRepeat; case 2: .clampToEdge; default: .repeat }
        }
        let d = MTLSamplerDescriptor()
        d.minFilter = .linear
        d.magFilter = .linear
        d.sAddressMode = mode(key & 3)
        d.tAddressMode = mode(key >> 2)
        let s = device.makeSamplerState(descriptor: d)
        samplers[key] = s
        return s
    }

    /// Encode posed draws `range` of `frame` seen with `camera`: opaque shapes first, then the
    /// blended ones.
    func encode(_ enc: MTLRenderCommandEncoder, frame: Frame, range: Range<Int>, camera: Camera) {
        guard let skin = frame.skin, !range.isEmpty else { return }
        enc.setFrontFacing(.counterClockwise)
        var light = camera.light
        let rows = Self.lightRowCount
        if light.count < rows { light += Array(repeating: SIMD4<Float>(0, 0, 0, 0), count: rows - light.count) }
        enc.setFragmentBytes(&light, length: rows * 16, index: 1)
        let slice = 1 / Float(frame.groups + 1)
        for blended in [false, true] {
            for p in frame.posed[range] {
                guard let m = model(p.draw.mesh, p.draw.model) else { continue }
                let t = p.draw.transform
                let center = 1 - Float(p.group + 1) * slice
                // a mirroring transform (or the upside-down course) turns the winding around
                let det = t.0 * (t.5 * t.10 - t.6 * t.9) - t.1 * (t.4 * t.10 - t.6 * t.8) + t.2 * (t.4 * t.9 - t.5 * t.8)
                let mirrored = (det < 0) != camera.flipY
                let card = p.draw.flags & UInt32(MMK_MESH_DRAW_CARD) != 0
                var u = Uniforms(vp: camera.viewProj,
                                 m0: SIMD4(t.0, t.1, t.2, t.3), m1: SIMD4(t.4, t.5, t.6, t.7), m2: SIMD4(t.8, t.9, t.10, t.11),
                                 depth: SIMD4(center, slice, camera.layered ? 1 : 0, camera.flipY ? -1 : 1),
                                 srt: .zero, misc: .zero, tint: .one, flags: 0)
                enc.setVertexBuffer(m.vertices, offset: 0, index: 0)
                enc.setVertexBuffer(skin, offset: p.skinOffset, index: 1)
                enc.setVertexBuffer(m.colors ?? noColors, offset: 0, index: 3)
                for (i, sh) in m.shapes.enumerated() where i < p.visible.count && p.visible[i] != 0 && sh.index_count > 0 {
                    let blend = Int((sh.flags >> UInt32(MMK_MESH_BLEND_SHIFT)) & 3)
                    let faces = (sh.flags >> UInt32(MMK_MESH_FACES_SHIFT)) & 3
                    if (blend != 0) != blended || faces == 3 { continue }
                    let unlit = card || sh.flags & UInt32(MMK_MESH_UNLIT) != 0
                    let extraTex = sh.extra_texture == MMK_TEXTURE_NONE ? nil : texture(sh.extra_texture)
                    let keep = UInt32(MMK_MESH_SELF_LIT | MMK_MESH_FOG | MMK_MESH_ALPHA_TEST) | UInt32(MMK_MESH_FOG1)
                        | UInt32(MMK_MESH_FOG_Y) | UInt32(MMK_MESH_WORLD_UV) | (extraTex != nil ? 3 << 12 : 0)
                        | UInt32(MMK_MESH_FOG1_Y) | (0xFF << UInt32(MMK_MESH_FOG_CALC_SHIFT))
                        | UInt32(MMK_MESH_NO_VERTEX_ALPHA)
                    let f = sh.form
                    u.formA = SIMD4(f.0, f.1, f.2, f.3)
                    u.formM = SIMD4(f.6, f.7, f.8, f.9)
                    u.formX = SIMD4(f.4, f.10, f.5, f.11)
                    u.flags = (unlit ? 1 : 0) | (sh.flags & keep) | (f.12 > 0.5 ? 1 << 30 : 0)
                    u.tint = SIMD4(sh.tint.0, sh.tint.1, sh.tint.2, sh.emission)
                    enc.setFragmentTexture(extraTex ?? white, index: 1)
                    let extra2Tex = sh.extra2_texture == MMK_TEXTURE_NONE ? nil : texture(sh.extra2_texture)
                    enc.setFragmentTexture(extra2Tex ?? white, index: 2)
                    let extra3Tex = sh.extra3_texture == MMK_TEXTURE_NONE ? nil : texture(sh.extra3_texture)
                    enc.setFragmentTexture(extra3Tex ?? white, index: 3)
                    withUnsafeBytes(of: sh.tev) { enc.setFragmentBytes($0.baseAddress!, length: $0.count, index: 2) }
                    u.srt = SIMD4(sh.srt.0, sh.srt.1, sh.srt.2, sh.srt.3)
                    u.misc = SIMD4(sh.srt.4, sh.vertex_color_ratio, sh.alpha_ref, m.colors == nil ? 0 : 1)
                    enc.setRenderPipelineState(pipelines[blend])
                    enc.setDepthStencilState(blend == 0 ? depthWrite : depthTest)
                    if unlit || faces == 2 {
                        enc.setCullMode(.none)
                    } else {
                        enc.setCullMode((faces == 0) != mirrored ? .back : .front)
                    }
                    enc.setVertexBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 2)
                    enc.setFragmentBytes(&u, length: MemoryLayout<Uniforms>.stride, index: 0)
                    let tex = p.textures[i] == MMK_TEXTURE_NONE ? nil : texture(p.textures[i])
                    enc.setFragmentTexture(tex ?? white, index: 0)
                    enc.setFragmentSamplerState(sampler(sh.flags), index: 0)
                    enc.drawIndexedPrimitives(type: .triangle, indexCount: Int(sh.index_count), indexType: .uint32,
                                              indexBuffer: m.indices, indexBufferOffset: Int(sh.first_index) * 4)
                }
            }
        }
        enc.setCullMode(.none)
    }

    /// A plain key light plus ambient (model viewer; scenes without an environment).
    static let neutralLight: [SIMD4<Float>] = {
        var l = Array(repeating: SIMD4<Float>(0, 0, 0, 0), count: lightRowCount)
        l[0] = SIMD4(0, 0, 500, 1)
        l[1] = SIMD4(simd_normalize(SIMD3<Float>(0.35, -0.6, -0.75)), 1)
        l[2] = SIMD4(0.6, 0.6, 0.6, 0)
        l[13] = SIMD4(0.5, 0.5, 0.5, 0)
        l[23] = SIMD4(1, 0, 0, 0)
        return l
    }()

    /// Rows of a light uniform block (mmk_host::scene3d::LIGHT_ROWS).
    static let lightRowCount = 39

    /// The light rows of an `MmkScene3d` uniform block (`lightRowCount` x 4 floats).
    static func lightRows<T>(_ tuple: T) -> [SIMD4<Float>] {
        withUnsafeBytes(of: tuple) { raw in
            let f = raw.bindMemory(to: Float.self)
            return (0..<lightRowCount).map { SIMD4(f[$0 * 4], f[$0 * 4 + 1], f[$0 * 4 + 2], f[$0 * 4 + 3]) }
        }
    }

    /// A column-major 16-float tuple as a matrix.
    static func matrix<T>(_ tuple: T) -> simd_float4x4 {
        withUnsafeBytes(of: tuple) { raw in
            let f = raw.bindMemory(to: Float.self)
            return simd_float4x4(columns: (SIMD4(f[0], f[1], f[2], f[3]), SIMD4(f[4], f[5], f[6], f[7]),
                                           SIMD4(f[8], f[9], f[10], f[11]), SIMD4(f[12], f[13], f[14], f[15])))
        }
    }
}

/// GPU copy of an asset-cache or UI texture (mtex formats are Metal pixel formats). GPUs without
/// BC support (iPhones, iPads before M1) get BCn decoded to RGBA8 by the core
/// (`mmk_texture_decode_rgba`); MMK_CPU_TEXTURES=1 forces that path for testing.
enum GPUTexture {
    static let forceCPU = ProcessInfo.processInfo.environment["MMK_CPU_TEXTURES"] == "1"

    static func make(_ device: MTLDevice, _ t: MmkTextureInfo) -> MTLTexture? {
        guard t.width > 0, t.height > 0 else { return nil }
        if t.format >= 2 && (forceCPU || !device.supportsBCTextureCompression) { return decoded(device, t) }
        let srgb = t.variant == 2
        let (format, blockBytes, blockSize): (MTLPixelFormat, Int, Int) = switch t.format {
        case 1: (srgb ? .rgba8Unorm_srgb : .rgba8Unorm, 4, 1)
        case 2: (srgb ? .bc1_rgba_srgb : .bc1_rgba, 8, 4)
        case 3: (srgb ? .bc2_rgba_srgb : .bc2_rgba, 16, 4)
        case 4: (srgb ? .bc3_rgba_srgb : .bc3_rgba, 16, 4)
        case 5: (t.variant == 1 ? .bc4_rSnorm : .bc4_rUnorm, 8, 4)
        case 6: (t.variant == 1 ? .bc5_rgSnorm : .bc5_rgUnorm, 16, 4)
        case 7: (.bc6H_rgbuFloat, 16, 4)
        case 8: (srgb ? .bc7_rgbaUnorm_srgb : .bc7_rgbaUnorm, 16, 4)
        default: (.invalid, 0, 0)
        }
        guard format != .invalid, let data = t.data else { return nil }
        let w = Int(t.width), h = Int(t.height)
        let desc = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: format, width: w, height: h, mipmapped: false)
        desc.usage = .shaderRead
        desc.swizzle = swizzle(t.channel_select)
        guard let tex = device.makeTexture(descriptor: desc) else { return nil }
        let bytesPerRow = (w + blockSize - 1) / blockSize * blockBytes
        tex.replace(region: MTLRegionMake2D(0, 0, w, h), mipmapLevel: 0, withBytes: data, bytesPerRow: bytesPerRow)
        return tex
    }

    /// BCn decoded on the CPU into an RGBA8 texture (BC4/BC5 decode to linear values).
    private static func decoded(_ device: MTLDevice, _ t: MmkTextureInfo) -> MTLTexture? {
        let w = Int(t.width), h = Int(t.height)
        var px = [UInt8](repeating: 0, count: w * h * 4)
        var info = t
        guard mmk_texture_decode_rgba(&info, &px, px.count) else { return nil }
        let srgb = t.variant == 2 && t.format != 5 && t.format != 6
        let desc = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: srgb ? .rgba8Unorm_srgb : .rgba8Unorm,
                                                            width: w, height: h, mipmapped: false)
        desc.usage = .shaderRead
        desc.swizzle = swizzle(t.channel_select)
        guard let tex = device.makeTexture(descriptor: desc) else { return nil }
        tex.replace(region: MTLRegionMake2D(0, 0, w, h), mipmapLevel: 0, withBytes: px, bytesPerRow: w * 4)
        return tex
    }

    /// Channel select (0 zero, 1 one, 2 R, 3 G, 4 B, 5 A) → Metal swizzle.
    static func swizzle(_ sel: (UInt8, UInt8, UInt8, UInt8)) -> MTLTextureSwizzleChannels {
        func swz(_ c: UInt8) -> MTLTextureSwizzle {
            switch c { case 0: .zero; case 1: .one; case 2: .red; case 3: .green; case 4: .blue; default: .alpha }
        }
        return MTLTextureSwizzleChannels(red: swz(sel.0), green: swz(sel.1), blue: swz(sel.2), alpha: swz(sel.3))
    }
}
