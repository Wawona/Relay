//! Host Vulkan for Wayland wasm clients.
//!
//! The guest still presents with `wl_shm`. When this process exports
//! `vkGetInstanceProcAddr` (MoltenVK inside Wawona, or `WAWONA_VULKAN_LIBRARY`),
//! the guest uploads a chess scene and this module draws it. No symbol means
//! [`probe`] is 0 and the guest keeps the software board. watchOS is that case.
//!
//! Layout matches `chess-linux` `src/board.wgsl`:
//! * vertex: 32 bytes, vec3 position, vec3 normal, vec2 uv
//! * instance: 112 bytes, mat4, tint, params, material
//! * draw: 20 bytes, pass, first vertex, vertex count, first instance, instance count
//! * pass 0 writes the reflection image, pass 1 is the board
//! * uniform: 144-byte camera block

use ash::vk;
use std::ffi::{CStr, CString};
use std::sync::Mutex;

const ENOSYS: i32 = 52;
const EINVAL: i32 = 28;
const EIO: i32 = 29;
const MAX_DIM: u32 = 2048;
/// Guest layer index is a cell in this grid. Matches `atlas_uv` in the wasm shader.
const ATLAS_COLS: u32 = 8;
const ATLAS_ROWS: u32 = 4;
const ATLAS_CELL: u32 = 256;
const DRAW_STRIDE: usize = 20;

#[allow(dead_code)]
struct Gpu {
    entry: ash::Entry,
    instance: ash::Instance,
    physical: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    mem: vk::PhysicalDeviceMemoryProperties,
    cmd_pool: vk::CommandPool,
    samples: vk::SampleCountFlags,
    pipeline: Option<Pipeline>,
    scene: Option<Scene>,
    targets: Option<Targets>,
}

struct Pipeline {
    layout: vk::PipelineLayout,
    desc_layout: vk::DescriptorSetLayout,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    desc_pool: vk::DescriptorPool,
    /// Bound while the reflection image is the color target. Samples a dummy.
    desc_refl: vk::DescriptorSet,
    /// Bound for the board. Samples the reflection image.
    desc_main: vk::DescriptorSet,
    sampler: vk::Sampler,
}

struct Scene {
    vert_buf: vk::Buffer,
    vert_mem: vk::DeviceMemory,
    vert_count: u32,
    inst_buf: vk::Buffer,
    inst_mem: vk::DeviceMemory,
    inst_cap: u64,
    uni_buf: vk::Buffer,
    uni_mem: vk::DeviceMemory,
    tex_image: vk::Image,
    tex_mem: vk::DeviceMemory,
    tex_view: vk::ImageView,
    dummy_image: vk::Image,
    dummy_mem: vk::DeviceMemory,
    dummy_view: vk::ImageView,
}

struct Targets {
    size: [u32; 2],
    refl: Image,
    color: Image,
    resolve: Image,
    depth: Image,
    fb_refl: vk::Framebuffer,
    fb_main: vk::Framebuffer,
    read_buf: vk::Buffer,
    read_mem: vk::DeviceMemory,
    /// Bytes from one row to the next in `read_mem`. Metal wants 256-byte rows.
    read_stride: usize,
}

struct Image {
    image: vk::Image,
    mem: vk::DeviceMemory,
    view: vk::ImageView,
}

struct Draw {
    pass: u32,
    v0: u32,
    vc: u32,
    i0: u32,
    ic: u32,
}

static GPU: Mutex<Option<Gpu>> = Mutex::new(None);
static PROBED: Mutex<Option<bool>> = Mutex::new(None);

fn gpu_lock() -> std::sync::MutexGuard<'static, Option<Gpu>> {
    GPU.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn probe() -> i32 {
    let mut known = PROBED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ok) = *known {
        return if ok { 1 } else { 0 };
    }
    match Gpu::open() {
        Ok(gpu) => {
            *gpu_lock() = Some(gpu);
            *known = Some(true);
            1
        }
        Err(err) => {
            eprintln!("wawona-vk: {err}");
            *known = Some(false);
            0
        }
    }
}

pub fn upload(spirv: &[u8], verts: &[u8], layers: &[Vec<u8>]) -> i32 {
    if spirv.len() < 20
        || verts.len() < 32
        || verts.len() % 32 != 0
        || layers.is_empty()
        || layers.len() > 32
    {
        return EINVAL;
    }
    if layers.iter().any(|l| l.len() != 256 * 256 * 4) {
        return EINVAL;
    }
    let mut slot = gpu_lock();
    let Some(gpu) = slot.as_mut() else {
        return ENOSYS;
    };
    match gpu.upload(spirv, verts, layers) {
        Ok(()) => 0,
        Err(err) => {
            remember("upload", &err);
            EIO
        }
    }
}

pub fn frame(
    instances: &[u8],
    draws: &[u8],
    uniform: &[u8],
    width: u32,
    height: u32,
    out: &mut [u8],
) -> i32 {
    if width == 0 || height == 0 || width > MAX_DIM || height > MAX_DIM || uniform.len() < 144 {
        return EINVAL;
    }
    if draws.len() % DRAW_STRIDE != 0 || instances.len() % 112 != 0 {
        return EINVAL;
    }
    let need = width as usize * height as usize * 4;
    if out.len() < need {
        return EINVAL;
    }
    let mut slot = gpu_lock();
    let Some(gpu) = slot.as_mut() else {
        return ENOSYS;
    };
    match gpu.draw(instances, draws, uniform, width, height, &mut out[..need]) {
        Ok(()) => 0,
        Err(err) => {
            remember("frame", &err);
            EIO
        }
    }
}

