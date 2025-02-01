use std::{
    collections::BTreeMap,
    sync::Arc,
};

use tracing::{
    debug,
    info,
};
use wgpu::{
    util::DeviceExt,
    Backends,
    Instance,
    InstanceDescriptor,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    platform::wayland::WindowAttributesExtWayland,
    window::{
        Window,
        WindowAttributes,
    },
};

mod config;

struct AppSurface {
    textures: BTreeMap<egui::TextureId, wgpu::Texture>,
    egui:     egui::Context,

    egui_render_pipeline: wgpu::RenderPipeline,

    device: wgpu::Device,
    queue:  wgpu::Queue,

    config: wgpu::SurfaceConfiguration,

    // Drop these LAST to prevent segfault :3
    surface: wgpu::Surface<'static>,
    window:  Arc<Window>,
}

const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

impl AppSurface {
    #[allow(clippy::too_many_lines)]
    pub fn new(
        instance: &wgpu::Instance,
        window: Window,
    ) -> Self {
        pollster::block_on(async {
            let window = Arc::new(window);
            let surface = instance.create_surface(window.clone()).expect("shitface");

            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    // TODO: Allow User Config
                    power_preference:       wgpu::PowerPreference::LowPower,
                    force_fallback_adapter: false,
                    compatible_surface:     Some(&surface),
                })
                .await
                .expect("shitface");
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label:             Some("AppSurfaceDevice"),
                        memory_hints:      wgpu::MemoryHints::MemoryUsage,
                        required_features: wgpu::Features::default(),
                        required_limits:   wgpu::Limits::default(), // TODO: WebGL2
                    },
                    None,
                )
                .await
                .expect("shitface");

            // TODO: Handle alternative color spaces
            let surface_capabilities = surface.get_capabilities(&adapter);
            let surface_format = surface_capabilities
                .formats
                .iter()
                .find(|format| format.is_srgb())
                .copied()
                .unwrap_or(surface_capabilities.formats[0]);

            let config = wgpu::SurfaceConfiguration {
                usage:                         wgpu::TextureUsages::RENDER_ATTACHMENT,
                format:                        surface_format,
                width:                         WINDOW_WIDTH,
                height:                        WINDOW_HEIGHT,
                present_mode:                  wgpu::PresentMode::Fifo, // VSync
                alpha_mode:                    surface_capabilities.alpha_modes[0],
                view_formats:                  vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);

            let egui_shader = device.create_shader_module(wgpu::include_wgsl!("egui.wgsl"));
            let egui_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label:                Some("egui_layout"),
                    bind_group_layouts:   &[],
                    push_constant_ranges: &[],
                });
            let egui_render_pipeline =
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label:         Some("egui_render"),
                    layout:        Some(&egui_pipeline_layout),
                    vertex:        wgpu::VertexState {
                        module:              &egui_shader,
                        entry_point:         Some("vs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers:             &[
                            wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<egui::epaint::Vertex>() as wgpu::BufferAddress,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32]
                            }
                        ],
                    },
                    fragment:      Some(wgpu::FragmentState {
                        module:              &egui_shader,
                        entry_point:         Some("fs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets:             &[Some(wgpu::ColorTargetState {
                            format:     config.format,
                            blend:      Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive:     wgpu::PrimitiveState {
                        topology:           wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face:         wgpu::FrontFace::Ccw,
                        cull_mode:          None, // egui gets pissy at BFC
                        polygon_mode:       wgpu::PolygonMode::Fill,
                        unclipped_depth:    false,
                        conservative:       false,
                    },
                    depth_stencil: None,
                    multisample:   wgpu::MultisampleState {
                        count:                     1,
                        mask:                      !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview:     None,
                    cache:         None,
                });

            Self {
                window,
                surface,
                device,
                queue,
                config,

                egui: egui::Context::default(),
                textures: BTreeMap::new(),
                egui_render_pipeline,
            }
        })
    }
}

struct App {
    instance: wgpu::Instance,

    surface: Option<AppSurface>,
}

impl App {
    pub fn new() -> Self {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::PRIMARY,
            ..Default::default()
        });

        Self {
            instance,
            surface: None,
        }
    }

    #[allow(clippy::unused_self)]
    fn render_ui(
        &self,
        ctx: &egui::Context,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Hello world!");
            if ui.button("Click me").clicked() {
                debug!("Debuged");
                // do something meaningful
            }
        });
    }
}

