//! Hydrology (TRD TR-W3): priority-flood depression filling (lakes), D8 flow
//! directions, precipitation-weighted flow accumulation (rivers), and the
//! water-access field animals use to find drinking water.

use super::{Water, World, HERE, NOWHERE};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

#[derive(PartialEq)]
struct Cell(f32, usize);
impl Eq for Cell {}
impl PartialOrd for Cell {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cell {
    fn cmp(&self, o: &Self) -> Ordering {
        // Min-heap on elevation, deterministic tie-break on index.
        o.0.partial_cmp(&self.0)
            .unwrap_or(Ordering::Equal)
            .then_with(|| o.1.cmp(&self.1))
    }
}

/// Priority-flood over the terrain from the outlets. Returns the filled surface
/// (a lake tile's value is its water level), each tile's D8 downstream tile
/// (`usize::MAX` at outlets) and the tiles in processing order (downstream first).
pub fn priority_flood(world: &World) -> (Vec<f32>, Vec<usize>, Vec<usize>) {
    let (w, h) = (world.w, world.h);
    let n = w * h;
    let mut filled = world.elevation.clone();
    let mut done = vec![false; n];
    let mut heap = BinaryHeap::new();
    // Outlets: ocean tiles, or the polar rows on a world without ocean.
    for i in 0..n {
        let y = i / w;
        let outlet = if world.has_ocean {
            world.water[i] == Water::Ocean
        } else {
            y == 0 || y == h - 1
        };
        if outlet {
            done[i] = true;
            heap.push(Cell(filled[i], i));
        }
    }
    let mut downstream = vec![usize::MAX; n];
    let mut order = Vec::with_capacity(n);
    while let Some(Cell(e, i)) = heap.pop() {
        order.push(i);
        for d in 0..8 {
            if let Some(j) = world.neighbour(i, d) {
                if !done[j] {
                    done[j] = true;
                    // Raise pits slightly above their spill point so water drains.
                    if filled[j] <= e {
                        filled[j] = e + 0.01;
                    }
                    downstream[j] = i;
                    heap.push(Cell(filled[j], j));
                }
            }
        }
    }
    (filled, downstream, order)
}

pub fn compute(world: &mut World) {
    let n = world.w * world.h;
    let (filled, downstream, order) = priority_flood(world);

    // Lakes: land tiles flooded more than 2 m by the fill.
    for i in 0..n {
        if world.water[i] == Water::None && filled[i] - world.elevation[i] > 2.0 {
            world.water[i] = Water::Lake;
        }
    }

    // Flow accumulation in reverse processing order (upstream first).
    let mut acc: Vec<f32> = (0..n)
        .map(|i| {
            if world.water[i] == Water::Ocean {
                0.0
            } else {
                world.precip[i]
            }
        })
        .collect();
    for &i in order.iter().rev() {
        let d = downstream[i];
        if d != usize::MAX && world.water[i] != Water::Ocean {
            acc[d] += acc[i];
        }
    }
    let land_precip: f32 = (0..n)
        .filter(|&i| world.water[i] == Water::None)
        .map(|i| world.precip[i])
        .sum::<f32>()
        / (world.land_tiles().max(1) as f32);
    let threshold = land_precip.max(200.0) * 12.0;
    for i in 0..n {
        world.river_flow[i] = acc[i];
        if world.water[i] == Water::None && acc[i] > threshold {
            world.water[i] = Water::River;
        }
    }

    for i in 0..n {
        world.water_access[i] = match world.water[i] {
            Water::Ocean => 0.0,
            Water::Lake | Water::River => 1.0,
            Water::None => ((world.precip[i] - 150.0) / 650.0).clamp(0.0, 0.95),
        };
    }
    water_field(world);
}

/// Multi-source BFS over land from tiles with good water access.
pub fn water_field(world: &mut World) {
    let n = world.w * world.h;
    let mut q = VecDeque::new();
    for i in 0..n {
        world.water_dir[i] = NOWHERE;
        world.water_dist[i] = u16::MAX;
        if world.is_land(i) && world.water_access[i] >= 0.5 {
            world.water_dir[i] = HERE;
            world.water_dist[i] = 0;
            q.push_back(i);
        }
    }
    while let Some(i) = q.pop_front() {
        for d in 0..8 {
            if let Some(j) = world.neighbour(i, d) {
                if world.is_land(j) && world.water_dist[j] == u16::MAX {
                    world.water_dist[j] = world.water_dist[i] + 1;
                    // Direction from j back toward i is the opposite of d.
                    world.water_dir[j] = ((d + 4) % 8) as u8;
                    q.push_back(j);
                }
            }
        }
    }
}
