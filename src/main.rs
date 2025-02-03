use std::sync::{
    Arc,
    RwLock,
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
    prelude::*,
    render::mesh::MeshVertexAttribute,
};
use bevy_egui::{
    egui,
    EguiContexts,
    EguiPlugin,
};
use step::step_file::StepFile;
use tracing::info;

mod config;

#[derive(Debug, thiserror::Error)]
enum StepAssetLoaderError {
    #[error("Io Error")]
    IoError(#[from] std::io::Error),
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
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await?;

        let buffer = StepFile::strip_flatten(&buffer);
        let parsed = StepFile::parse(&buffer);

        let (mesh, _stats) = triangulate::triangulate::triangulate(&parsed);
        Ok(Mesh::new(
            bevy::render::mesh::PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            mesh.verts
                .iter()
                .map(|vertex| vertex.pos.data.0[0].map(|v| v as f32))
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            mesh.verts
                .iter()
                .map(|vertex| vertex.norm.data.0[0].map(|v| v as f32))
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            mesh.verts
                .iter()
                .map(|vertex| {
                    let arr = vertex.color.data.0[0].map(|v| v as f32);
                    [arr[0], arr[1], arr[2], 1.0]
                })
                .collect::<Vec<_>>(),
        )
        .with_inserted_indices(bevy::render::mesh::Indices::U32(
            mesh.triangles
                .iter()
                .flat_map(|tri| tri.verts.iter().copied())
                .collect::<Vec<_>>(),
        )))
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
        .add_plugins(EguiPlugin)
        .register_asset_loader(StepAssetLoader)
        // Systems that create Egui widgets should be run during the `Update` Bevy schedule,
        // or after the `EguiPreUpdateSet::BeginPass` system (which belongs to the `PreUpdate` Bevy
        // schedule).
        .add_systems(Startup, setup)
        .add_systems(Update, ui_example_system)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh: Handle<Mesh> = asset_server.load("test.step");
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: BLUE.into(),
            ..Default::default()
        })),
        Transform::from_scale(Vec3::splat(0.1)),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 7., 14.0).looking_at(Vec3::new(0., 1., 0.), Vec3::Y),
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
