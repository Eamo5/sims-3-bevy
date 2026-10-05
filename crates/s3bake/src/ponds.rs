//! Lots' own ground, and ponds. Lots keep their sculpted ground and a water table
//! (`s3formats::lot::LotTerrain`); wherever the ground dips below the water table there is a
//! pond. The world heightmap is flattened under lots, so the lots' dips (pond basins, the sunken
//! basins of fountains, hollows) are carved back into it here.

use s3formats::lot::LotTerrain;
use s3formats::world::{Heightmap, LotInfo};
use s3pkg::Package;

use crate::types::{LotBuildingBaked, PondBaked};

/// Carves each lot's dips (all of a pond lot's ground around the water) into `hm` and returns the
/// lots' water. Ground under a house's floors is left alone.
pub fn carve_ponds(pkg: &Package, lots: &[LotInfo], buildings: &[LotBuildingBaked], hm: &mut Heightmap) -> Vec<PondBaked> {
    let mut out = Vec::new();
    for (i, lot) in lots.iter().enumerate() {
        let Some(t) = LotTerrain::load(pkg, lot.id) else { continue };
        let no_water = vec![-100.0; t.nx * t.nz];
        let water = t.water.as_ref().unwrap_or(&no_water);
        // Cells under a house's floors (with a tile around them).
        let housed: std::collections::HashSet<(i64, i64)> = buildings
            .iter()
            .filter(|b| b.lot as usize == i)
            .flat_map(|b| b.floors.iter().filter(|f| f.level >= 1))
            .flat_map(|f| (-1..=1).flat_map(move |dx| (-1..=1).map(move |dz| (f.x as i64 + dx, f.z as i64 + dz))))
            .collect();
        let (s, c) = lot.rotation.sin_cos();
        let to_world = |x: f32, z: f32| (lot.corner[0] + x * c + z * s, lot.corner[2] - x * s + z * c);
        // The ground level, and the lot's base height: set on the base that best fits the world
        // along the lot's edge, the ground level is the one nearest the lot's own height.
        let mut edge = Vec::new();
        for x in 0..t.nx {
            edge.push((x, 0));
            edge.push((x, t.nz - 1));
        }
        for z in 0..t.nz {
            edge.push((0, z));
            edge.push((t.nx - 1, z));
        }
        let Some((ground, base)) = (0..t.levels.len())
            .map(|l| {
                let mut d: Vec<f32> = edge
                    .iter()
                    .map(|&(x, z)| {
                        let (wx, wz) = to_world(x as f32, z as f32);
                        hm.sample(wx, wz) - t.at(l, x, z)
                    })
                    .collect();
                d.sort_by(f32::total_cmp);
                let mid = d[d.len() / 2];
                // How well the lot's edge meets the world around it.
                let mut dev: Vec<f32> = d.iter().map(|v| (v - mid).abs()).collect();
                dev.sort_by(f32::total_cmp);
                (l, mid, dev[dev.len() / 2])
            })
            .min_by(|a, b| (a.1 - lot.corner[1]).abs().total_cmp(&(b.1 - lot.corner[1]).abs()))
            .filter(|(_, _, dev)| *dev < 0.15)
            .map(|(l, b, _)| (l, b))
        else {
            continue;
        };
        let g = &t.levels[ground];
        let wet = |k: usize| water[k] > g[k] + 0.01;
        let pond = (0..t.nx * t.nz).filter(|&k| wet(k)).count() >= 4;
        // Carve the lot's ground into the world around the water (the rest of the lot is left
        // as the world has it, so floors and foundations aren't disturbed).
        let near_water = |lx: f32, lz: f32| {
            let (cx, cz) = (lx.round() as i64, lz.round() as i64);
            (-2..=2).any(|dx| {
                (-2..=2).any(|dz| {
                    let (x, z) = (cx + dx, cz + dz);
                    x >= 0 && z >= 0 && (x as usize) < t.nx && (z as usize) < t.nz && wet(x as usize * t.nz + z as usize)
                })
            })
        };
        let corners = [(0.0, 0.0), (t.nx as f32 - 1.0, 0.0), (0.0, t.nz as f32 - 1.0), (t.nx as f32 - 1.0, t.nz as f32 - 1.0)].map(|(x, z)| to_world(x, z));
        let (x0, x1) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (z0, z1) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let ground_at = |lx: f32, lz: f32| {
            let (fx, fz) = (lx.floor(), lz.floor());
            let (ix, iz) = (fx as usize, fz as usize);
            let (tx, tz) = (lx - fx, lz - fz);
            let h = |x: usize, z: usize| t.at(ground, x, z);
            let a = h(ix, iz) + (h(ix + 1, iz) - h(ix, iz)) * tx;
            let b = h(ix, iz + 1) + (h(ix + 1, iz + 1) - h(ix, iz + 1)) * tx;
            a + (b - a) * tz
        };
        let mut carved = 0;
        for iz in (z0.floor().max(0.0) as usize)..=(z1.ceil() as usize).min(hm.height - 1) {
            for ix in (x0.floor().max(0.0) as usize)..=(x1.ceil() as usize).min(hm.width - 1) {
                let (dx, dz) = (ix as f32 - lot.corner[0], iz as f32 - lot.corner[2]);
                let (lx, lz) = (dx * c - dz * s, dx * s + dz * c);
                if lx < 0.0 || lz < 0.0 || lx > t.nx as f32 - 1.0 || lz > t.nz as f32 - 1.0 {
                    continue;
                }
                let h = base + ground_at(lx, lz);
                let v = (h / hm.scale).round().clamp(0.0, 65535.0) as u16;
                let k = iz * hm.width + ix;
                // Around a pond, the lot's ground as it is; elsewhere only its dips, and not
                // under a house.
                let dip = v < hm.data[k] && (hm.data[k] - v) as f32 * hm.scale > 0.1 && !housed.contains(&(lx.floor() as i64, lz.floor() as i64));
                if !(pond && near_water(lx, lz) || dip) {
                    continue;
                }
                if hm.data[k] != v {
                    hm.data[k] = v;
                    carved += 1;
                }
            }
        }
        if !pond {
            if carved > 0 {
                eprintln!("lot {}: {carved} heights dug", lot.internal_name);
            }
            continue;
        }
        let levels: Vec<f32> = (0..t.nx * t.nz).map(|k| if wet(k) { base + water[k] } else { f32::NAN }).collect();
        let wet_n = (0..t.nx * t.nz).filter(|&k| wet(k)).count();
        let (lo, hi) = (0..t.nx * t.nz).filter(|&k| wet(k)).fold((f32::MAX, f32::MIN), |(a, b), k| (a.min(water[k]), b.max(water[k])));
        let deepest = (0..t.nx * t.nz).filter(|&k| wet(k)).map(|k| water[k] - g[k]).fold(0.0f32, f32::max);
        eprintln!(
            "pond on {}: ground level {ground} of {}, base {base:.2}, {wet_n} wet vertices, water {lo:.2}..{hi:.2}, deepest {deepest:.2}, {carved} heights carved",
            lot.internal_name,
            t.levels.len()
        );
        out.push(PondBaked { lot: i as u32, nx: t.nx as u32, nz: t.nz as u32, water: levels });
    }
    out
}