impl ApplicationHandler for App {
    fn resumed(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
        if self.surface.is_none() {
            let window = event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Laplace")
                        .with_name("floating", "floating")
                        .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
                )
                .expect("shitface");
            self.surface = Some(AppSurface::new(&self.instance, window));
        }
    }

    #[allow(clippy::too_many_lines)]
    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(window) = &self.surface else {
            return;
        };

        match event {
            WindowEvent::Resized(size) => {
                let Some(window) = &mut self.surface else {
                    return;
                };

                window.config.width = size.width;
                window.config.height = size.height;
                window.surface.configure(&window.device, &window.config);

                #[allow(clippy::cast_precision_loss)]
                window
                    .egui
                    .send_viewport_cmd(egui::ViewportCommand::InnerSize(
                        [size.width, size.height].map(|v| v as f32).into(),
                    ));
            },
            WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: _,
            } => {
                // I think this is what we need to do?
                #[allow(clippy::cast_possible_truncation)]
                window.egui.set_zoom_factor(scale_factor as f32);
            },
            WindowEvent::CloseRequested => {
                event_loop.exit();
            },
            WindowEvent::RedrawRequested => {
                // Render to screen
                let output = window.surface.get_current_texture().expect("shitface");
                let view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    window
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Render Encoder"),
                        });

                let egui_input = egui::RawInput::default(); // TODO
                let egui_output = window.egui.run(egui_input, |ctx| self.render_ui(ctx));
                // TODO: Handle any copied text or other PlatformOutput

                let egui_prims = window
                    .egui
                    .tessellate(egui_output.shapes, egui_output.pixels_per_point);

                let Some(window) = &mut self.surface else {
                    return;
                };

                // `egui_output.texture_delta.set` contains new+updated texture data
                tracing::debug_span!("egui::TextureDelta").in_scope(|| {
                    for (id, delta) in egui_output.textures_delta.set {
                        debug!(?id, region = ?delta.pos, size = ?delta.image.size());
                        let texture_size = wgpu::Extent3d {
                            width:                 u32::try_from(delta.image.width())
                                .expect("Image too large (width over u32::MAX)"),
                            height:                u32::try_from(delta.image.height())
                                .expect("Image too large (height over u32::MAX)"),
                            depth_or_array_layers: 1,
                        };

                        if let std::collections::btree_map::Entry::Vacant(e) =
                            window.textures.entry(id)
                        {
                            // Create new texture
                            let texture = window.device.create_texture(&wgpu::TextureDescriptor {
                                label:           Some("egui_texture"),
                                size:            texture_size,
                                mip_level_count: 1,
                                sample_count:    1,
                                dimension:       wgpu::TextureDimension::D2,
                                format:          wgpu::TextureFormat::Rgba8UnormSrgb,
                                usage:           wgpu::TextureUsages::COPY_DST
                                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                                view_formats:    &[],
                            });

                            e.insert(texture);
                        }

                        let texture = window.textures.get(&id).expect("Texture doesn't exist");

                        // FIXME: Try do this slightly nicer
                        let image_data = match delta.image {
                            egui::ImageData::Color(color_image) => color_image.pixels.clone(),
                            egui::ImageData::Font(font_image) => {
                                font_image.srgba_pixels(None).collect::<Vec<_>>()
                            },
                        };

                        // Update texture data
                        window.queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture,
                                mip_level: 0,
                                origin: delta.pos.map_or(wgpu::Origin3d::ZERO, |[x, y]| {
                                    wgpu::Origin3d {
                                        x: u32::try_from(x)
                                            .expect("Image too large (width over u32::MAX)"),
                                        y: u32::try_from(y)
                                            .expect("Image too large (height over u32::MAX)"),
                                        z: 0,
                                    }
                                }),
                                aspect: wgpu::TextureAspect::All,
                            },
                            bytemuck::cast_slice(&image_data),
                            wgpu::TexelCopyBufferLayout {
                                offset:         0,
                                bytes_per_row:  Some(4 * texture_size.width),
                                rows_per_image: Some(texture_size.height),
                            },
                            texture_size,
                        );
                    }

                    // idk if we need to do this before a render, or if the GPU will wait for the
                    // textures to copy first
                    window.queue.submit([]);
                });

                // egui Render pass
                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label:                    Some("Render Pass"),
                        color_attachments:        &[Some(wgpu::RenderPassColorAttachment {
                            view:           &view,
                            resolve_target: None,
                            ops:            wgpu::Operations {
                                load:  wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1,
                                    g: 0.2,
                                    b: 0.3,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set:      None,
                        timestamp_writes:         None,
                    });

                    render_pass.set_pipeline(&window.egui_render_pipeline);

                    for egui::ClippedPrimitive {
                        clip_rect: _,
                        primitive,
                    } in egui_prims
                    {
                        match primitive {
                            egui::epaint::Primitive::Callback(_) => unimplemented!(),
                            egui::epaint::Primitive::Mesh(mesh) => {
                                // TODO: See if this can be reused at all
                                let vertex_buffer = window.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label:    Some("egui_vertices"),
                                        usage:    wgpu::BufferUsages::VERTEX,
                                        contents: bytemuck::cast_slice(&mesh.vertices),
                                    },
                                );

                                let index_buffer = window.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label:    Some("egui_indices"),
                                        usage:    wgpu::BufferUsages::INDEX,
                                        contents: bytemuck::cast_slice(&mesh.indices),
                                    },
                                );

                                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                                render_pass.set_index_buffer(
                                    index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint32,
                                );

                                // think this is the right thing
                                /*
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                render_pass.set_scissor_rect(
                                    clip_rect.left() as u32,
                                    clip_rect.top() as u32,
                                    clip_rect.width() as u32,
                                    clip_rect.height() as u32,
                                );
                                */
                                render_pass.draw_indexed(
                                    0..u32::try_from(mesh.indices.len()).expect("Too many indices"),
                                    0,
                                    0..1,
                                );
                            },
                        }
                    }
                }

                // submit will accept anything that implements IntoIter
                window.queue.submit(std::iter::once(encoder.finish()));
                output.present();

                // `egui_output.texture_delta.free` contains anything that can be destroyed now

                window.window.request_redraw();
            },
            _ => {
                // Ignore what we're not using
            },
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fmt_subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(fmt_subscriber)?;

    info!("Hello world!");

    // Ensure config is Loaded + Valid
    config::GLOBAL_CONFIG.write();

    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll); // TODO: Investigate
    event_loop.run_app(&mut App::new())?;

    Ok(())
}
