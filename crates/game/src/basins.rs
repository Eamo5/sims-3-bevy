//! Sinks set in counters: the counter's top cut away over the sink's bowl, so the basin shows (the
//! game draws a sink's `BasinInterior` mesh through what it's in; here the counter's top is cut
//! to the bowl's outline, from the bake's `basins`). Recut whenever a sink or a counter is put
//! down, moved or sold; a counter with no sink in it any more gets its own top back.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use crate::interact::GameObject;

pub struct BasinsPlugin;

impl Plugin for BasinsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, cut_counters.run_if(in_state(crate::AppState::InGame)));
    }
}

/// A counter mesh's own, uncut, to cut again from (or put back).
#[derive(Component)]
struct UncutMesh(Handle<Mesh>);

/// What a counter's been cut for: the sinks in it, where they stood (centimetres).
#[derive(Component, PartialEq)]
struct CutFor(Vec<(Entity, [i32; 4])>);

/// How far from a counter's middle a sink can be and be in it (metres); how near a counter's
/// top the triangles cut are; how far under it the bowl's points are what's cut for; and how
/// much wider than them the cut is (under the rim's lip: no sliver of counter left inside it).
const IN_COUNTER: f32 = 0.75;
const TOP_NEAR: f32 = 0.02;
const BELOW_TOP: f32 = 0.004;
const GROW: f32 = 0.004;

/// Where some points are, seen from above (their middle).
fn centre(pts: &[Vec3]) -> Vec2 {
    pts.iter().map(|p| p.xz()).sum::<Vec2>() / pts.len().max(1) as f32
}

/// The highest flat top of a mesh over a place (none: no flat top of it there).
fn top_over(mesh: &Mesh, world: &Transform, at: Vec2) -> Option<f32> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let indices: Vec<u32> = match mesh.indices()? {
        Indices::U16(v) => v.iter().map(|i| *i as u32).collect(),
        Indices::U32(v) => v.clone(),
    };
    let mut best: Option<f32> = None;
    for tri in indices.chunks_exact(3) {
        let w = [0, 1, 2].map(|k| world.transform_point(Vec3::from(pos[tri[k] as usize])));
        let n = (w[1] - w[0]).cross(w[2] - w[0]).normalize_or_zero();
        if n.y.abs() < 0.9 {
            continue;
        }
        // (Over the sink's middle: inside the triangle seen from above.)
        let p = [w[0].xz(), w[1].xz(), w[2].xz()];
        let s = [0, 1, 2].map(|k| (p[(k + 1) % 3] - p[k]).perp_dot(at - p[k]));
        if s.iter().all(|v| *v >= 0.0) || s.iter().all(|v| *v <= 0.0) {
            let y = (w[0].y + w[1].y + w[2].y) / 3.0;
            best = Some(best.map_or(y, |b: f32| b.max(y)));
        }
    }
    best
}

/// An outline a little wider all round.
fn grow(hull: &[Vec2], by: f32) -> Vec<Vec2> {
    let c = hull.iter().copied().sum::<Vec2>() / hull.len() as f32;
    hull.iter().map(|p| *p + (*p - c).normalize_or_zero() * by).collect()
}