/// The guest shell only sees the errno. The simulator process often has no
/// `HOME`, so write every temp path and `syslog` until one sticks.
fn remember(kind: &str, err: &str) {
    eprintln!("wawona-vk {kind}: {err}");
    let line = format!("{kind}: {err}\n");
    let mut paths = vec![
        std::env::temp_dir().join("wawona-vk-last.txt"),
        std::path::PathBuf::from("/tmp/wawona-vk-last.txt"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        paths.push(home.join("Documents/wawona-vk-last.txt"));
        paths.push(home.join("tmp/wawona-vk-last.txt"));
        paths.push(home.join("wawona-vk-last.txt"));
    }
    for path in &paths {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(path, &line).is_ok() {
            break;
        }
    }
    let note = CString::new(format!("wawona-vk {kind}: {err}")).unwrap_or_else(|_| c"wawona-vk".to_owned());
    unsafe {
        libc::syslog(3, c"%s".as_ptr(), note.as_ptr());
    }
}

fn load_entry() -> Result<ash::Entry, String> {
    unsafe {
        let sym = if let Ok(path) = std::env::var("WAWONA_VULKAN_LIBRARY") {
            let c = CString::new(path).map_err(|_| "bad WAWONA_VULKAN_LIBRARY".to_string())?;
            let handle = libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if handle.is_null() {
                return Err("dlopen WAWONA_VULKAN_LIBRARY failed".into());
            }
            libc::dlsym(handle, c"vkGetInstanceProcAddr".as_ptr())
        } else {
            libc::dlsym(libc::RTLD_DEFAULT, c"vkGetInstanceProcAddr".as_ptr())
        };
        if sym.is_null() {
            return Err("vkGetInstanceProcAddr is not in this process".into());
        }
        let get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr = std::mem::transmute(sym);
        Ok(ash::Entry::from_static_fn(ash::StaticFn {
            get_instance_proc_addr,
        }))
    }
}

impl Gpu {
    fn open() -> Result<Self, String> {
        let entry = load_entry()?;
        let offered = unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(|e| e.to_string())?;
        let has = |name: &CStr| {
            offered
                .iter()
                .any(|ext| unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) } == name)
        };
        let mut ext_names = Vec::new();
        let mut flags = vk::InstanceCreateFlags::empty();
        if has(c"VK_KHR_portability_enumeration") {
            ext_names.push(c"VK_KHR_portability_enumeration".as_ptr());
            flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        }
        if has(c"VK_KHR_get_physical_device_properties2") {
            ext_names.push(c"VK_KHR_get_physical_device_properties2".as_ptr());
        }
        let app = vk::ApplicationInfo::default()
            .application_name(c"wawona-vk")
            .api_version(vk::API_VERSION_1_1);
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&ext_names)
            .flags(flags);
        let instance = unsafe { entry.create_instance(&info, None) }.map_err(|e| {
            let names: Vec<String> = offered
                .iter()
                .map(|ext| {
                    unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) }
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            format!("{e}; instance extensions: {names:?}")
        })?;
        let physical = unsafe { instance.enumerate_physical_devices() }
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or("no Vulkan device")?;
        let families = unsafe { instance.get_physical_device_queue_family_properties(physical) };
        let queue_family = families
            .iter()
            .position(|q| q.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .ok_or("no graphics queue")? as u32;
        let offered = unsafe { instance.enumerate_device_extension_properties(physical) }
            .map_err(|e| e.to_string())?;
        let portability = offered.iter().any(|ext| {
            let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
            name == c"VK_KHR_portability_subset"
        });
        let dev_ext: Vec<*const i8> = if portability {
            vec![c"VK_KHR_portability_subset".as_ptr()]
        } else {
            Vec::new()
        };
        let avail = unsafe { instance.get_physical_device_features(physical) };
        let mut features = vk::PhysicalDeviceFeatures::default();
        features.shader_sampled_image_array_dynamic_indexing =
            avail.shader_sampled_image_array_dynamic_indexing;
        let prio = [1.0f32];
        let queue_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&prio);
        let dev_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_info))
            .enabled_extension_names(&dev_ext)
            .enabled_features(&features);
        let device = unsafe { instance.create_device(physical, &dev_info, None) }
            .map_err(|e| e.to_string())?;
        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family);
        let cmd_pool =
            unsafe { device.create_command_pool(&pool_info, None) }.map_err(|e| e.to_string())?;
        // Offscreen readback. 4x MSAA plus a transient color image fails
        // `vkCreateImage` / submit on the iOS simulator MoltenVK. The SHM
        // blit does not need multisample.
        let samples = vk::SampleCountFlags::TYPE_1;
        let mem = unsafe { instance.get_physical_device_memory_properties(physical) };
        Ok(Self {
            entry,
            instance,
            physical,
            device,
            queue,
            mem,
            cmd_pool,
            samples,
            pipeline: None,
            scene: None,
            targets: None,
        })
    }

    fn upload(&mut self, spirv: &[u8], verts: &[u8], layers: &[Vec<u8>]) -> Result<(), String> {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
        self.destroy_targets();
        self.destroy_scene();
        self.destroy_pipeline();
        let pipeline = self.make_pipeline(spirv)?;
        let scene = self.make_scene(verts, layers)?;
        self.pipeline = Some(pipeline);
        self.scene = Some(scene);
        Ok(())
    }

    fn draw(
        &mut self,
        instances: &[u8],
        draws: &[u8],
        uniform: &[u8],
        width: u32,
        height: u32,
        out: &mut [u8],
    ) -> Result<(), String> {
        if self.pipeline.is_none() || self.scene.is_none() {
            return Err("upload before frame".into());
        }
        self.ensure_targets(width, height)
            .map_err(|e| format!("targets {width}x{height}: {e}"))?;
        self.write_bytes(
            self.scene.as_ref().unwrap().inst_buf,
            self.scene.as_ref().unwrap().inst_mem,
            instances,
        )
        .map_err(|e| format!("instances: {e}"))?;
        self.write_uniform(uniform).map_err(|e| format!("uniform: {e}"))?;
        let parsed = parse_draws(draws)?;
        self.record_and_read(&parsed, width, height, out)
            .map_err(|e| format!("record: {e}"))
    }
}

fn parse_draws(bytes: &[u8]) -> Result<Vec<Draw>, String> {
    let mut out = Vec::with_capacity(bytes.len() / DRAW_STRIDE);
    for chunk in bytes.chunks_exact(DRAW_STRIDE) {
        let u = |i| u32::from_le_bytes(chunk[i..i + 4].try_into().unwrap());
        let draw = Draw {
            pass: u(0),
            v0: u(4),
            vc: u(8),
            i0: u(12),
            ic: u(16),
        };
        if draw.pass > 1 {
            return Err("draw pass".into());
        }
        out.push(draw);
    }
    Ok(out)
}

