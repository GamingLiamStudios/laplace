#![allow(clippy::needless_pass_by_value)]
use std::{
    marker::PhantomPinned,
    ops::Bound,
    path::{
        Path,
        PathBuf,
    },
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
        io::{
            file::FileAssetReader,
            memory::MemoryAssetReader,
            AssetReader,
            AssetSource,
            PathStream,
        },
        AssetLoader,
        AssetPath,
        AsyncReadExt,
        RenderAssetUsages,
    },
    color::palettes::css::{
        BLUE,
        DEEP_PINK,
        GREEN,
        LIME,
        RED,
        WHITE,
    },
    diagnostic::{
        DiagnosticPath,
        DiagnosticsStore,
        FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin,
    },
    ecs::world::CommandQueue,
    gizmos,
    pbr::wireframe::{
        Wireframe,
        WireframeColor,
        WireframeConfig,
        WireframePlugin,
    },
    prelude::{
        Transform,
        *,
    },
    render::{
        settings::{
            Backends,
            WgpuFeatures,
            WgpuSettings,
        },
        RenderPlugin,
    },
    tasks::{
        block_on,
        futures_lite::{
            future,
            StreamExt,
        },
        AsyncComputeTaskPool,
        Task,
    },
};
use bevy_egui::{
    egui::{
        self,
        RichText,
    },
    EguiContexts,
    EguiPlugin,
};
use bevy_panorbit_camera::{
    PanOrbitCamera,
    PanOrbitCameraPlugin,
};
use rayon::prelude::*;
use tracing::info;
use truck_meshalgo::prelude::*;
use truck_modeling::{
    Line,
    Transform as TruckTransform,
    TOLERANCE,
};
use truck_stepio::r#in::{
    ruststep,
    Table,
};

mod config;

#[derive(Debug, thiserror::Error)]
enum StepAssetLoaderError {
    #[error("Io Error")]
    IoError(#[from] std::io::Error),
    #[error("Step Parse Error")]
    StepError(#[from] ruststep::error::Error),
}

#[derive(Asset, TypePath)]
struct StepAsset {
    mesh:      Handle<Mesh>,
    table:     Table,
    polylines: Vec<PolylineCurve<Point3>>,
}

#[derive(Default)]
struct StepAssetLoader;

impl AssetLoader for StepAssetLoader {
    type Asset = StepAsset;
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

        let mut polylines = Vec::new();
        let mesh = table
            .shell
            .values()
            .map(|shell| {
                let shell = table.to_compressed_shell(shell).expect("shitface");
                let bounds = shell.robust_triangulation(0.01).to_polygon().bounding_box();
                let poly = shell.robust_triangulation(bounds.diameter() * 0.001);
                polylines.extend(poly.edges.iter().cloned().map(|edge| edge.curve));
                poly.to_polygon()
            })
            .reduce(|mut acc, e| {
                acc.merge(e);
                acc
            })
            .expect("shitface")
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

        let mesh = load_context.add_labeled_asset("StepAsset_Mesh".to_string(), mesh);
        Ok(StepAsset {
            mesh,
            table,
            polylines,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["step"]
    }
}

#[derive(Component, Default)]
struct LoadingMesh {
    handle: Handle<StepAsset>,
}

#[derive(Resource)]
struct SelectedMesh {
    id:     Entity,
    loaded: Handle<StepAsset>,
}

#[derive(Resource)]
struct FoundFiles {
    paths: Vec<PathBuf>,
}

#[derive(Component)]
struct ComputeTask(Task<CommandQueue>);

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
        .add_plugins((EguiPlugin, PanOrbitCameraPlugin, FrameTimeDiagnosticsPlugin))
        .register_asset_loader(StepAssetLoader)
        .init_asset::<StepAsset>()
        .add_systems(Startup, setup)
        .add_systems(Update, (ui_example_system, render_gizmos))
        .add_systems(Update, (while_mesh_loading, while_compute_task))
        //.add_systems(FixedUpdate, camera_movement)
        .run();
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut ambient_light: ResMut<AmbientLight>,

    mut config_store: ResMut<GizmoConfigStore>,
) {
    let compute_pool = AsyncComputeTaskPool::get();

    let step: Handle<StepAsset> = asset_server.load("test2.step");
    commands.spawn(LoadingMesh { handle: step });

    let compute_entity = commands.spawn_empty().id();
    let task = compute_pool.spawn(async move {
        let paths = AssetSource::get_default_reader("assets".to_string())()
            .read_directory(Path::new("."))
            .await
            .expect("shitface");
        let mut paths_vec = Vec::new();
        paths
            .for_each(|path| {
                let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                    return;
                };
                if ext == "step" {
                    paths_vec.push(path);
                }
            })
            .await;

        let mut queue = CommandQueue::default();
        queue.push(move |world: &mut World| {
            world.insert_resource(FoundFiles { paths: paths_vec });
            world.despawn(compute_entity);
        });

        queue
    });
    commands.entity(compute_entity).insert(ComputeTask(task));

    ambient_light.brightness = 500.0;

    commands.spawn((
        PanOrbitCamera::default(),
        Transform::from_xyz(0.0, 1., 1.0).looking_at(Vec3::new(0., 0., 0.), Vec3::Y),
    ));

    let (config, _) = config_store.config_mut::<DefaultGizmoConfigGroup>();
    //config.line_width = 5.0;
    //config.depth_bias = -1.0;
}

fn render_gizmos(
    mut gizmos: Gizmos,
    loaded: Res<Assets<StepAsset>>,
) {
    for (_, loaded) in loaded.iter() {
        for PolylineCurve(points) in &loaded.polylines {
            gizmos.linestrip(
                points
                    .iter()
                    .map(|v| Vec3::new(v.x as f32, v.y as f32, v.z as f32)),
                WHITE,
            );
        }

        /*
        for shell in loaded.table.shell.values() {
            let shell = loaded.table.to_compressed_shell(shell).expect("shitface");
            for edge in shell.edges {
                match edge.curve {
                    truck_stepio::r#in::step_geometry::Curve3D::Line(Line(start, end)) => {
                        gizmos.line(
                            Vec3::new(start.x as f32, start.y as f32, start.z as f32),
                            Vec3::new(end.x as f32, end.y as f32, end.z as f32),
                            WHITE,
                        );
                    },
                    truck_stepio::r#in::step_geometry::Curve3D::Conic(conic) => {
                        let (start, end) = conic.range_tuple();
                        let interval = Interval::new(start as f32, end as f32).expect("shitface");
                        const STEP: i32 = 200;
                        gizmos.curve_3d(
                            FunctionCurve::new(
                                // Domain should *probably* be the parameter_range, but this works
                                interval,
                                |x| {
                                    let point = conic.subs(x as f64);
                                    Vec3::new(point.x as f32, point.y as f32, point.z as f32)
                                },
                            ),
                            (0..STEP)
                                .map(|i| {
                                    start * ((STEP - i) as f64 / (STEP as f64))
                                        + end * (i as f64 / (STEP as f64))
                                })
                                .map(|v| v as f32),
                            WHITE,
                        );
                    },
                    _ => (),
                }
            }
        }
        */
    }
}

fn while_mesh_loading(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,

    loaded: Res<Assets<StepAsset>>,
    loading: Query<(Entity, &LoadingMesh)>,
) {
    for (entity, loading) in loading.iter() {
        let Some(loaded) = loaded.get(&loading.handle) else {
            continue;
        };

        info!("Asset Loaded");
        let id = commands
            .spawn((
                Mesh3d(loaded.mesh.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: BLUE.into(),
                    ..Default::default()
                })),
                Transform::from_scale(Vec3::splat(1.0)),
            ))
            .id();
        commands.insert_resource(SelectedMesh {
            id,
            loaded: loading.handle.clone(),
        });
        commands.entity(entity).despawn();
    }
}

