//! Trees up close, as the game draws them: grown from their SpeedTree parameters
//! (`s3bake::spt`): the trunk and branches in their bark, the fronds and leaf cards from the
//! species' composite map, fitted to the outline of its 360° billboard so the tree up close is
//! the tree its pictures show from afar. Near the camera the grown tree stands in for the
//! billboard (SpeedTree's levels of detail, dithered from one to the other); it sways in the
//! wind, its leaves turn and fall through the seasons, and snow settles on it.

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::primitives::Aabb;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use s3bake::TreeKindBaked;

pub struct SpeedTreesPlugin;

impl Plugin for SpeedTreesPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shaders/tree_part.wgsl");
        embedded_asset!(app, "shaders/tree_part_frag.wgsl");
        embedded_asset!(app, "shaders/tree_part_shadow.wgsl");
        app.add_plugins(MaterialPlugin::<TreePartMaterial>::default()).add_systems(Update, (tree_look, grow_lot_trees));
    }
}

/// Trees bought from the catalogue (on the lots): grown as the town's are, in their crossed
/// pictures' stead, each species once; near enough to be seen grown wherever they're seen from,
/// they cast their own shadows.
#[allow(clippy::too_many_arguments)]
fn grow_lot_trees(
    mut commands: Commands,
    added: Query<(Entity, &crate::interact::GameObject, Option<&Children>), Added<crate::interact::GameObject>>,
    drawn: Query<(), With<Mesh3d>>,
    baked: Option<Res<crate::baked::Baked>>,
    assets: Option<ResMut<crate::objects::ObjectAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut part_mats: ResMut<Assets<TreePartMaterial>>,
    mut cache: Local<std::collections::HashMap<s3bake::Key, Option<Grown>>>,
) {
    let (Some(baked), Some(mut assets)) = (baked, assets) else { return };
    for (e, o, children) in &added {
        let Some(k) = assets.garden_kind(o.objd).filter(|k| k.round && k.speedtree.is_some()).cloned() else { continue };
        let grown = match cache.get(&o.objd) {
            Some(g) => g.clone(),
            None => {
                let mut ctx = crate::objects::AssetCtx { baked: &baked.0, meshes: &mut meshes, images: &mut images, materials: &mut mats };
                let comp = k.composite.and_then(|c| assets.texture(&mut ctx, c));
                let bark = k.bark.and_then(|b| assets.texture(&mut ctx, b));
                let g = grow(&k).zip(comp).map(|(cpu, comp)| ready(cpu, &k, bark, comp, crate::world::evergreen(&k), &mut meshes, &mut part_mats));
                cache.insert(o.objd, g.clone());
                g
            }
        };
        let Some(g) = grown else { continue };
        for c in children.into_iter().flat_map(|c| c.iter()) {
            if drawn.contains(c) {
                commands.entity(c).try_insert(Visibility::Hidden);
            }
        }
        commands.entity(e).with_children(|p| {
            for (mesh, mat, foliage) in &g.parts {
                let mut e = p.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::IDENTITY, g.aabb, GrownPart));
                if *foliage {
                    e.insert(bevy::light::NotShadowReceiver);
                }
            }
        });
    }
}

/// A grown tree's part (for finding them).
#[derive(Component)]
pub struct GrownPart;

/// `TREE_LOOK=<metres>[,<pitch>]` (tests): eight seconds in, the camera on the grown tree nearest
/// where it's looking, from that far off.
fn tree_look(time: Res<Time>, parts: Query<&GlobalTransform, With<GrownPart>>, mut cams: Query<&mut crate::camera::SimsCamera>, mut done: Local<u8>) {
    let Ok(v) = std::env::var("TREE_LOOK") else { return };
    if *done >= 3 || time.elapsed_secs() < 8.0 + *done as f32 {
        return;
    }
    let Ok(mut cam) = cams.single_mut() else { return };
    let mut it = v.split(',').filter_map(|x| x.parse::<f32>().ok());
    let (dist, pitch) = (it.next().unwrap_or(12.0), it.next());
    let at = cam.focus;
    let Some(t) = parts.iter().map(|g| g.translation()).min_by(|a, b| a.distance(at).total_cmp(&b.distance(at))) else { return };
    *done += 1;
    cam.look_at(t + Vec3::Y * dist * 0.3);
    cam.distance = dist;
    if let Some(p) = pitch {
        cam.pitch = p;
    }
    info!("tree look: at {t}");
}