fn memory_type(
    mem: &vk::PhysicalDeviceMemoryProperties,
    bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Result<u32, String> {
    (0..mem.memory_type_count)
        .find(|i| {
            bits & (1 << i) != 0 && mem.memory_types[*i as usize].property_flags.contains(flags)
        })
        .ok_or_else(|| format!("no memory type {flags:?}"))
}

fn buffer(
    device: &ash::Device,
    mem: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
    host: bool,
) -> Result<(vk::Buffer, vk::DeviceMemory), String> {
    let info = vk::BufferCreateInfo::default()
        .size(size.max(4))
        .usage(usage);
    let buf = unsafe { device.create_buffer(&info, None) }.map_err(|e| e.to_string())?;
    let req = unsafe { device.get_buffer_memory_requirements(buf) };
    let flags = if host {
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
    } else {
        vk::MemoryPropertyFlags::DEVICE_LOCAL
    };
    let ty = memory_type(mem, req.memory_type_bits, flags).or_else(|_| {
        memory_type(
            mem,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
    })?;
    let alloc = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(ty);
    let memory = unsafe { device.allocate_memory(&alloc, None) }.map_err(|e| e.to_string())?;
    unsafe { device.bind_buffer_memory(buf, memory, 0) }.map_err(|e| e.to_string())?;
    Ok((buf, memory))
}

fn image(
    device: &ash::Device,
    mem: &vk::PhysicalDeviceMemoryProperties,
    width: u32,
    height: u32,
    layers: u32,
    format: vk::Format,
    samples: vk::SampleCountFlags,
    usage: vk::ImageUsageFlags,
) -> Result<(vk::Image, vk::DeviceMemory), String> {
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(layers)
        .samples(samples)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let img = unsafe { device.create_image(&info, None) }.map_err(|e| e.to_string())?;
    let req = unsafe { device.get_image_memory_requirements(img) };
    // iOS simulator Metal will not blit a private (DEVICE_LOCAL) color
    // attachment into a buffer. Shared memory is the readback path.
    let host_visible = usage.contains(vk::ImageUsageFlags::TRANSFER_SRC)
        && usage.contains(vk::ImageUsageFlags::COLOR_ATTACHMENT);
    let ty = if host_visible {
        memory_type(
            mem,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .or_else(|_| {
            memory_type(
                mem,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
        })
    } else {
        memory_type(
            mem,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
    }
    .or_else(|_| memory_type(mem, req.memory_type_bits, vk::MemoryPropertyFlags::empty()))?;
    let alloc = vk::MemoryAllocateInfo::default()
        .allocation_size(req.size)
        .memory_type_index(ty);
    let memory = unsafe { device.allocate_memory(&alloc, None) }.map_err(|e| e.to_string())?;
    unsafe { device.bind_image_memory(img, memory, 0) }.map_err(|e| e.to_string())?;
    Ok((img, memory))
}

fn view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    layers: u32,
    kind: vk::ImageViewType,
    aspect: vk::ImageAspectFlags,
) -> Result<vk::ImageView, String> {
    let info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(kind)
        .format(format)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: layers,
        });
    unsafe { device.create_image_view(&info, None) }.map_err(|e| e.to_string())
}

impl Gpu {
    fn make_pipeline(&mut self, spirv: &[u8]) -> Result<Pipeline, String> {
        let words = spirv_words(spirv)?;
        let module_info = vk::ShaderModuleCreateInfo::default().code(&words);
        let module = unsafe { self.device.create_shader_module(&module_info, None) }
            .map_err(|e| e.to_string())?;
        let bindings = [
            vk::VertexInputBindingDescription {
                binding: 0,
                stride: 32,
                input_rate: vk::VertexInputRate::VERTEX,
            },
            vk::VertexInputBindingDescription {
                binding: 1,
                stride: 112,
                input_rate: vk::VertexInputRate::INSTANCE,
            },
        ];
        let mut attrs = Vec::new();
        let vert = [
            (0u32, vk::Format::R32G32B32_SFLOAT, 0u32),
            (1, vk::Format::R32G32B32_SFLOAT, 12),
            (2, vk::Format::R32G32_SFLOAT, 24),
        ];
        for (loc, fmt, off) in vert {
            attrs.push(vk::VertexInputAttributeDescription {
                location: loc,
                binding: 0,
                format: fmt,
                offset: off,
            });
        }
        for loc in 3..10 {
            attrs.push(vk::VertexInputAttributeDescription {
                location: loc,
                binding: 1,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: (loc - 3) * 16,
            });
        }
        let vi = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attrs);
        let ia = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let vp = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let ms =
            vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(self.samples);
        let depth = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
        let blend_att = vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA);
        let blend = vk::PipelineColorBlendStateCreateInfo::default()
            .attachments(std::slice::from_ref(&blend_att));
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(module)
                .name(c"vs"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(module)
                .name(c"fs"),
        ];
        let desc_layout = descriptor_layout(&self.device)?;
        let layout_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(std::slice::from_ref(&desc_layout));
        let layout = unsafe { self.device.create_pipeline_layout(&layout_info, None) }
            .map_err(|e| e.to_string())?;
        let render_pass = render_pass(&self.device, self.samples)?;
        let pipe_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vi)
            .input_assembly_state(&ia)
            .viewport_state(&vp)
            .rasterization_state(&raster)
            .multisample_state(&ms)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);
        let pipeline = unsafe {
            self.device.create_graphics_pipelines(
                vk::PipelineCache::null(),
                std::slice::from_ref(&pipe_info),
                None,
            )
        }
        .map_err(|(_, e)| e.to_string())?
        .into_iter()
        .next()
        .ok_or("pipeline")?;
        unsafe { self.device.destroy_shader_module(module, None) };
        let (desc_pool, desc_refl, desc_main) = descriptor_pool(&self.device, desc_layout)?;
        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT);
        let sampler = unsafe { self.device.create_sampler(&sampler_info, None) }
            .map_err(|e| e.to_string())?;
        Ok(Pipeline {
            layout,
            desc_layout,
            render_pass,
            pipeline,
            desc_pool,
            desc_refl,
            desc_main,
            sampler,
        })
    }

    fn make_scene(&mut self, verts: &[u8], layers: &[Vec<u8>]) -> Result<Scene, String> {
        let (vert_buf, vert_mem) = buffer(
            &self.device,
            &self.mem,
            verts.len() as u64,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            true,
        )?;
        write_mem(&self.device, vert_mem, verts)?;
        let inst_cap = (4096 * 112) as u64;
        let (inst_buf, inst_mem) = buffer(
            &self.device,
            &self.mem,
            inst_cap,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            true,
        )?;
        let (uni_buf, uni_mem) = buffer(
            &self.device,
            &self.mem,
            256,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            true,
        )?;
        let (tex_image, tex_mem) = image(
            &self.device,
            &self.mem,
            ATLAS_CELL * ATLAS_COLS,
            ATLAS_CELL * ATLAS_ROWS,
            1,
            vk::Format::R8G8B8A8_SRGB,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )?;
        self.upload_layers(tex_image, layers)?;
        let tex_view = view(
            &self.device,
            tex_image,
            vk::Format::R8G8B8A8_SRGB,
            1,
            vk::ImageViewType::TYPE_2D,
            vk::ImageAspectFlags::COLOR,
        )?;
        let (dummy_image, dummy_mem) = image(
            &self.device,
            &self.mem,
            1,
            1,
            1,
            vk::Format::R8G8B8A8_UNORM,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )?;
        let dummy_view = view(
            &self.device,
            dummy_image,
            vk::Format::R8G8B8A8_UNORM,
            1,
            vk::ImageViewType::TYPE_2D,
            vk::ImageAspectFlags::COLOR,
        )?;
        self.clear_dummy(dummy_image)?;
        Ok(Scene {
            vert_buf,
            vert_mem,
            vert_count: (verts.len() / 32) as u32,
            inst_buf,
            inst_mem,
            inst_cap,
            uni_buf,
            uni_mem,
            tex_image,
            tex_mem,
            tex_view,
            dummy_image,
            dummy_mem,
            dummy_view,
        })
    }

    fn clear_dummy(&self, dummy: vk::Image) -> Result<(), String> {
        let cmd = self.one_shot()?;
        unsafe {
            transition(
                &self.device,
                cmd,
                dummy,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                1,
            );
            self.device.cmd_clear_color_image(
                cmd,
                dummy,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
                &[vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                }],
            );
            transition(
                &self.device,
                cmd,
                dummy,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                1,
            );
        }
        self.submit(cmd)
    }

    fn upload_layers(&self, tex: vk::Image, layers: &[Vec<u8>]) -> Result<(), String> {
        let bytes: usize = layers.iter().map(|l| l.len()).sum();
        let (staging, staging_mem) = buffer(
            &self.device,
            &self.mem,
            bytes as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            true,
        )?;
        let mut packed = Vec::with_capacity(bytes);
        for layer in layers {
            packed.extend_from_slice(layer);
        }
        write_mem(&self.device, staging_mem, &packed)?;
        let cmd = self.one_shot()?;
        unsafe {
            transition(
                &self.device,
                cmd,
                tex,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                1,
            );
            let mut regions = Vec::new();
            for (i, _) in layers.iter().enumerate() {
                let col = (i as u32) % ATLAS_COLS;
                let row = (i as u32) / ATLAS_COLS;
                regions.push(vk::BufferImageCopy {
                    buffer_offset: (i * 256 * 256 * 4) as u64,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D {
                        x: (col * ATLAS_CELL) as i32,
                        y: (row * ATLAS_CELL) as i32,
                        z: 0,
                    },
                    image_extent: vk::Extent3D {
                        width: ATLAS_CELL,
                        height: ATLAS_CELL,
                        depth: 1,
                    },
                });
            }
            self.device.cmd_copy_buffer_to_image(
                cmd,
                staging,
                tex,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &regions,
            );
            transition(
                &self.device,
                cmd,
                tex,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                1,
            );
        }
        self.submit(cmd)?;
        unsafe {
            self.device.destroy_buffer(staging, None);
            self.device.free_memory(staging_mem, None);
        }
        Ok(())
    }

    fn write_uniform(&self, uniform: &[u8]) -> Result<(), String> {
        let scene = self.scene.as_ref().unwrap();
        write_mem(&self.device, scene.uni_mem, &uniform[..144])
    }

    fn write_bytes(
        &mut self,
        buf: vk::Buffer,
        mem: vk::DeviceMemory,
        data: &[u8],
    ) -> Result<(), String> {
        let scene = self.scene.as_ref().unwrap();
        if data.len() as u64 > scene.inst_cap && buf == scene.inst_buf {
            return Err("too many instances".into());
        }
        let _ = buf;
        if data.is_empty() {
            return Ok(());
        }
        write_mem(&self.device, mem, data)
    }

    fn ensure_targets(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self
            .targets
            .as_ref()
            .is_some_and(|t| t.size == [width, height])
        {
            return Ok(());
        }
        self.destroy_targets();
        let samples = self.samples;
        let (refl_i, refl_m) = image(
            &self.device,
            &self.mem,
            width,
            height,
            1,
            vk::Format::R8G8B8A8_UNORM,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC,
        )?;
        let refl_v = view(
            &self.device,
            refl_i,
            vk::Format::R8G8B8A8_UNORM,
            1,
            vk::ImageViewType::TYPE_2D,
            vk::ImageAspectFlags::COLOR,
        )?;
        let (color_i, color_m) = image(
            &self.device,
            &self.mem,
            width,
            height,
            1,
            vk::Format::R8G8B8A8_UNORM,
            samples,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
        )?;
        let color_v = view(
            &self.device,
            color_i,
            vk::Format::R8G8B8A8_UNORM,
            1,
            vk::ImageViewType::TYPE_2D,
            vk::ImageAspectFlags::COLOR,
        )?;
        let (resolve_i, resolve_m) = image(
            &self.device,
            &self.mem,
            width,
            height,
            1,
            vk::Format::R8G8B8A8_UNORM,
            vk::SampleCountFlags::TYPE_1,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
        )?;
        let resolve_v = view(
            &self.device,
            resolve_i,
            vk::Format::R8G8B8A8_UNORM,
            1,
            vk::ImageViewType::TYPE_2D,
            vk::ImageAspectFlags::COLOR,
        )?;
        let (depth_i, depth_m) = image(
            &self.device,
            &self.mem,
            width,
            height,
            1,
            vk::Format::D32_SFLOAT,
            samples,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        )?;
        let depth_v = view(
            &self.device,
            depth_i,
            vk::Format::D32_SFLOAT,
            1,
            vk::ImageViewType::TYPE_2D,
            vk::ImageAspectFlags::DEPTH,
        )?;
        let pass = self.pipeline.as_ref().unwrap().render_pass;
        let (fb_refl, fb_main) = if samples == vk::SampleCountFlags::TYPE_1 {
            (
                framebuffer(&self.device, pass, &[refl_v, depth_v], width, height)?,
                framebuffer(&self.device, pass, &[resolve_v, depth_v], width, height)?,
            )
        } else {
            (
                framebuffer(
                    &self.device,
                    pass,
                    &[color_v, refl_v, depth_v],
                    width,
                    height,
                )?,
                framebuffer(
                    &self.device,
                    pass,
                    &[color_v, resolve_v, depth_v],
                    width,
                    height,
                )?,
            )
        };
        let row_bytes = (width as usize) * 4;
        let read_stride = row_bytes.div_ceil(256) * 256;
        let nbytes = (read_stride * height as usize) as u64;
        let (read_buf, read_mem) = buffer(
            &self.device,
            &self.mem,
            nbytes,
            vk::BufferUsageFlags::TRANSFER_DST,
            true,
        )?;
        self.bind_descriptors(refl_v)?;
        self.targets = Some(Targets {
            size: [width, height],
            refl: Image {
                image: refl_i,
                mem: refl_m,
                view: refl_v,
            },
            color: Image {
                image: color_i,
                mem: color_m,
                view: color_v,
            },
            resolve: Image {
                image: resolve_i,
                mem: resolve_m,
                view: resolve_v,
            },
            depth: Image {
                image: depth_i,
                mem: depth_m,
                view: depth_v,
            },
            fb_refl,
            fb_main,
            read_buf,
            read_mem,
            read_stride,
        });
        Ok(())
    }

    fn bind_descriptors(&self, refl_view: vk::ImageView) -> Result<(), String> {
        let pipe = self.pipeline.as_ref().unwrap();
        let scene = self.scene.as_ref().unwrap();
        let uni_info = vk::DescriptorBufferInfo {
            buffer: scene.uni_buf,
            offset: 0,
            range: 144,
        };
        let tex_info = vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: scene.tex_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        };
        let samp_info = vk::DescriptorImageInfo {
            sampler: pipe.sampler,
            image_view: vk::ImageView::null(),
            image_layout: vk::ImageLayout::UNDEFINED,
        };
        let dummy_info = vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: scene.dummy_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        };
        let refl_info = vk::DescriptorImageInfo {
            sampler: vk::Sampler::null(),
            image_view: refl_view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        };
        let mut writes = Vec::new();
        for (set, refl) in [(pipe.desc_refl, &dummy_info), (pipe.desc_main, &refl_info)] {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(std::slice::from_ref(&uni_info)),
            );
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(&tex_info)),
            );
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(std::slice::from_ref(&samp_info)),
            );
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(std::slice::from_ref(refl)),
            );
        }
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        Ok(())
    }

    fn record_and_read(
        &mut self,
        draws: &[Draw],
        width: u32,
        height: u32,
        out: &mut [u8],
    ) -> Result<(), String> {
        let cmd = self.one_shot()?;
        let pipe = self.pipeline.as_ref().unwrap();
        let scene = self.scene.as_ref().unwrap();
        let targets = self.targets.as_ref().unwrap();
        unsafe {
            let msaa = self.samples != vk::SampleCountFlags::TYPE_1;
            let begin_pass = |fb, clear: [f32; 4]| {
                let clear_color = vk::ClearValue {
                    color: vk::ClearColorValue { float32: clear },
                };
                let clear_depth = vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                };
                let clears = if msaa {
                    vec![clear_color, vk::ClearValue::default(), clear_depth]
                } else {
                    vec![clear_color, clear_depth]
                };
                let info = vk::RenderPassBeginInfo::default()
                    .render_pass(pipe.render_pass)
                    .framebuffer(fb)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: vk::Extent2D { width, height },
                    })
                    .clear_values(&clears);
                self.device
                    .cmd_begin_render_pass(cmd, &info, vk::SubpassContents::INLINE);
            };
            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: width as f32,
                height: height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width, height },
            };
            let draw_list = |pass_id, set: vk::DescriptorSet| {
                self.device
                    .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipe.pipeline);
                self.device.cmd_set_viewport(cmd, 0, &[viewport]);
                self.device.cmd_set_scissor(cmd, 0, &[scissor]);
                self.device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipe.layout,
                    0,
                    &[set],
                    &[],
                );
                self.device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[scene.vert_buf, scene.inst_buf],
                    &[0, 0],
                );
                for d in draws
                    .iter()
                    .filter(|d| d.pass == pass_id && d.vc > 0 && d.ic > 0)
                {
                    if d.v0.saturating_add(d.vc) > scene.vert_count {
                        continue;
                    }
                    // iOS simulator MoltenVK rejects vkCmdDraw firstInstance != 0
                    // (drawVertexBaseInstance is absent). The instance rate starts
                    // at the bound buffer offset instead. 112 is 16-byte aligned.
                    self.device.cmd_bind_vertex_buffers(
                        cmd,
                        1,
                        &[scene.inst_buf],
                        &[d.i0 as u64 * 112],
                    );
                    self.device.cmd_draw(cmd, d.vc, d.ic, d.v0, 0);
                }
            };
            begin_pass(targets.fb_refl, [0.0, 0.0, 0.0, 0.0]);
            draw_list(0, pipe.desc_refl);
            self.device.cmd_end_render_pass(cmd);
            transition(
                &self.device,
                cmd,
                targets.refl.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                vk::AccessFlags::SHADER_READ,
                1,
            );
            begin_pass(
                targets.fb_main,
                [239.0 / 255.0, 235.0 / 255.0, 225.0 / 255.0, 1.0],
            );
            draw_list(1, pipe.desc_main);
            self.device.cmd_end_render_pass(cmd);
            let copy = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: (targets.read_stride / 4) as u32,
                buffer_image_height: height,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D::default(),
                image_extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
            };
            self.device.cmd_copy_image_to_buffer(
                cmd,
                targets.resolve.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                targets.read_buf,
                &[copy],
            );
        }
        if let Err(e) = self.submit(cmd) {
            let why = self.explain_record(draws, width, height);
            return Err(format!("{e} | {why}"));
        }
        let stride = self.targets.as_ref().unwrap().read_stride;
        read_mem_rows(
            &self.device,
            self.targets.as_ref().unwrap().read_mem,
            width,
            height,
            stride,
            out,
        )
    }

    /// Which recorded command the simulator rejects. Runs only after a failed frame.
    fn explain_record(&self, draws: &[Draw], width: u32, height: u32) -> String {
        let max_i0 = draws.iter().map(|d| d.i0).max().unwrap_or(0);
        let mut parts = vec![format!("base0 max_i0={max_i0} n={}", draws.len())];
        let end = |cmd| match unsafe { self.device.end_command_buffer(cmd) } {
            Ok(()) => "ok".to_string(),
            Err(e) => e.to_string(),
        };
        let pipe = self.pipeline.as_ref().unwrap();
        let scene = self.scene.as_ref().unwrap();
        let targets = self.targets.as_ref().unwrap();
        let begin_main = |cmd| {
            let clear_color = vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            };
            let clear_depth = vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            };
            let clears = [clear_color, clear_depth];
            let info = vk::RenderPassBeginInfo::default()
                .render_pass(pipe.render_pass)
                .framebuffer(targets.fb_main)
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D { width, height },
                })
                .clear_values(&clears);
            unsafe {
                self.device
                    .cmd_begin_render_pass(cmd, &info, vk::SubpassContents::INLINE);
            }
        };
        if let Ok(cmd) = self.one_shot() {
            begin_main(cmd);
            unsafe { self.device.cmd_end_render_pass(cmd) };
            parts.push(format!("pass:{}", end(cmd)));
            unsafe { self.device.free_command_buffers(self.cmd_pool, &[cmd]) };
        }
        let first = draws.iter().find(|d| d.vc > 0 && d.ic > 0);
        if let (Ok(cmd), Some(d)) = (self.one_shot(), first) {
            begin_main(cmd);
            unsafe {
                self.device.cmd_bind_pipeline(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipe.pipeline,
                );
                self.device.cmd_bind_vertex_buffers(
                    cmd,
                    0,
                    &[scene.vert_buf, scene.inst_buf],
                    &[0, d.i0 as u64 * 112],
                );
                self.device.cmd_draw(cmd, d.vc, d.ic, d.v0, 0);
                self.device.cmd_end_render_pass(cmd);
            }
            parts.push(format!("draw:{}", end(cmd)));
            unsafe { self.device.free_command_buffers(self.cmd_pool, &[cmd]) };
        }
        if let Ok(cmd) = self.one_shot() {
            let copy = vk::BufferImageCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_offset: vk::Offset3D::default(),
                image_extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
            };
            unsafe {
                self.device.cmd_copy_image_to_buffer(
                    cmd,
                    targets.resolve.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    targets.read_buf,
                    &[copy],
                );
            }
            parts.push(format!("copy:{}", end(cmd)));
            unsafe { self.device.free_command_buffers(self.cmd_pool, &[cmd]) };
        }
        parts.join(" ")
    }

    fn one_shot(&self) -> Result<vk::CommandBuffer, String> {
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.cmd_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let cmd =
            unsafe { self.device.allocate_command_buffers(&info) }.map_err(|e| e.to_string())?[0];
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        unsafe { self.device.begin_command_buffer(cmd, &begin) }.map_err(|e| e.to_string())?;
        Ok(cmd)
    }

    fn submit(&self, cmd: vk::CommandBuffer) -> Result<(), String> {
        unsafe { self.device.end_command_buffer(cmd) }.map_err(|e| format!("end_command_buffer: {e}"))?;
        let info = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));
        unsafe {
            self.device
                .queue_submit(self.queue, &[info], vk::Fence::null())
        }
        .map_err(|e| format!("queue_submit: {e}"))?;
        unsafe { self.device.queue_wait_idle(self.queue) }.map_err(|e| format!("queue_wait_idle: {e}"))?;
        unsafe { self.device.free_command_buffers(self.cmd_pool, &[cmd]) };
        Ok(())
    }

    fn destroy_pipeline(&mut self) {
        let Some(p) = self.pipeline.take() else {
            return;
        };
        unsafe {
            self.device.destroy_pipeline(p.pipeline, None);
            self.device.destroy_pipeline_layout(p.layout, None);
            self.device.destroy_render_pass(p.render_pass, None);
            self.device.destroy_descriptor_pool(p.desc_pool, None);
            self.device
                .destroy_descriptor_set_layout(p.desc_layout, None);
            self.device.destroy_sampler(p.sampler, None);
        }
    }

    fn destroy_scene(&mut self) {
        let Some(s) = self.scene.take() else { return };
        unsafe {
            self.device.destroy_buffer(s.vert_buf, None);
            self.device.free_memory(s.vert_mem, None);
            self.device.destroy_buffer(s.inst_buf, None);
            self.device.free_memory(s.inst_mem, None);
            self.device.destroy_buffer(s.uni_buf, None);
            self.device.free_memory(s.uni_mem, None);
            self.device.destroy_image_view(s.tex_view, None);
            self.device.destroy_image(s.tex_image, None);
            self.device.free_memory(s.tex_mem, None);
            self.device.destroy_image_view(s.dummy_view, None);
            self.device.destroy_image(s.dummy_image, None);
            self.device.free_memory(s.dummy_mem, None);
        }
    }

    fn destroy_targets(&mut self) {
        let Some(t) = self.targets.take() else { return };
        unsafe {
            self.device.destroy_framebuffer(t.fb_refl, None);
            self.device.destroy_framebuffer(t.fb_main, None);
            for img in [t.refl, t.color, t.resolve, t.depth] {
                self.device.destroy_image_view(img.view, None);
                self.device.destroy_image(img.image, None);
                self.device.free_memory(img.mem, None);
            }
            self.device.destroy_buffer(t.read_buf, None);
            self.device.free_memory(t.read_mem, None);
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
        self.destroy_targets();
        self.destroy_scene();
        self.destroy_pipeline();
        unsafe {
            self.device.destroy_command_pool(self.cmd_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
        let _ = &self.entry;
        let _ = &self.physical;
    }
}

fn spirv_words(bytes: &[u8]) -> Result<Vec<u32>, String> {
    if bytes.len() % 4 != 0 {
        return Err("spirv length".into());
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}

fn descriptor_layout(device: &ash::Device) -> Result<vk::DescriptorSetLayout, String> {
    let b = |binding, ty, stages| {
        vk::DescriptorSetLayoutBinding::default()
            .binding(binding)
            .descriptor_type(ty)
            .descriptor_count(1)
            .stage_flags(stages)
    };
    let bindings = [
        b(
            0,
            vk::DescriptorType::UNIFORM_BUFFER,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
        ),
        b(
            1,
            vk::DescriptorType::SAMPLED_IMAGE,
            vk::ShaderStageFlags::FRAGMENT,
        ),
        b(
            2,
            vk::DescriptorType::SAMPLER,
            vk::ShaderStageFlags::FRAGMENT,
        ),
        b(
            3,
            vk::DescriptorType::SAMPLED_IMAGE,
            vk::ShaderStageFlags::FRAGMENT,
        ),
    ];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
    unsafe { device.create_descriptor_set_layout(&info, None) }.map_err(|e| e.to_string())
}

fn descriptor_pool(
    device: &ash::Device,
    layout: vk::DescriptorSetLayout,
) -> Result<(vk::DescriptorPool, vk::DescriptorSet, vk::DescriptorSet), String> {
    let sizes = [
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UNIFORM_BUFFER,
            descriptor_count: 2,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::SAMPLED_IMAGE,
            descriptor_count: 4,
        },
        vk::DescriptorPoolSize {
            ty: vk::DescriptorType::SAMPLER,
            descriptor_count: 2,
        },
    ];
    let info = vk::DescriptorPoolCreateInfo::default()
        .max_sets(2)
        .pool_sizes(&sizes);
    let pool = unsafe { device.create_descriptor_pool(&info, None) }.map_err(|e| e.to_string())?;
    let layouts = [layout, layout];
    let alloc = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    let mut sets = unsafe { device.allocate_descriptor_sets(&alloc) }.map_err(|e| e.to_string())?;
    let main = sets.pop().ok_or("descriptor set")?;
    let refl = sets.pop().ok_or("descriptor set")?;
    Ok((pool, refl, main))
}

fn render_pass(
    device: &ash::Device,
    samples: vk::SampleCountFlags,
) -> Result<vk::RenderPass, String> {
    // One subpass. Reflection and main are separate passes that share this
    // description: color (optional msaa) resolves into attachment 0 when
    // sample count is 1 the color attachment itself is the resolve target.
    let color = vk::AttachmentDescription::default()
        .format(vk::Format::R8G8B8A8_UNORM)
        .samples(if samples == vk::SampleCountFlags::TYPE_1 {
            vk::SampleCountFlags::TYPE_1
        } else {
            samples
        })
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(if samples == vk::SampleCountFlags::TYPE_1 {
            vk::AttachmentStoreOp::STORE
        } else {
            vk::AttachmentStoreOp::DONT_CARE
        })
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(if samples == vk::SampleCountFlags::TYPE_1 {
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL
        } else {
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        });
    let mut attachments = vec![color];
    let mut refs = vec![vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
    }];
    let mut resolve_ref = None;
    if samples != vk::SampleCountFlags::TYPE_1 {
        attachments.push(
            vk::AttachmentDescription::default()
                .format(vk::Format::R8G8B8A8_UNORM)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL),
        );
        resolve_ref = Some(vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        });
        refs[0].attachment = 0;
    }
    let depth_index = attachments.len() as u32;
    attachments.push(
        vk::AttachmentDescription::default()
            .format(vk::Format::D32_SFLOAT)
            .samples(samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
    );
    let depth_ref = vk::AttachmentReference {
        attachment: depth_index,
        layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
    };
    let mut sub = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&refs)
        .depth_stencil_attachment(&depth_ref);
    if let Some(r) = resolve_ref.as_ref() {
        sub = sub.resolve_attachments(std::slice::from_ref(r));
    }
    let deps = [
        vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            )
            .dst_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            ),
        // The copy to the host buffer runs in this command buffer after the
        // pass. Without this edge MoltenVK on iOS fails the submit.
        vk::SubpassDependency::default()
            .src_subpass(0)
            .dst_subpass(vk::SUBPASS_EXTERNAL)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::TRANSFER)
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ),
    ];
    let info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(std::slice::from_ref(&sub))
        .dependencies(&deps);
    unsafe { device.create_render_pass(&info, None) }.map_err(|e| e.to_string())
}