#[allow(clippy::type_complexity)]
fn cut_counters(
    mut commands: Commands,
    changed: Query<&GameObject, Or<(Changed<Transform>, Added<GameObject>)>>,
    mut removed: RemovedComponents<GameObject>,
    objects: Query<(Entity, &GameObject, &Transform, Option<&CutFor>, Option<&Children>)>,
    mut parts: Query<(&mut Mesh3d, Option<&Transform>, Option<&UncutMesh>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    (baked, ui): (Option<Res<crate::baked::Baked>>, Option<Res<crate::icons::GameUi>>),
) {
    let (Some(baked), Some(ui)) = (baked, ui) else { return };
    let basins = &ui.data.basins;
    if basins.is_empty() {
        return;
    }
    let script = |o: &GameObject| baked.0.catalog_entry(&o.objd).map_or("", |c| c.script.as_str());
    let is_sink = |o: &GameObject| basins.iter().any(|b| b.0 == o.objd);
    let is_counter = |o: &GameObject| {
        let s = script(o);
        s.contains("Counters.") && !s.contains("Sink")
    };
    // (Only when a sink or a counter has come, gone or moved.)
    let dirty = changed.iter().any(|o| is_sink(o) || is_counter(o)) | (removed.read().count() > 0);
    if !dirty {
        return;
    }
    // The sinks and their bowls' points, where they stand.
    let sinks: Vec<(Entity, Vec<Vec3>, [i32; 4])> = objects
        .iter()
        .filter_map(|(e, o, tf, ..)| {
            let (_, pts) = basins.iter().find(|b| b.0 == o.objd)?;
            let at = tf.translation;
            let key = [(at.x * 100.0) as i32, (at.y * 100.0) as i32, (at.z * 100.0) as i32, (tf.rotation.to_euler(EulerRot::YXZ).0 * 100.0) as i32];
            Some((e, pts.iter().map(|p| tf.transform_point(Vec3::from(*p))).collect(), key))
        })
        .collect();
    for (counter, o, tf, cut, children) in &objects {
        if !is_counter(o) {
            continue;
        }
        let near: Vec<&(Entity, Vec<Vec3>, [i32; 4])> = sinks.iter().filter(|s| centre(&s.1).distance(tf.translation.xz()) < IN_COUNTER).collect();
        let want = CutFor(near.iter().map(|s| (s.0, s.2)).collect());
        if cut.is_some_and(|c| *c == want) || (cut.is_none() && near.is_empty()) {
            continue;
        }
        // (Its meshes as baked, in the order its parts were put out: the drawn ones are on the
        // graphics card only.)
        let cpu: Vec<Mesh> = if near.is_empty() {
            Vec::new()
        } else {
            baked.0.catalog_entry(&o.objd).map(|c| c.models.clone()).unwrap_or_default().iter().flat_map(|m| baked.0.model(m).map(crate::objects::cpu_model).unwrap_or_default()).map(|p| p.mesh).collect()
        };
        // Each sink's opening: its bowl where it's under the counter's top (the highest of the
        // counter's flat tops over it), and that height.
        let worlds: Vec<Transform> = children.into_iter().flatten().filter_map(|c| parts.get(*c).ok()).map(|(_, local, _)| *tf * local.copied().unwrap_or_default()).collect();
        let here: Vec<(Entity, Vec<Vec2>, f32, [i32; 4])> = near
            .iter()
            .filter_map(|s| {
                let at = centre(&s.1);
                let top = cpu.iter().zip(&worlds).filter_map(|(m, w)| top_over(m, w, at)).fold(f32::MIN, f32::max);
                if top == f32::MIN {
                    return None;
                }
                let under: Vec<[f32; 2]> = s.1.iter().filter(|p| p.y < top - BELOW_TOP).map(|p| [p.x, p.z]).collect();
                let hull: Vec<Vec2> = s3formats::object::convex_hull(under).into_iter().map(Vec2::from).collect();
                (hull.len() >= 3).then(|| (s.0, grow(&hull, GROW), top, s.2))
            })
            .collect();
        let here: Vec<&(Entity, Vec<Vec2>, f32, [i32; 4])> = here.iter().collect();
        let mut k = 0;
        for child in children.into_iter().flatten() {
            let Ok((mut mesh3d, local, uncut)) = parts.get_mut(*child) else { continue };
            let own = uncut.map_or(mesh3d.0.clone(), |u| u.0.clone());
            if uncut.is_none() {
                commands.entity(*child).insert(UncutMesh(own.clone()));
            }
            let world = *tf * local.copied().unwrap_or_default();
            let cut_mesh = cpu.get(k).and_then(|m| cut_top(m, &world, &here));
            k += 1;
            mesh3d.0 = match cut_mesh {
                Some(m) => meshes.add(m),
                None => own,
            };
        }
        commands.entity(counter).insert(want);
    }
}

/// A counter mesh with its top cut away over each sink's bowl (none: nothing of it there, or a
/// mesh that can't be cut).
fn cut_top(mesh: &Mesh, world: &Transform, sinks: &[&(Entity, Vec<Vec2>, f32, [i32; 4])]) -> Option<Mesh> {
    let Some(VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return None };
    let indices: Vec<u32> = match mesh.indices()? {
        Indices::U16(v) => v.iter().map(|i| *i as u32).collect(),
        Indices::U32(v) => v.clone(),
    };
    // (Every attribute as floats, to cut along with the positions.)
    let mut attrs: Vec<(bevy::mesh::MeshVertexAttribute, usize, Vec<f32>)> = Vec::new();
    for (a, v) in mesh.attributes() {
        let (n, flat): (usize, Vec<f32>) = match v {
            VertexAttributeValues::Float32x2(x) => (2, x.iter().flatten().copied().collect()),
            VertexAttributeValues::Float32x3(x) => (3, x.iter().flatten().copied().collect()),
            VertexAttributeValues::Float32x4(x) => (4, x.iter().flatten().copied().collect()),
            _ => return None,
        };
        attrs.push((a.clone(), n, flat));
    }
    let world_of = |i: u32| world.transform_point(Vec3::from(pos[i as usize]));
    let mut out_idx: Vec<u32> = Vec::with_capacity(indices.len());
    let mut changed = false;
    let mut next = pos.len() as u32;
    for tri in indices.chunks_exact(3) {
        let w = [world_of(tri[0]), world_of(tri[1]), world_of(tri[2])];
        let normal = (w[1] - w[0]).cross(w[2] - w[0]).normalize_or_zero();
        let hit = sinks.iter().find(|s| normal.y.abs() > 0.9 && w.iter().all(|p| (p.y - s.2).abs() < TOP_NEAR) && overlaps(&w, &s.1));
        let Some(s) = hit else {
            out_idx.extend_from_slice(tri);
            continue;
        };
        changed = true;
        // The triangle less the bowl's outline: what's outside each edge in turn, kept.
        let corners = [tri[0], tri[1], tri[2]];
        let mut rest: Vec<(Vec2, [u32; 3], [f32; 3])> = (0..3).map(|k| (w[k].xz(), corners, bary(k))).collect();
        let hole = &s.1;
        for e in 0..hole.len() {
            let (a, b) = (hole[e], hole[(e + 1) % hole.len()]);
            let side = |p: Vec2| (b - a).perp_dot(p - a);
            let outside = clip(&rest, |p| -side(p));
            rest = clip(&rest, side);
            if outside.len() >= 3 {
                // (A fan of new vertices, each the original triangle's attributes blended.)
                let base = next;
                for v in &outside {
                    for (_, n, flat) in attrs.iter_mut() {
                        for c in 0..*n {
                            let val = (0..3).map(|k| flat[v.1[k] as usize * *n + c] * v.2[k]).sum::<f32>();
                            flat.push(val);
                        }
                    }
                    next += 1;
                }
                for k in 1..outside.len() as u32 - 1 {
                    out_idx.extend_from_slice(&[base, base + k, base + k + 1]);
                }
            }
            if rest.len() < 3 {
                break;
            }
        }
    }
    if !changed {
        return None;
    }
    let mut m = Mesh::new(mesh.primitive_topology(), RenderAssetUsages::RENDER_WORLD);
    for (a, n, flat) in attrs {
        let v = match n {
            2 => VertexAttributeValues::Float32x2(flat.chunks_exact(2).map(|c| [c[0], c[1]]).collect()),
            3 => VertexAttributeValues::Float32x3(flat.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()),
            _ => VertexAttributeValues::Float32x4(flat.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()),
        };
        m.insert_attribute(a, v);
    }
    m.insert_indices(Indices::U32(out_idx));
    Some(m)
}

fn bary(k: usize) -> [f32; 3] {
    let mut b = [0.0; 3];
    b[k] = 1.0;
    b
}

/// Whether a triangle (seen from above) and a convex outline overlap at all.
fn overlaps(tri: &[Vec3; 3], hole: &[Vec2]) -> bool {
    let t: Vec<(Vec2, [u32; 3], [f32; 3])> = tri.iter().enumerate().map(|(k, p)| (p.xz(), [0; 3], bary(k))).collect();
    let mut rest = t;
    for e in 0..hole.len() {
        let (a, b) = (hole[e], hole[(e + 1) % hole.len()]);
        rest = clip(&rest, |p| (b - a).perp_dot(p - a));
        if rest.len() < 3 {
            return false;
        }
    }
    // (More than a sliver.)
    area(&rest) > 1e-5
}

fn area(poly: &[(Vec2, [u32; 3], [f32; 3])]) -> f32 {
    (0..poly.len()).map(|i| poly[i].0.perp_dot(poly[(i + 1) % poly.len()].0)).sum::<f32>().abs() * 0.5
}

/// A polygon clipped to where `side` is at least nought (Sutherland–Hodgman), its points'
/// weights blended along the cut edges.
fn clip(poly: &[(Vec2, [u32; 3], [f32; 3])], side: impl Fn(Vec2) -> f32) -> Vec<(Vec2, [u32; 3], [f32; 3])> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (&poly[i], &poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p.0), side(q.0));
        if sp >= 0.0 {
            out.push(*p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            let w = [p.2[0] + (q.2[0] - p.2[0]) * t, p.2[1] + (q.2[1] - p.2[1]) * t, p.2[2] + (q.2[2] - p.2[2]) * t];
            out.push((p.0.lerp(q.0, t), p.1, w));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_top() -> Mesh {
        // A one-metre square counter top at 0.8 m, two triangles facing up.
        let mut m = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-0.5, 0.8, -0.5], [0.5, 0.8, -0.5], [0.5, 0.8, 0.5], [-0.5, 0.8, 0.5]]);
        m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4]);
        m.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        m.insert_indices(Indices::U32(vec![0, 2, 1, 0, 3, 2]));
        m
    }

    fn top_area(m: &Mesh) -> f32 {
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { return 0.0 };
        let Some(Indices::U32(i)) = m.indices() else { return 0.0 };
        i.chunks_exact(3).map(|t| (Vec3::from(p[t[1] as usize]) - Vec3::from(p[t[0] as usize])).cross(Vec3::from(p[t[2] as usize]) - Vec3::from(p[t[0] as usize])).length() * 0.5).sum()
    }

    #[test]
    fn a_sinks_bowl_is_cut_out_of_the_counter_top() {
        let hole = vec![Vec2::new(-0.2, -0.2), Vec2::new(0.2, -0.2), Vec2::new(0.2, 0.2), Vec2::new(-0.2, 0.2)];
        let sink = (Entity::PLACEHOLDER, hole, 0.8, [0; 4]);
        let cut = cut_top(&square_top(), &Transform::IDENTITY, &[&sink]).expect("cut");
        // (The bowl's 0.16 square metres gone from the metre.)
        assert!((top_area(&cut) - 0.84).abs() < 1e-3, "{}", top_area(&cut));
        // (A sink elsewhere, or at another height, cuts nothing.)
        let away = (Entity::PLACEHOLDER, sink.1.iter().map(|p| *p + Vec2::new(3.0, 0.0)).collect(), 0.8, [0; 4]);
        assert!(cut_top(&square_top(), &Transform::IDENTITY, &[&away]).is_none());
        let low = (Entity::PLACEHOLDER, sink.1.clone(), 0.3, [0; 4]);
        assert!(cut_top(&square_top(), &Transform::IDENTITY, &[&low]).is_none());
    }

    #[test]
    fn hulls_go_round_anticlockwise() {
        let h = s3formats::object::convex_hull(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]]);
        assert_eq!(h.len(), 4);
        let area: f32 = (0..h.len()).map(|i| h[i][0] * h[(i + 1) % h.len()][1] - h[(i + 1) % h.len()][0] * h[i][1]).sum();
        assert!(area > 0.0);
    }
}