/// A grown tree's bark, fronds or leaves (see `shaders/tree_part.wgsl`).
pub type TreePartMaterial = ExtendedMaterial<StandardMaterial, TreePartExt>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct TreePartExt {
    #[uniform(100)]
    pub part: TreePart,
}

#[derive(ShaderType, Reflect, Debug, Clone, Copy)]
pub struct TreePart {
    /// x: 0 bark, 1 fronds, 2 leaves; y: the tree's height (m); z: its crown's reach (m).
    pub params: Vec4,
    /// x: fall colour, y: leaves gone, z: snow (0..1), w: 1 for an evergreen (`weather`).
    pub season: Vec4,
}

impl MaterialExtension for TreePartExt {
    fn vertex_shader() -> ShaderRef {
        "embedded://sims3/shaders/tree_part.wgsl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "embedded://sims3/shaders/tree_part.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/tree_part_frag.wgsl".into()
    }
    fn prepass_fragment_shader() -> ShaderRef {
        "embedded://sims3/shaders/tree_part_shadow.wgsl".into()
    }
}

/// How much brighter the leaves are drawn than their maps: SpeedTree's leaves are lit as much
/// from the light through them and round them as on them (as bright as the billboards are).
const FOLIAGE_LIGHT: f32 = 1.4;
/// How many leaf cards a tree's grown with (SpeedTree's levels of detail keep about so many).
const LEAF_BUDGET: usize = 450;
/// Out to here (metres from the camera) a tree's drawn grown; beyond, from its pictures.
pub const NEAR: f32 = 70.0;
/// Over how far the one gives way to the other.
pub const BLEND: f32 = 10.0;

/// A species grown, on the CPU: its bark, fronds and leaves, as tall as its billboard (metres),
/// and the room it takes.
pub struct GrownCpu {
    pub bark: Mesh,
    pub fronds: Mesh,
    pub leaves: Mesh,
    pub aabb: Aabb,
    pub reach: f32,
}

fn part_mesh(p: &s3bake::spt::Part, s: f32) -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, p.positions.iter().map(|v| [v[0] * s, v[1] * s, v[2] * s]).collect::<Vec<_>>());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals.clone());
    m.insert_attribute(Mesh::ATTRIBUTE_UV_0, p.uvs.clone());
    m.insert_indices(Indices::U32(p.indices.clone()));
    m
}

/// Grows a species with SpeedTree parameters, fitted to its outline.
pub fn grow(k: &TreeKindBaked) -> Option<GrownCpu> {
    let t = k.speedtree.as_ref()?;
    let mut g = s3bake::spt::grow(t, LEAF_BUDGET);
    if g.leaves.positions.is_empty() && g.fronds.positions.is_empty() {
        return None;
    }
    if !k.envelope.is_empty() {
        s3bake::spt::fit(&mut g, &k.envelope);
    }
    let s = k.height.max(0.3) / g.max[1].max(1e-3);
    let mut leaves = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    let l = &g.leaves;
    leaves.insert_attribute(Mesh::ATTRIBUTE_POSITION, l.positions.iter().map(|v| [v[0] * s, v[1] * s, v[2] * s]).collect::<Vec<_>>());
    leaves.insert_attribute(Mesh::ATTRIBUTE_NORMAL, l.normals.clone());
    leaves.insert_attribute(Mesh::ATTRIBUTE_UV_0, l.uvs.clone());
    leaves.insert_attribute(Mesh::ATTRIBUTE_UV_1, l.corners.iter().map(|c| [c[0] * s, c[1] * s]).collect::<Vec<_>>());
    leaves.insert_indices(Indices::U32(l.indices.clone()));
    let (mn, mx) = (Vec3::from(g.min) * s, Vec3::from(g.max) * s);
    let reach = mn.x.abs().max(mn.z.abs()).max(mx.x.abs()).max(mx.z.abs());
    Some(GrownCpu {
        bark: part_mesh(&g.bark, s),
        fronds: part_mesh(&g.fronds, s),
        leaves,
        // (Swaying, it reaches a little further.)
        aabb: Aabb::from_min_max(mn - Vec3::splat(0.3), mx + Vec3::splat(0.3)),
        reach,
    })
}