fn framebuffer(
    device: &ash::Device,
    pass: vk::RenderPass,
    views: &[vk::ImageView],
    width: u32,
    height: u32,
) -> Result<vk::Framebuffer, String> {
    let info = vk::FramebufferCreateInfo::default()
        .render_pass(pass)
        .attachments(views)
        .width(width)
        .height(height)
        .layers(1);
    unsafe { device.create_framebuffer(&info, None) }.map_err(|e| e.to_string())
}

fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags,
    dst_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_access: vk::AccessFlags,
    layers: u32,
) {
    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old)
        .new_layout(new)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: layers,
        })
        .src_access_mask(src_access)
        .dst_access_mask(dst_access);
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

fn write_mem(device: &ash::Device, mem: vk::DeviceMemory, data: &[u8]) -> Result<(), String> {
    unsafe {
        let ptr = device
            .map_memory(mem, 0, data.len() as u64, vk::MemoryMapFlags::empty())
            .map_err(|e| e.to_string())?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        device.unmap_memory(mem);
    }
    Ok(())
}

#[cfg(test)]
mod smoke {
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
        TEST.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn moltenvk_draws_a_red_triangle() {
        let _guard = lock();
        let Some(path) = molten() else {
            eprintln!("skip vulkan smoke: set WAWONA_VULKAN_LIBRARY");
            return;
        };
        unsafe { std::env::set_var("WAWONA_VULKAN_LIBRARY", &path) };
        assert_eq!(super::probe(), 1, "MoltenVK probe failed");
        let spirv = tiny_spirv();
        let mut verts = Vec::new();
        for p in [[-1.0f32, -1.0, 0.0], [3.0, -1.0, 0.0], [-1.0, 3.0, 0.0]] {
            for x in p {
                verts.extend(x.to_le_bytes());
            }
            for _ in 0..5 {
                verts.extend(0.0f32.to_le_bytes());
            }
        }
        let layer = vec![255u8; 256 * 256 * 4];
        assert_eq!(super::upload(&spirv, &verts, &[layer]), 0, "upload");
        let mut instance = Vec::new();
        for col in 0..4 {
            for row in 0..4 {
                instance.extend(if row == col { 1.0f32 } else { 0.0 }.to_le_bytes());
            }
        }
        for x in [1.0f32, 0.0, 0.0, 1.0] {
            instance.extend(x.to_le_bytes());
        }
        for _ in 0..8 {
            instance.extend(0.0f32.to_le_bytes());
        }
        let mut draws = Vec::new();
        for x in [1u32, 0, 3, 0, 1] {
            draws.extend(x.to_le_bytes());
        }
        let mut uniform = vec![0u8; 144];
        for col in 0..4 {
            let o = col * 16 + col * 4;
            uniform[o..o + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        uniform[128..132].copy_from_slice(&64.0f32.to_le_bytes());
        uniform[132..136].copy_from_slice(&64.0f32.to_le_bytes());
        let mut out = vec![0u8; 64 * 64 * 4];
        assert_eq!(
            super::frame(&instance, &draws, &uniform, 64, 64, &mut out),
            0,
            "frame"
        );
        let i = (32 * 64 + 32) * 4;
        assert!(out[i] > 200, "red channel {}", out[i]);
        assert!(out[i + 1] < 40, "green channel {}", out[i + 1]);
    }

    #[test]
    fn chess_board_shader_draws_geometry() {
        let _guard = lock();
        let Some(path) = molten() else {
            eprintln!("skip chess shader smoke: set WAWONA_VULKAN_LIBRARY");
            return;
        };
        unsafe { std::env::set_var("WAWONA_VULKAN_LIBRARY", &path) };
        let Ok(root) = std::env::var("CHESS_FOR_LINUX") else {
            eprintln!("skip chess shader smoke: set CHESS_FOR_LINUX");
            return;
        };
        let Some(spirv) = find_spv(&root) else {
            eprintln!("skip chess shader smoke: board.spv not built");
            return;
        };
        let verts = std::fs::read(format!("{root}/assets/pawn.mesh")).expect("pawn mesh");
        let png = std::fs::read(format!("{root}/Styles/Wood/WhiteBoard.png")).expect("wood");
        let img = image::load_from_memory(&png).expect("decode").into_rgba8();
        let img = image::imageops::resize(&img, 256, 256, image::imageops::FilterType::Triangle);
        assert_eq!(super::probe(), 1);
        assert_eq!(
            super::upload(&spirv, &verts, &[img.into_raw()]),
            0,
            "chess upload"
        );
        let mut instance = Vec::new();
        for col in 0..4 {
            for row in 0..4 {
                instance.extend(if row == col { 1.0f32 } else { 0.0 }.to_le_bytes());
            }
        }
        for x in [
            1.0f32, 1.0, 1.0, 1.0, 0.0, 0.45, 0.0, 0.0, 0.68, 0.55, 35.0, 1.0,
        ] {
            instance.extend(x.to_le_bytes());
        }
        let mut draws = Vec::new();
        for x in [1u32, 0, (verts.len() / 32) as u32, 0, 1] {
            draws.extend(x.to_le_bytes());
        }
        let mut uniform = vec![0u8; 144];
        for col in 0..4 {
            uniform[col * 20..col * 20 + 4].copy_from_slice(&1.0f32.to_le_bytes());
        }
        uniform[128..132].copy_from_slice(&128.0f32.to_le_bytes());
        uniform[132..136].copy_from_slice(&128.0f32.to_le_bytes());
        let mut out = vec![0u8; 128 * 128 * 4];
        assert_eq!(
            super::frame(&instance, &draws, &uniform, 128, 128, &mut out),
            0,
            "chess frame"
        );
        let cream = [239u8, 235, 225];
        let changed = out.chunks_exact(4).filter(|p| p[0..3] != cream).count();
        assert!(changed > 20, "pawn pixels {changed}");
    }

    fn find_spv(root: &str) -> Option<Vec<u8>> {
        let build = std::path::Path::new(root).join("wasm/target");
        let mut stack = vec![build];
        while let Some(dir) = stack.pop() {
            let rd = std::fs::read_dir(&dir).ok()?;
            for ent in rd.flatten() {
                let path = ent.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.file_name().is_some_and(|n| n == "board.spv") {
                    return std::fs::read(path).ok();
                }
            }
        }
        None
    }

    fn molten() -> Option<String> {
        let path = std::env::var("WAWONA_VULKAN_LIBRARY").ok()?;
        std::path::Path::new(&path).is_file().then_some(path)
    }

    fn tiny_spirv() -> Vec<u8> {
        let src = r#"
struct Camera { vp: mat4x4<f32>, eye: vec4<f32>, background: vec4<f32>, light: vec4<f32>, lighting: vec4<f32>, viewport: vec4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var surfaces: texture_2d_array<f32>;
@group(0) @binding(2) var surface_sampler: sampler;
@group(0) @binding(3) var reflections: texture_2d<f32>;
struct Input {
  @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>,
  @location(3) m0: vec4<f32>, @location(4) m1: vec4<f32>, @location(5) m2: vec4<f32>, @location(6) m3: vec4<f32>,
  @location(7) tint: vec4<f32>, @location(8) params: vec4<f32>, @location(9) material: vec4<f32>,
}
struct Output { @builtin(position) clip: vec4<f32>, @location(0) tint: vec4<f32> }
@vertex fn vs(v: Input) -> Output {
  let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
  var out: Output;
  out.clip = camera.vp * model * vec4<f32>(v.position, 1.0);
  out.tint = v.tint + v.normal.xyzz * 0.0 + vec4<f32>(v.uv, v.params.xy) * 0.0 + v.material * 0.0;
  return out;
}
@fragment fn fs(v: Output) -> @location(0) vec4<f32> { return v.tint; }
"#;
        let module = naga::front::wgsl::parse_str(src).expect("wgsl");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        let info = validator.validate(&module).expect("validate");
        let words =
            naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)
                .expect("spirv");
        let mut bytes = Vec::new();
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }
}

fn read_mem_rows(
    device: &ash::Device,
    mem: vk::DeviceMemory,
    width: u32,
    height: u32,
    stride: usize,
    out: &mut [u8],
) -> Result<(), String> {
    let tight = width as usize * 4;
    let bytes = stride * height as usize;
    unsafe {
        let ptr = device
            .map_memory(mem, 0, bytes as u64, vk::MemoryMapFlags::empty())
            .map_err(|e| format!("map_memory: {e}"))? as *const u8;
        for y in 0..height as usize {
            std::ptr::copy_nonoverlapping(
                ptr.add(y * stride),
                out.as_mut_ptr().add(y * tight),
                tight,
            );
        }
        device.unmap_memory(mem);
    }
    Ok(())
}