fn while_compute_task(
    mut commands: Commands,
    mut tasks: Query<&mut ComputeTask>,
) {
    for mut task in &mut tasks {
        if let Some(mut queue) = block_on(future::poll_once(&mut task.0)) {
            commands.append(&mut queue);
        }
    }
}

fn ui_example_system(
    mut contexts: EguiContexts,
    diagnostics: Res<DiagnosticsStore>,
    available: Option<Res<FoundFiles>>,

    mut commands: Commands,
    asset_server: Res<AssetServer>,
    loaded: Option<ResMut<SelectedMesh>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
        .unwrap_or(0.0)
        .round();
    let frame_time = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(bevy::diagnostic::Diagnostic::value)
        .unwrap_or(0.0)
        .round();

    egui::SidePanel::left("Left Panel").show(contexts.ctx_mut(), |ui| {
        ui.heading("Debug info");
        ui.label(format!("FPS: {fps}"));
        ui.label(format!("Frame: {frame_time}ms"));

        ui.separator();
        ui.heading("Hello World!");
        if ui.button("fuck me").clicked() {
            info!("harder");
        }

        let Some(paths) = available else {
            return;
        };

        let original = loaded
            .as_ref()
            .and_then(|loaded| loaded.loaded.path())
            .map(|path| path.path().to_path_buf());
        let mut sel = original.clone();
        egui::ComboBox::new("uh", "woof")
            .selected_text(
                original
                    .as_ref()
                    .map_or("None".to_string(), |path| format!("{path:?}")),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut sel, None, "None");
                for path in &paths.paths {
                    ui.selectable_value(&mut sel, Some(path.clone()), format!("{path:?}"));
                }
            });
        if original != sel {
            // Despawn old
            if let Some(loaded) = loaded {
                commands.entity(loaded.id).despawn();
                commands.remove_resource::<SelectedMesh>();
            }
            // Spawn new
            if let Some(sel) = sel {
                commands.spawn(LoadingMesh {
                    handle: asset_server.load(sel),
                });
            }
        }
    });

    egui::Window::new("Hello").show(contexts.ctx_mut(), |ui| {
        ui.label("world");
    });
}