/// A species ready to draw: its parts' meshes and materials, and its room.
#[derive(Clone)]
pub struct Grown {
    /// Each part's mesh and material, and whether it's leaves or fronds (not darkened by the
    /// tree's own shadow: SpeedTree's leaves have their shading in them).
    pub parts: Vec<(Handle<Mesh>, Handle<TreePartMaterial>, bool)>,
    pub aabb: Aabb,
}

/// Its meshes and materials made: the bark in its picture, the fronds and leaves cut out of the
/// composite map's (soft at their edges, as the billboards are).
pub fn ready(cpu: GrownCpu, k: &TreeKindBaked, bark: Option<Handle<Image>>, composite: Handle<Image>, evergreen: bool, meshes: &mut Assets<Mesh>, mats: &mut Assets<TreePartMaterial>) -> Grown {
    let season = Vec4::new(0.0, 0.0, 0.0, evergreen as u8 as f32);
    let params = |mode: f32| Vec4::new(mode, k.height.max(0.3), cpu.reach, 0.0);
    // (The leaves as bright as the billboard's, the twigs halfway: `leaf_tint`.)
    let tint = k.leaf_tint.unwrap_or([1.0; 3]).map(|t| t.max(1.0) * FOLIAGE_LIGHT);
    let foliage = |tex: Handle<Image>, power: f32| StandardMaterial {
        base_color_texture: Some(tex),
        base_color: Color::linear_rgb(tint[0].powf(power), tint[1].powf(power), tint[2].powf(power)),
        alpha_mode: AlphaMode::AlphaToCoverage,
        // (Both sides drawn, but lit as they're turned, not flipped: `tree_part.wgsl`.)
        double_sided: false,
        cull_mode: None,
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        // (Leaves let some of the light through.)
        diffuse_transmission: 0.3,
        ..default()
    };
    let mut parts = Vec::new();
    if cpu.bark.count_vertices() > 0 {
        let base = StandardMaterial { base_color_texture: bark, base_color: Color::WHITE, perceptual_roughness: 0.95, reflectance: 0.08, ..default() };
        parts.push((meshes.add(cpu.bark), mats.add(TreePartMaterial { base, extension: TreePartExt { part: TreePart { params: params(0.0), season } } }), false));
    }
    if cpu.fronds.count_vertices() > 0 {
        parts.push((meshes.add(cpu.fronds), mats.add(TreePartMaterial { base: foliage(composite.clone(), 0.5), extension: TreePartExt { part: TreePart { params: params(1.0), season } } }), true));
    }
    if cpu.leaves.count_vertices() > 0 {
        parts.push((meshes.add(cpu.leaves), mats.add(TreePartMaterial { base: foliage(composite, 1.0), extension: TreePartExt { part: TreePart { params: params(2.0), season } } }), true));
    }
    Grown { parts, aabb: cpu.aabb }
}

/// The grown tree's visibility: near the camera, dithering away over the billboard beyond (which
/// comes in where it starts to: `tree.wgsl`'s `near`).
pub fn near_range() -> bevy::camera::visibility::VisibilityRange {
    bevy::camera::visibility::VisibilityRange { start_margin: 0.0..0.0, end_margin: NEAR - BLEND * 0.5..NEAR + BLEND * 0.5, use_aabb: false }
}

