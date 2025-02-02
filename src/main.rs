use std::sync::{
    Arc,
    RwLock,
};

use eframe::{
    egui,
    egui_wgpu::{
        self,
        CallbackTrait,
        WgpuConfiguration,
    },
};
use tracing::debug;

mod config;

fn main() -> eframe::Result {
    let fmt_subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(fmt_subscriber)
        .expect("Failed to set tracing subscriber");

    let native_options =
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            wgpu_options: WgpuConfiguration {
                wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew {
                    supported_backends: wgpu::Backends::PRIMARY,
                    power_preference:   config::GLOBAL_CONFIG.render.preferred_gpu,
                    device_descriptor:  Arc::new(|_adapter| {
                        // Kinda copied from eframe src :p
                        wgpu::DeviceDescriptor {
                            label:             Some("laplace_wgpu_device"),
                            required_features: wgpu::Features::default(),
                            required_limits:   wgpu::Limits {
                                max_texture_dimension_2d: 8192,
                                ..wgpu::Limits::default()
                            },
                            memory_hints:      wgpu::MemoryHints::default(),
                        }
                    }),
                },
                ..Default::default()
            },
            window_builder: Some(Box::new(|builder| {
                builder.with_title("Laplace").with_app_id("floating") // Just a smol debug thing for my local setup
            })),
            ..Default::default()
        };

    eframe::run_native(
        "Laplace",
        native_options,
        Box::new(|cc| Ok(Box::new(Laplace::new(cc)))),
    )
}

struct RenderState {
    pipeline: wgpu::RenderPipeline,
}

#[derive(Clone, Copy)]
struct CustomRenderer {}

impl CustomRenderer {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc.wgpu_render_state.as_ref().expect("No WGPU?");

        let device = &render_state.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:                Some("laplace_pipeline_layout"),
            bind_group_layouts:   &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("laplace_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers:             &[],
            },

            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets:             &[Some(wgpu::ColorTargetState {
                    format:     render_state.target_format,
                    blend:      None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),

            primitive:     wgpu::PrimitiveState {
                topology:           wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face:         wgpu::FrontFace::Ccw,
                cull_mode:          Some(wgpu::Face::Back),
                polygon_mode:       wgpu::PolygonMode::Fill,
                unclipped_depth:    false,
                conservative:       false,
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            multiview:     None,
            cache:         None,
        });

        render_state
            .renderer
            .write()
            .callback_resources
            .insert(RenderState { pipeline });
        Self {}
    }
}

impl CallbackTrait for CustomRenderer {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let render_state: Option<&RenderState> = callback_resources.get();
        render_state.map_or(Vec::new(), |render_state| {
            render_state.prepare(device, queue, screen_descriptor, egui_encoder)
        })
    }

    fn finish_prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let render_state: Option<&RenderState> = callback_resources.get();
        render_state.map_or(Vec::new(), |render_state| {
            render_state.finish_prepare(device, queue, egui_encoder)
        })
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let render_state: Option<&RenderState> = callback_resources.get();
        if let Some(state) = render_state {
            state.paint(info, render_pass);
        }
    }
}

impl RenderState {
    #[allow(clippy::unused_self)]
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    #[allow(clippy::unused_self)]
    fn finish_prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _egui_encoder: &mut wgpu::CommandEncoder,
    ) -> Vec<wgpu::CommandBuffer> {
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
    ) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.draw(0..3, 0..1);
    }
}

struct Laplace {
    renderer: CustomRenderer,
}

impl Laplace {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            renderer: CustomRenderer::new(cc),
        }
    }
}

impl eframe::App for Laplace {
    fn update(
        &mut self,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
    ) {
        egui::SidePanel::left("Left Panel").show(ctx, |ui| {
            ui.heading("Hello World!");
            if ui.button("fuck me").clicked() {
                debug!("harder");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (_id, rect) = ui.allocate_space(ui.available_size());
            let callback = egui_wgpu::Callback::new_paint_callback(rect, self.renderer);
            ui.painter().add(callback);
        });
    }
}
