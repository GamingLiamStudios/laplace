#![allow(clippy::needless_pass_by_value)]
use std::{
    marker::PhantomPinned,
    pin::Pin,
    ptr::NonNull,
    sync::{
        Arc,
        RwLock,
    },
    time::Instant,
};

use bevy::{
    asset::{
        AssetLoader,
        AsyncReadExt,
        RenderAssetUsages,
    },
    color::palettes::css::{
        BLUE,
        RED,
    },
    input::mouse::MouseMotion,
    prelude::{
        Transform,
        *,
    },
    render::mesh::MeshVertexAttribute,
    window::{
        CursorGrabMode,
        PrimaryWindow,
    },
};
use bevy_egui::{
    egui,
    EguiContexts,
    EguiInputSet,
    EguiPlugin,
    EguiPostUpdateSet,
    EguiPreUpdateSet,
};
use bevy_panorbit_camera::{
    PanOrbitCamera,
    PanOrbitCameraPlugin,
};
use rayon::prelude::*;
use tracing::info;
use truck_meshalgo::prelude::*;
use truck_modeling::TOLERANCE;
use truck_stepio::r#in::ruststep;

mod config;

#[derive(Debug, thiserror::Error)]
enum StepAssetLoaderError {
    #[error("Io Error")]
    IoError(#[from] std::io::Error),
    #[error("Step Parse Error")]
    StepError(#[from] ruststep::error::Error),
}

#[derive(Default)]
struct StepAssetLoader;

impl AssetLoader for StepAssetLoader {
    type Asset = Mesh;
    type Error = StepAssetLoaderError;
    type Settings = ();

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        info!(path = ?load_context.path(), "Processing StepFile");
        let start = Instant::now();

        let mut buffer = String::new();
        reader.read_to_string(&mut buffer).await?;

        let exchange = ruststep::parser::parse(&buffer)?;
        let table = truck_stepio::r#in::Table::from_data_section(&exchange.data[0]);
        let mesh = table
            .shell
            .par_iter()
            .map(|(_idx, shell)| {
                let shell = table.to_compressed_shell(shell).expect("shitface");
                let poly = shell.robust_triangulation(0.01).to_polygon().bounding_box();
                let mut poly = shell
                    .robust_triangulation(poly.diameter() * 0.001)
                    .to_polygon();
                poly.remove_degenerate_faces();
                poly
            })
            .reduce(PolygonMesh::default, |mut acc, e| {
                acc.merge(e);
                acc
            })
            .expands(|attribs| {
                let pos = attribs.position.cast::<f32>().expect("shitface");
                let normal = attribs
                    .normal
                    .expect("shitface")
                    .cast::<f32>()
                    .expect("shitface");
                ([pos.x, pos.y, pos.z], [normal.x, normal.y, normal.z])
            });
        let indices = mesh
            .faces()
            .triangle_iter()
            .flatten()
            .map(|x| u32::try_from(x).expect("shitface"))
            .collect::<Vec<_>>();
        let (vertices, normals): (Vec<_>, Vec<_>) = mesh.attributes().iter().copied().unzip();

        info!(elapsed = ?start.elapsed(), tris = indices.len() / 3,  "Finished processing");
        let mesh = Mesh::new(
            bevy::render::mesh::PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(bevy::render::mesh::Indices::U32(indices));

        Ok(mesh)
    }

    fn extensions(&self) -> &[&str] {
        &["step"]
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                name: Some("floating".to_string()),
                title: "Laplace".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }))
        .add_plugins((EguiPlugin, PanOrbitCameraPlugin))
        .register_asset_loader(StepAssetLoader)
        // Systems that create Egui widgets should be run during the `Update` Bevy schedule,
        // or after the `EguiPreUpdateSet::BeginPass` system (which belongs to the `PreUpdate` Bevy
        // schedule).
        .add_systems(Startup, setup)
        .add_systems(Update, ui_example_system)
        //.add_systems(FixedUpdate, camera_movement)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient_light: ResMut<AmbientLight>,
) {
    let step: Handle<Mesh> = asset_server.load("/home/gls/source/Rust/Laplace/assets/test.step");
    commands.spawn((
        Mesh3d(step),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: BLUE.into(),
            ..Default::default()
        })),
        Transform::from_scale(Vec3::splat(1.0)),
    ));

    ambient_light.brightness = 500.0;

    commands.spawn((
        PanOrbitCamera::default(),
        Transform::from_xyz(0.0, 1., 1.0).looking_at(Vec3::new(0., 0., 0.), Vec3::Y),
    ));
}

fn ui_example_system(mut contexts: EguiContexts) {
    egui::SidePanel::left("Left Panel").show(contexts.ctx_mut(), |ui| {
        ui.heading("Hello World!");
        if ui.button("fuck me").clicked() {
            info!("harder");
        }
    });

    egui::Window::new("Hello").show(contexts.ctx_mut(), |ui| {
        ui.label("world");
    });
}
