//! FPTVOX7 exact surface extraction from a Metal-sampled scalar lattice.

use crate::ffi::{FptRenderConfig, fpt_metal_sample_materials, fpt_metal_sample_topology_grid};
use crate::fptvox::{
    FptvoxBvhCell, FptvoxBvhNode, FptvoxBvhTriangle, FptvoxIndexedTriangle,
    FptvoxIndexedTriangleBvhSurface, FptvoxIndexedTriangleCell, FptvoxIndexedTriangleSurface,
    FptvoxTriangle, FptvoxTriangleBvhSurface, FptvoxTriangleCell, FptvoxTriangleSurface,
};
use crate::mandelbulber::MandelbulberMaterial;
use crate::voxel::{Aabb, CoordinateSystem, SurfaceMaterial, VoxelCell};
use anyhow::{Context, Result, ensure};
use marching_cubes::tables::{EDGE_TABLE, TRI_TABLE};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::path::Path;
use std::time::Instant;

const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [1, 2],
    [2, 3],
    [3, 0],
    [4, 5],
    [5, 6],
    [6, 7],
    [7, 4],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];
const NORMAL_CLUSTER_COSINE: f32 = 0.94;

#[derive(Clone, Copy, Default)]
struct SurfaceVertex {
    position: [f32; 3],
    color_index: f32,
    color: [f32; 3],
}

/// One normalized mesh vertex used by the exact PLY-to-FPTVOX7 reference path.
#[derive(Clone, Copy, Debug)]
pub struct MeshSurfaceVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

#[derive(Clone, Copy)]
struct ColorCluster {
    weight: f32,
    normal_sum: [f32; 3],
    color_sum: [f32; 3],
}

#[derive(Default)]
struct CellSurface {
    coordinate: [u32; 3],
    triangles: Vec<FptvoxTriangle>,
    color_clusters: Vec<ColorCluster>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Fptvox7BuildSummary {
    pub topology_policy: String,
    pub topology_fallback_offset: f32,
    pub topology_threshold_scale: f32,
    pub metal_topology_ms: f64,
    pub marching_cubes_ms: f64,
    pub clipping_ms: f64,
    pub metal_material_ms: f64,
    pub source_triangles: u64,
    pub clipped_triangles: u64,
    pub occupied_cells: usize,
    pub boundary_cells: usize,
    /// Occupied boundary cells in x-, x+, y-, y+, z-, z+ order.
    pub boundary_face_cells: [usize; 6],
}

#[derive(Clone, Debug)]
pub struct Fptvox7Build {
    pub surface: FptvoxTriangleSurface,
    pub summary: Fptvox7BuildSummary,
}

fn c_path(path: &Path) -> Result<CString> {
    CString::new(path.to_string_lossy().as_bytes()).context("path contains a NUL byte")
}

fn bridge_error(error: &[i8]) -> String {
    let bytes = error
        .iter()
        .take_while(|value| **value != 0)
        .map(|value| *value as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn boundary_cell_counts<'a>(
    coordinates: impl IntoIterator<Item = &'a [u32; 3]>,
    resolution: [u32; 3],
) -> (usize, [usize; 6]) {
    let mut boundary_cells = 0;
    let mut faces = [0; 6];
    for coordinate in coordinates {
        let mut boundary = false;
        for axis in 0..3 {
            if coordinate[axis] == 0 {
                faces[axis * 2] += 1;
                boundary = true;
            }
            if coordinate[axis] + 1 == resolution[axis] {
                faces[axis * 2 + 1] += 1;
                boundary = true;
            }
        }
        boundary_cells += usize::from(boundary);
    }
    (boundary_cells, faces)
}

fn interpolate(a: SurfaceVertex, b: SurfaceVertex, av: f32, bv: f32) -> SurfaceVertex {
    let denominator = bv - av;
    let amount = if denominator.abs() <= 1.0e-20 {
        0.5
    } else {
        (-av / denominator).clamp(0.0, 1.0)
    };
    SurfaceVertex {
        position: std::array::from_fn(|axis| {
            a.position[axis] + (b.position[axis] - a.position[axis]) * amount
        }),
        color_index: a.color_index + (b.color_index - a.color_index) * amount,
        color: [0.0; 3],
    }
}

fn clip_polygon(
    mut polygon: Vec<SurfaceVertex>,
    axis: usize,
    boundary: f32,
    keep_greater: bool,
) -> Vec<SurfaceVertex> {
    if polygon.is_empty() {
        return polygon;
    }
    let inside = |point: SurfaceVertex| {
        if keep_greater {
            point.position[axis] >= boundary - 1.0e-6
        } else {
            point.position[axis] <= boundary + 1.0e-6
        }
    };
    let mut output = Vec::with_capacity(polygon.len() + 1);
    let mut previous = *polygon.last().expect("nonempty polygon");
    let mut previous_inside = inside(previous);
    for current in polygon.drain(..) {
        let current_inside = inside(current);
        if current_inside != previous_inside {
            let denominator = current.position[axis] - previous.position[axis];
            let amount = if denominator.abs() <= 1.0e-20 {
                0.0
            } else {
                ((boundary - previous.position[axis]) / denominator).clamp(0.0, 1.0)
            };
            output.push(SurfaceVertex {
                position: std::array::from_fn(|component| {
                    previous.position[component]
                        + (current.position[component] - previous.position[component]) * amount
                }),
                color_index: previous.color_index
                    + (current.color_index - previous.color_index) * amount,
                color: std::array::from_fn(|component| {
                    previous.color[component]
                        + (current.color[component] - previous.color[component]) * amount
                }),
            });
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn triangle_area_squared(triangle: [[f32; 3]; 3]) -> f32 {
    let a = std::array::from_fn::<_, 3, _>(|axis| triangle[1][axis] - triangle[0][axis]);
    let b = std::array::from_fn::<_, 3, _>(|axis| triangle[2][axis] - triangle[0][axis]);
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    cross.iter().map(|value| value * value).sum()
}

fn triangle_normal(triangle: [SurfaceVertex; 3]) -> [f32; 3] {
    let a = std::array::from_fn::<_, 3, _>(|axis| {
        triangle[1].position[axis] - triangle[0].position[axis]
    });
    let b = std::array::from_fn::<_, 3, _>(|axis| {
        triangle[2].position[axis] - triangle[0].position[axis]
    });
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = cross.iter().map(|value| value * value).sum::<f32>().sqrt();
    if length > 1.0e-20 {
        cross.map(|value| value / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn polygon_area(polygon: &[SurfaceVertex]) -> f32 {
    (1..polygon.len() - 1)
        .map(|index| {
            triangle_area_squared([
                polygon[0].position,
                polygon[index].position,
                polygon[index + 1].position,
            ])
            .sqrt()
                * 0.5
        })
        .sum()
}

impl CellSurface {
    fn add_color(&mut self, normal: [f32; 3], color: [f32; 3], weight: f32) {
        if let Some(cluster) = self.color_clusters.iter_mut().find(|cluster| {
            let length = cluster
                .normal_sum
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                .sqrt()
                .max(1.0e-20);
            let dot = (0..3)
                .map(|axis| cluster.normal_sum[axis] / length * normal[axis])
                .sum::<f32>();
            dot.abs() >= NORMAL_CLUSTER_COSINE
        }) {
            cluster.weight += weight;
            for axis in 0..3 {
                cluster.normal_sum[axis] += normal[axis] * weight;
                cluster.color_sum[axis] += color[axis] * weight;
            }
        } else {
            self.color_clusters.push(ColorCluster {
                weight,
                normal_sum: normal.map(|value| value * weight),
                color_sum: color.map(|value| value * weight),
            });
        }
    }

    fn primary_color(&self) -> [f32; 3] {
        let cluster = self
            .color_clusters
            .iter()
            .max_by(|left, right| left.weight.total_cmp(&right.weight))
            .expect("surface cell has a color cluster");
        cluster
            .color_sum
            .map(|value| value / cluster.weight.max(f32::EPSILON))
    }
}

fn mandelbulber_mesh_color(index: f32, material: &MandelbulberMaterial) -> [f32; 3] {
    let wrapped = index.abs() % (248.0 * 256.0);
    let position = (wrapped / 256.0 / 10.0 * material.coloring_speed as f32
        + material.palette_offset as f32)
        % 1.0;
    let stops = &material.surface_gradient;
    let upper = stops.partition_point(|stop| stop.position < position);
    let second = upper.min(stops.len() - 1);
    let first = second.saturating_sub(1);
    if first == second {
        return stops[first].color;
    }
    let amount = ((position - stops[first].position)
        / (stops[second].position - stops[first].position).max(1.0e-20))
    .clamp(0.0, 1.0);
    std::array::from_fn(|axis| {
        let first_u8 = (stops[first].color[axis] * 256.0).floor();
        let second_u8 = (stops[second].color[axis] * 256.0).floor();
        (first_u8 * (1.0 - amount) + second_u8 * amount).floor() / 255.0
    })
}

fn pack_vertex(vertex: [f32; 3]) -> u32 {
    let quantize = |value: f32| (value.clamp(0.0, 1.0) * 1023.0).round() as u32;
    quantize(vertex[0]) | (quantize(vertex[1]) << 10) | (quantize(vertex[2]) << 20)
}

fn pack_triangle_color(triangle: [MeshSurfaceVertex; 3]) -> u32 {
    (0..3).fold(0u32, |packed, channel| {
        let value = triangle
            .iter()
            .map(|vertex| vertex.color[channel])
            .sum::<f32>()
            / 3.0;
        packed | (u32::from((value.clamp(0.0, 1.0) * 255.0).round() as u8) << (channel * 8))
    })
}

fn pack_vertex_color(color: [f32; 3]) -> u32 {
    (0..3).fold(0u32, |packed, channel| {
        packed
            | (u32::from((color[channel].clamp(0.0, 1.0) * 255.0).round() as u8) << (channel * 8))
    })
}

fn append_clipped_triangle(
    cells: &mut BTreeMap<u64, CellSurface>,
    triangle: [SurfaceVertex; 3],
    resolution: [u32; 3],
) {
    let grid = triangle.map(|vertex| SurfaceVertex {
        position: std::array::from_fn(|axis| vertex.position[axis] * resolution[axis] as f32),
        color_index: vertex.color_index,
        color: vertex.color,
    });
    let minimum: [f32; 3] = std::array::from_fn(|axis| {
        grid.iter()
            .map(|vertex| vertex.position[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let maximum: [f32; 3] = std::array::from_fn(|axis| {
        grid.iter()
            .map(|vertex| vertex.position[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    let start: [u32; 3] = std::array::from_fn(|axis| minimum[axis].floor().max(0.0) as u32);
    let end: [u32; 3] =
        std::array::from_fn(|axis| maximum[axis].floor().min(resolution[axis] as f32 - 1.0) as u32);
    for z in start[2]..=end[2] {
        for y in start[1]..=end[1] {
            for x in start[0]..=end[0] {
                let coordinate = [x, y, z];
                let mut polygon = grid.to_vec();
                for axis in 0..3 {
                    polygon = clip_polygon(polygon, axis, coordinate[axis] as f32, true);
                    polygon = clip_polygon(polygon, axis, coordinate[axis] as f32 + 1.0, false);
                }
                if polygon.len() < 3 {
                    continue;
                }
                let local = polygon
                    .iter()
                    .map(|vertex| {
                        std::array::from_fn(|axis| vertex.position[axis] - coordinate[axis] as f32)
                    })
                    .collect::<Vec<_>>();
                let linear = u64::from(x)
                    + u64::from(y) * u64::from(resolution[0])
                    + u64::from(z) * u64::from(resolution[0]) * u64::from(resolution[1]);
                let mut retained = Vec::new();
                for index in 1..local.len() - 1 {
                    let clipped = [local[0], local[index], local[index + 1]];
                    if triangle_area_squared(clipped) > 1.0e-12 {
                        retained.push(FptvoxTriangle {
                            vertices: clipped.map(pack_vertex),
                        });
                    }
                }
                if !retained.is_empty() {
                    let normal = triangle_normal(grid);
                    let color = std::array::from_fn(|axis| {
                        polygon.iter().map(|vertex| vertex.color[axis]).sum::<f32>()
                            / polygon.len() as f32
                    });
                    let weight = polygon_area(&polygon).max(1.0e-6);
                    let cell = cells.entry(linear).or_insert_with(|| CellSurface {
                        coordinate,
                        ..CellSurface::default()
                    });
                    cell.triangles.extend(retained);
                    cell.add_color(normal, color, weight);
                }
            }
        }
    }
}

fn pack_indexed_vertex(vertex: [f32; 3]) -> [u16; 3] {
    vertex.map(|value| (value.clamp(0.0, 1.0) * 65535.0).round() as u16)
}

fn append_indexed_triangle(
    cells: &mut BTreeMap<u64, (CellSurface, Vec<u32>)>,
    triangle: [SurfaceVertex; 3],
    triangle_index: u32,
    resolution: [u32; 3],
) -> bool {
    let grid = triangle.map(|vertex| SurfaceVertex {
        position: std::array::from_fn(|axis| vertex.position[axis] * resolution[axis] as f32),
        color_index: vertex.color_index,
        color: vertex.color,
    });
    let minimum: [f32; 3] = std::array::from_fn(|axis| {
        grid.iter()
            .map(|vertex| vertex.position[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let maximum: [f32; 3] = std::array::from_fn(|axis| {
        grid.iter()
            .map(|vertex| vertex.position[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    let start: [u32; 3] = std::array::from_fn(|axis| minimum[axis].floor().max(0.0) as u32);
    let end: [u32; 3] =
        std::array::from_fn(|axis| maximum[axis].floor().min(resolution[axis] as f32 - 1.0) as u32);
    let mut referenced = false;
    for z in start[2]..=end[2] {
        for y in start[1]..=end[1] {
            for x in start[0]..=end[0] {
                let coordinate = [x, y, z];
                let mut polygon = grid.to_vec();
                for axis in 0..3 {
                    polygon = clip_polygon(polygon, axis, coordinate[axis] as f32, true);
                    polygon = clip_polygon(polygon, axis, coordinate[axis] as f32 + 1.0, false);
                }
                if polygon.len() < 3 {
                    continue;
                }
                let local = polygon
                    .iter()
                    .map(|vertex| {
                        std::array::from_fn(|axis| vertex.position[axis] - coordinate[axis] as f32)
                    })
                    .collect::<Vec<_>>();
                if !(1..local.len() - 1).any(|index| {
                    triangle_area_squared([local[0], local[index], local[index + 1]]) > 1.0e-12
                }) {
                    continue;
                }
                let linear = u64::from(x)
                    + u64::from(y) * u64::from(resolution[0])
                    + u64::from(z) * u64::from(resolution[0]) * u64::from(resolution[1]);
                let normal = triangle_normal(grid);
                let color = std::array::from_fn(|axis| {
                    polygon.iter().map(|vertex| vertex.color[axis]).sum::<f32>()
                        / polygon.len() as f32
                });
                let weight = polygon_area(&polygon).max(1.0e-6);
                let (surface, references) = cells.entry(linear).or_insert_with(|| {
                    (
                        CellSurface {
                            coordinate,
                            ..CellSurface::default()
                        },
                        Vec::new(),
                    )
                });
                surface.add_color(normal, color, weight);
                references.push(triangle_index);
                referenced = true;
            }
        }
    }
    referenced
}

fn append_marching_cube(
    triangles: &mut Vec<[SurfaceVertex; 3]>,
    positions: [SurfaceVertex; 8],
    values: [f32; 8],
    material: Option<&MandelbulberMaterial>,
) {
    let mut cube = 0_usize;
    for (corner, value) in values.iter().enumerate() {
        if *value < 0.0 {
            cube |= 1 << corner;
        }
    }
    let edge_mask = EDGE_TABLE[cube] as u16;
    if edge_mask == 0 {
        return;
    }
    let mut vertices = [SurfaceVertex::default(); 12];
    for edge in 0..12 {
        if edge_mask & (1 << edge) != 0 {
            let [a, b] = EDGES[edge];
            let mut vertex = interpolate(positions[a], positions[b], values[a], values[b]);
            vertex.color = material.map_or([1.0; 3], |material| {
                mandelbulber_mesh_color(vertex.color_index, material)
            });
            vertices[edge] = vertex;
        }
    }
    for edges in TRI_TABLE[cube].chunks_exact(3) {
        if edges[0] < 0 {
            break;
        }
        triangles.push([
            vertices[edges[0] as usize],
            vertices[edges[1] as usize],
            vertices[edges[2] as usize],
        ]);
    }
}

fn extract_surface(
    field: &[f32],
    color_indices: &[f32],
    sample_resolution: [usize; 3],
    output_offset: [f32; 3],
    output_step: [f32; 3],
    material: Option<&MandelbulberMaterial>,
) -> Vec<[SurfaceVertex; 3]> {
    let index = |x: usize, y: usize, z: usize| {
        x + y * sample_resolution[0] + z * sample_resolution[0] * sample_resolution[1]
    };
    let mut triangles = Vec::new();
    for z in 0..sample_resolution[2] - 1 {
        for y in 0..sample_resolution[1] - 1 {
            for x in 0..sample_resolution[0] - 1 {
                let positions = CORNERS.map(|corner| {
                    let sample = index(x + corner[0], y + corner[1], z + corner[2]);
                    SurfaceVertex {
                        position: [
                            (x + corner[0]) as f32 * output_step[0] + output_offset[0],
                            (y + corner[1]) as f32 * output_step[1] + output_offset[1],
                            (z + corner[2]) as f32 * output_step[2] + output_offset[2],
                        ],
                        color_index: color_indices[sample],
                        color: [0.0; 3],
                    }
                });
                let values =
                    CORNERS.map(|corner| field[index(x + corner[0], y + corner[1], z + corner[2])]);
                append_marching_cube(&mut triangles, positions, values, material);
            }
        }
    }
    triangles
}

fn clip_surface(
    triangles: &[[SurfaceVertex; 3]],
    output_resolution: [u32; 3],
) -> BTreeMap<u64, CellSurface> {
    let mut cells = BTreeMap::new();
    for triangle in triangles {
        append_clipped_triangle(&mut cells, *triangle, output_resolution);
    }
    cells
}

/// Build an exact triangle surface from a mesh already normalized to the
/// requested output bounds. This uses the same cell clipping and quantization
/// as the Metal-generated V7 path so comparisons isolate mesh generation.
pub fn build_triangle_surface_from_normalized_mesh<I>(
    triangles: I,
    source_triangle_count: usize,
    output_resolution: u32,
    sampling_resolution: u32,
    bounds: Aabb,
    material_template: SurfaceMaterial,
) -> Result<FptvoxTriangleSurface>
where
    I: IntoIterator<Item = [MeshSurfaceVertex; 3]>,
{
    build_triangle_surface_from_normalized_mesh_3d(
        triangles,
        source_triangle_count,
        [output_resolution; 3],
        [sampling_resolution; 3],
        bounds,
        material_template,
    )
}

pub fn build_triangle_surface_from_normalized_mesh_3d<I>(
    triangles: I,
    source_triangle_count: usize,
    output_resolution: [u32; 3],
    sampling_resolution: [u32; 3],
    bounds: Aabb,
    material_template: SurfaceMaterial,
) -> Result<FptvoxTriangleSurface>
where
    I: IntoIterator<Item = [MeshSurfaceVertex; 3]>,
{
    ensure!(
        output_resolution
            .iter()
            .all(|resolution| (1..=512).contains(resolution)),
        "V7 output resolution must be 1..512"
    );
    ensure!(
        sampling_resolution
            .iter()
            .all(|resolution| (2..=1024).contains(resolution)),
        "V7 mesh sampling resolution must be 2..1024"
    );
    let mut cell_triangles = BTreeMap::new();
    for triangle in triangles {
        let triangle = triangle.map(|vertex| SurfaceVertex {
            position: vertex.position,
            color_index: 0.0,
            color: vertex.color,
        });
        append_clipped_triangle(&mut cell_triangles, triangle, output_resolution);
    }
    ensure!(
        !cell_triangles.is_empty(),
        "normalized mesh produced no FPTVOX7 surface cells"
    );

    let mut cells = Vec::with_capacity(cell_triangles.len());
    let mut clipped_triangles = Vec::new();
    for (_, surface) in cell_triangles {
        let color = surface.primary_color();
        let cell = VoxelCell::from_material(SurfaceMaterial {
            base_color: color,
            ..material_template
        });
        let first_triangle =
            u32::try_from(clipped_triangles.len()).context("V7 triangle offset exceeds u32")?;
        let triangle_count =
            u32::try_from(surface.triangles.len()).context("V7 cell triangle count exceeds u32")?;
        clipped_triangles.extend(surface.triangles);
        cells.push(FptvoxTriangleCell {
            coordinate: surface.coordinate,
            cell,
            first_triangle,
            triangle_count,
        });
    }
    ensure!(
        source_triangle_count > 0 && !clipped_triangles.is_empty(),
        "normalized mesh requires non-empty source and clipped triangle streams"
    );
    Ok(FptvoxTriangleSurface {
        resolution: output_resolution,
        sampling_resolution,
        bounds,
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells,
        triangles: clipped_triangles,
    })
}

/// Build the V8 source-triangle representation from a normalized mesh.
/// Source triangles remain global; occupied cells contain only an index list.
pub fn build_indexed_triangle_surface_from_normalized_mesh_3d<I>(
    triangles: I,
    output_resolution: [u32; 3],
    sampling_resolution: [u32; 3],
    bounds: Aabb,
    material_template: SurfaceMaterial,
) -> Result<FptvoxIndexedTriangleSurface>
where
    I: IntoIterator<Item = [MeshSurfaceVertex; 3]>,
{
    ensure!(
        output_resolution
            .iter()
            .all(|resolution| (1..=512).contains(resolution)),
        "V8 output resolution must be 1..512"
    );
    ensure!(
        sampling_resolution
            .iter()
            .all(|resolution| (2..=1024).contains(resolution)),
        "V8 mesh sampling resolution must be 2..1024"
    );
    let mut indexed_cells = BTreeMap::new();
    let mut indexed_triangles = Vec::new();
    let mut triangle_colors = Vec::new();
    let mut triangle_vertex_colors = Vec::new();
    for triangle in triangles {
        let surface_triangle = triangle.map(|vertex| SurfaceVertex {
            position: vertex.position,
            color_index: 0.0,
            color: vertex.color,
        });
        let triangle_index =
            u32::try_from(indexed_triangles.len()).context("V8 triangle index exceeds u32")?;
        let indexed_triangle = FptvoxIndexedTriangle {
            vertices: triangle.map(|vertex| pack_indexed_vertex(vertex.position)),
        };
        let quantized_surface_triangle = std::array::from_fn(|vertex| SurfaceVertex {
            position: indexed_triangle.vertices[vertex]
                .map(|component| f32::from(component) / 65535.0),
            ..surface_triangle[vertex]
        });
        if append_indexed_triangle(
            &mut indexed_cells,
            quantized_surface_triangle,
            triangle_index,
            output_resolution,
        ) {
            indexed_triangles.push(indexed_triangle);
            triangle_colors.push(pack_triangle_color(triangle));
            triangle_vertex_colors.push(triangle.map(|vertex| pack_vertex_color(vertex.color)));
        }
    }
    ensure!(
        !indexed_cells.is_empty() && !indexed_triangles.is_empty(),
        "normalized mesh produced no FPTVOX8 surface cells"
    );

    let mut cells = Vec::with_capacity(indexed_cells.len());
    let mut references = Vec::new();
    for (_, (surface, cell_references)) in indexed_cells {
        let first_reference =
            u32::try_from(references.len()).context("V8 reference offset exceeds u32")?;
        let reference_count =
            u32::try_from(cell_references.len()).context("V8 cell reference count exceeds u32")?;
        references.extend(cell_references);
        cells.push(FptvoxIndexedTriangleCell {
            coordinate: surface.coordinate,
            cell: VoxelCell::from_material(SurfaceMaterial {
                base_color: surface.primary_color(),
                ..material_template
            }),
            first_reference,
            reference_count,
        });
    }
    Ok(FptvoxIndexedTriangleSurface {
        resolution: output_resolution,
        sampling_resolution,
        bounds,
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells,
        triangles: indexed_triangles,
        triangle_colors,
        triangle_vertex_colors,
        references,
    })
}

#[derive(Clone, Copy)]
struct BvhBuildTriangle {
    triangle: FptvoxTriangle,
    original_order: u32,
    min: [f32; 3],
    max: [f32; 3],
    centroid: [f32; 3],
}

fn decode_local_triangle_vertex(packed: u32) -> [f32; 3] {
    [
        (packed & 0x3ff) as f32 / 1023.0,
        ((packed >> 10) & 0x3ff) as f32 / 1023.0,
        ((packed >> 20) & 0x3ff) as f32 / 1023.0,
    ]
}

fn quantize_bvh_bounds(minimum: [f32; 3], maximum: [f32; 3]) -> [u8; 6] {
    let mut packed = [0_u8; 6];
    for axis in 0..3 {
        let minimum = (minimum[axis] * 255.0).floor() as i32 - 1;
        let maximum = (maximum[axis] * 255.0).ceil() as i32 + 1;
        packed[axis] = minimum.clamp(0, 255) as u8;
        packed[axis + 3] = maximum.clamp(0, 255) as u8;
    }
    packed
}

fn append_stackless_bvh(
    triangles: &mut [BvhBuildTriangle],
    leaf_size: usize,
    nodes: &mut Vec<FptvoxBvhNode>,
    reordered: &mut Vec<FptvoxBvhTriangle>,
) -> Result<()> {
    let node_index = nodes.len();
    nodes.push(FptvoxBvhNode {
        bounds: [0; 6],
        triangle_count: 0,
        first_triangle: 0,
        escape: 0,
    });
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for triangle in triangles.iter() {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(triangle.min[axis]);
            maximum[axis] = maximum[axis].max(triangle.max[axis]);
        }
    }
    let bounds = quantize_bvh_bounds(minimum, maximum);
    if triangles.len() <= leaf_size {
        let first_triangle =
            u32::try_from(reordered.len()).context("V10 triangle offset exceeds u32")?;
        let triangle_count =
            u16::try_from(triangles.len()).context("V10 leaf triangle count exceeds u16")?;
        reordered.extend(triangles.iter().map(|triangle| FptvoxBvhTriangle {
            triangle: triangle.triangle,
            original_order: triangle.original_order,
        }));
        nodes[node_index] = FptvoxBvhNode {
            bounds,
            triangle_count,
            first_triangle,
            escape: u32::try_from(nodes.len()).context("V10 node count exceeds u32")?,
        };
        return Ok(());
    }

    let extent = std::array::from_fn::<_, 3, _>(|axis| maximum[axis] - minimum[axis]);
    let axis = (0..3)
        .max_by(|left, right| extent[*left].total_cmp(&extent[*right]))
        .expect("three BVH axes");
    triangles.sort_unstable_by(|left, right| {
        left.centroid[axis]
            .total_cmp(&right.centroid[axis])
            .then_with(|| left.original_order.cmp(&right.original_order))
    });
    let middle = triangles.len() / 2;
    let (left, right) = triangles.split_at_mut(middle);
    append_stackless_bvh(left, leaf_size, nodes, reordered)?;
    append_stackless_bvh(right, leaf_size, nodes, reordered)?;
    nodes[node_index] = FptvoxBvhNode {
        bounds,
        triangle_count: 0,
        first_triangle: 0,
        escape: u32::try_from(nodes.len()).context("V10 node count exceeds u32")?,
    };
    Ok(())
}

/// Build a stackless per-cell BVH over the exact quantized V7 triangle stream.
/// Triangle intersections remain bit-identical; `original_order` restores V7's
/// deterministic winner when two triangles produce the same depth.
pub fn build_triangle_bvh_surface(
    surface: &FptvoxTriangleSurface,
    leaf_size: usize,
) -> Result<FptvoxTriangleBvhSurface> {
    ensure!((2..=64).contains(&leaf_size), "V10 leaf size must be 2..64");
    let mut cells = Vec::with_capacity(surface.cells.len());
    let mut triangles = Vec::with_capacity(surface.triangles.len());
    let mut nodes = Vec::new();
    for cell in &surface.cells {
        let start = cell.first_triangle as usize;
        let end = start + cell.triangle_count as usize;
        let mut build_triangles = surface.triangles[start..end]
            .iter()
            .enumerate()
            .map(|(order, triangle)| {
                let vertices = triangle.vertices.map(decode_local_triangle_vertex);
                let min = std::array::from_fn(|axis| {
                    vertices
                        .iter()
                        .map(|vertex| vertex[axis])
                        .fold(f32::INFINITY, f32::min)
                });
                let max = std::array::from_fn(|axis| {
                    vertices
                        .iter()
                        .map(|vertex| vertex[axis])
                        .fold(f32::NEG_INFINITY, f32::max)
                });
                BvhBuildTriangle {
                    triangle: *triangle,
                    original_order: order as u32,
                    min,
                    max,
                    centroid: std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5),
                }
            })
            .collect::<Vec<_>>();
        let first_node = u32::try_from(nodes.len()).context("V10 node offset exceeds u32")?;
        append_stackless_bvh(&mut build_triangles, leaf_size, &mut nodes, &mut triangles)?;
        let node_count = u32::try_from(nodes.len() - first_node as usize)
            .context("V10 cell node count exceeds u32")?;
        cells.push(FptvoxBvhCell {
            coordinate: cell.coordinate,
            cell: cell.cell,
            first_node,
            node_count,
        });
    }
    Ok(FptvoxTriangleBvhSurface {
        resolution: surface.resolution,
        sampling_resolution: surface.sampling_resolution,
        bounds: surface.bounds,
        coordinate_system: surface.coordinate_system,
        cells,
        triangles,
        nodes,
    })
}

#[derive(Clone, Copy)]
struct IndexedBvhBuildReference {
    triangle: u32,
    min: [f32; 3],
    max: [f32; 3],
    centroid: [f32; 3],
}

fn append_stackless_indexed_bvh(
    references: &mut [IndexedBvhBuildReference],
    leaf_size: usize,
    nodes: &mut Vec<FptvoxBvhNode>,
    reordered: &mut Vec<u32>,
) -> Result<()> {
    let node_index = nodes.len();
    nodes.push(FptvoxBvhNode {
        bounds: [0; 6],
        triangle_count: 0,
        first_triangle: 0,
        escape: 0,
    });
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for reference in references.iter() {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(reference.min[axis]);
            maximum[axis] = maximum[axis].max(reference.max[axis]);
        }
    }
    let bounds = quantize_bvh_bounds(minimum, maximum);
    if references.len() <= leaf_size {
        let first_reference =
            u32::try_from(reordered.len()).context("V11 reference offset exceeds u32")?;
        let reference_count =
            u16::try_from(references.len()).context("V11 leaf reference count exceeds u16")?;
        reordered.extend(references.iter().map(|reference| reference.triangle));
        nodes[node_index] = FptvoxBvhNode {
            bounds,
            triangle_count: reference_count,
            first_triangle: first_reference,
            escape: u32::try_from(nodes.len()).context("V11 node count exceeds u32")?,
        };
        return Ok(());
    }

    let extent = std::array::from_fn::<_, 3, _>(|axis| maximum[axis] - minimum[axis]);
    let axis = (0..3)
        .max_by(|left, right| extent[*left].total_cmp(&extent[*right]))
        .expect("three BVH axes");
    references.sort_unstable_by(|left, right| {
        left.centroid[axis]
            .total_cmp(&right.centroid[axis])
            .then_with(|| left.triangle.cmp(&right.triangle))
    });
    let middle = references.len() / 2;
    let (left, right) = references.split_at_mut(middle);
    append_stackless_indexed_bvh(left, leaf_size, nodes, reordered)?;
    append_stackless_indexed_bvh(right, leaf_size, nodes, reordered)?;
    nodes[node_index] = FptvoxBvhNode {
        bounds,
        triangle_count: 0,
        first_triangle: 0,
        escape: u32::try_from(nodes.len()).context("V11 node count exceeds u32")?,
    };
    Ok(())
}

/// Build per-cell stackless BVHs while retaining one global V8 triangle stream.
pub fn build_indexed_triangle_bvh_surface(
    surface: &FptvoxIndexedTriangleSurface,
    leaf_size: usize,
) -> Result<FptvoxIndexedTriangleBvhSurface> {
    ensure!((2..=64).contains(&leaf_size), "V11 leaf size must be 2..64");
    let mut cells = Vec::with_capacity(surface.cells.len());
    let mut references = Vec::with_capacity(surface.references.len());
    let mut nodes = Vec::new();
    for cell in &surface.cells {
        let start = cell.first_reference as usize;
        let end = start + cell.reference_count as usize;
        let cell_origin = cell.coordinate.map(|value| value as f32);
        let mut build_references = surface.references[start..end]
            .iter()
            .map(|triangle_index| {
                let triangle = &surface.triangles[*triangle_index as usize];
                let vertices = triangle.vertices.map(|vertex| {
                    std::array::from_fn::<_, 3, _>(|axis| {
                        f32::from(vertex[axis]) / 65535.0 * surface.resolution[axis] as f32
                            - cell_origin[axis]
                    })
                });
                let min = std::array::from_fn(|axis| {
                    vertices
                        .iter()
                        .map(|vertex| vertex[axis])
                        .fold(f32::INFINITY, f32::min)
                        .clamp(0.0, 1.0)
                });
                let max = std::array::from_fn(|axis| {
                    vertices
                        .iter()
                        .map(|vertex| vertex[axis])
                        .fold(f32::NEG_INFINITY, f32::max)
                        .clamp(0.0, 1.0)
                });
                IndexedBvhBuildReference {
                    triangle: *triangle_index,
                    min,
                    max,
                    centroid: std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5),
                }
            })
            .collect::<Vec<_>>();
        let first_node = u32::try_from(nodes.len()).context("V11 node offset exceeds u32")?;
        append_stackless_indexed_bvh(
            &mut build_references,
            leaf_size,
            &mut nodes,
            &mut references,
        )?;
        let node_count = u32::try_from(nodes.len() - first_node as usize)
            .context("V11 cell node count exceeds u32")?;
        cells.push(FptvoxBvhCell {
            coordinate: cell.coordinate,
            cell: cell.cell,
            first_node,
            node_count,
        });
    }
    Ok(FptvoxIndexedTriangleBvhSurface {
        resolution: surface.resolution,
        sampling_resolution: surface.sampling_resolution,
        bounds: surface.bounds,
        coordinate_system: surface.coordinate_system,
        cells,
        triangles: surface.triangles.clone(),
        triangle_colors: surface.triangle_colors.clone(),
        triangle_vertex_colors: surface.triangle_vertex_colors.clone(),
        references,
        nodes,
    })
}

pub fn build_triangle_surface(
    metallib: &Path,
    config: &FptRenderConfig,
    output_resolution: u32,
    sampling_resolution: u32,
    bounds: Aabb,
    world_scale: f32,
    material: Option<&MandelbulberMaterial>,
    threshold_scale: f32,
) -> Result<Fptvox7Build> {
    build_triangle_surface_with_resolutions(
        metallib,
        config,
        [output_resolution; 3],
        [sampling_resolution; 3],
        bounds,
        world_scale,
        material,
        threshold_scale,
    )
}

pub fn aspect_resolutions(bounds: Aabb, maximum: u32) -> [u32; 3] {
    let span = std::array::from_fn::<_, 3, _>(|axis| bounds.max[axis] - bounds.min[axis]);
    let maximum_span = span.iter().copied().fold(0.0_f32, f32::max);
    std::array::from_fn(|axis| {
        ((span[axis] / maximum_span * maximum as f32).round() as u32).clamp(2, maximum)
    })
}

pub fn mandelbulber_mesh_resolutions(bounds: Aabb, maximum: u32) -> [u32; 3] {
    let span = std::array::from_fn::<_, 3, _>(|axis| {
        f64::from(bounds.max[axis]) - f64::from(bounds.min[axis])
    });
    let maximum_span = span.iter().copied().fold(0.0_f64, f64::max);
    let step = maximum_span / f64::from(maximum);
    std::array::from_fn(|axis| {
        if span[axis] == maximum_span {
            maximum
        } else {
            (span[axis] / step).floor().clamp(1.0, f64::from(maximum)) as u32
        }
    })
}

pub fn build_triangle_surface_anisotropic(
    metallib: &Path,
    config: &FptRenderConfig,
    output_resolution: u32,
    sampling_resolution: u32,
    bounds: Aabb,
    world_scale: f32,
    material: Option<&MandelbulberMaterial>,
    threshold_scale: f32,
) -> Result<Fptvox7Build> {
    build_triangle_surface_with_resolutions(
        metallib,
        config,
        aspect_resolutions(bounds, output_resolution),
        aspect_resolutions(bounds, sampling_resolution),
        bounds,
        world_scale,
        material,
        threshold_scale,
    )
}

pub fn build_triangle_surface_grid(
    metallib: &Path,
    config: &FptRenderConfig,
    output_resolution: [u32; 3],
    sampling_resolution: [u32; 3],
    bounds: Aabb,
    world_scale: f32,
    material: Option<&MandelbulberMaterial>,
    threshold_scale: f32,
) -> Result<Fptvox7Build> {
    build_triangle_surface_with_resolutions(
        metallib,
        config,
        output_resolution,
        sampling_resolution,
        bounds,
        world_scale,
        material,
        threshold_scale,
    )
}

pub fn expand_bounds_preserving_voxel_size(
    bounds: Aabb,
    output_resolution: [u32; 3],
    sampling_resolution: [u32; 3],
    requested_margin: f32,
) -> Result<(Aabb, [u32; 3], [u32; 3], f32)> {
    ensure!(
        requested_margin.is_finite() && (0.001..=1.0).contains(&requested_margin),
        "surface triangle automatic bounds margin must be 0.001..1.0"
    );
    let requested_scale = 1.0 + 2.0 * requested_margin;
    let maximum_scale = output_resolution
        .iter()
        .map(|resolution| 512.0 / *resolution as f32)
        .chain(
            sampling_resolution
                .iter()
                .map(|resolution| 511.0 / *resolution as f32),
        )
        .fold(requested_scale, f32::min);
    let scale = requested_scale.min(maximum_scale);
    ensure!(
        scale > 1.0 + 1.0e-6,
        "surface triangle grids have no capacity for an automatic bounds expansion"
    );
    let effective_margin = 0.5 * (scale - 1.0);
    let span = std::array::from_fn::<_, 3, _>(|axis| bounds.max[axis] - bounds.min[axis]);
    let expanded = Aabb::new(
        std::array::from_fn(|axis| bounds.min[axis] - span[axis] * effective_margin),
        std::array::from_fn(|axis| bounds.max[axis] + span[axis] * effective_margin),
    );
    let scaled_grid = |resolution: [u32; 3], limit: u32| {
        resolution.map(|value| ((value as f32 * scale - 1.0e-5).ceil() as u32).min(limit))
    };
    Ok((
        expanded,
        scaled_grid(output_resolution, 512),
        scaled_grid(sampling_resolution, 511),
        effective_margin,
    ))
}

fn build_triangle_surface_with_resolutions(
    metallib: &Path,
    config: &FptRenderConfig,
    output_resolution: [u32; 3],
    sampling_resolution: [u32; 3],
    bounds: Aabb,
    world_scale: f32,
    material: Option<&MandelbulberMaterial>,
    threshold_scale: f32,
) -> Result<Fptvox7Build> {
    ensure!(
        sampling_resolution
            .iter()
            .all(|resolution| (1..=511).contains(resolution)),
        "V7 sampling resolution must be 1..511"
    );
    ensure!(
        output_resolution
            .iter()
            .all(|resolution| (1..=512).contains(resolution)),
        "V7 output resolution must be 1..512"
    );
    let lattice_resolution = sampling_resolution.map(|resolution| resolution + 1);
    let mut sampling_config = *config;
    sampling_config.voxel_resolution = *lattice_resolution.iter().max().expect("three axes");
    ensure!(
        threshold_scale.is_finite() && (0.25..=4.0).contains(&threshold_scale),
        "V7 topology threshold scale must be 0.25..4"
    );
    if config.vset_values[115] != 0.0 {
        sampling_config.vset_values[129] = config.vset_values[129].max(1.0e-7) / threshold_scale;
    }
    let maximum_span = bounds
        .max
        .iter()
        .zip(bounds.min)
        .map(|(max, min)| (max - min) * world_scale)
        .fold(0.0_f32, f32::max);
    let maximum_resolution = *sampling_resolution.iter().max().expect("three axes");
    let maximum_step = maximum_span / maximum_resolution as f32;
    let threshold = if config.vset_values[115] != 0.0 {
        0.5 * maximum_step / config.vset_values[129].max(1.0e-7)
    } else {
        config.vset_values[117].max(1.0e-7)
    } * threshold_scale;
    // The mesh-specialized Delta-DE source derives its probe scale from the
    // effective detail size, including dynamic-threshold and CLI scaling.
    sampling_config.vset_values[117] = threshold;
    // mesh_export.cpp extends the lower bound by one step plus the iso
    // threshold, then reconstructs the upper bound from N uniform steps.
    // The resulting lattice is shifted rather than symmetrically expanded.
    let extension = maximum_step + threshold;
    sampling_config.voxel_bounds_min = bounds.min.map(|value| value * world_scale - extension);
    sampling_config.voxel_bounds_max = std::array::from_fn(|axis| {
        sampling_config.voxel_bounds_min[axis] + maximum_step * sampling_resolution[axis] as f32
    });
    let output_offset = std::array::from_fn(|axis| {
        -extension / ((bounds.max[axis] - bounds.min[axis]) * world_scale)
    });
    let output_step = std::array::from_fn(|axis| {
        maximum_step / ((bounds.max[axis] - bounds.min[axis]) * world_scale)
    });
    let (
        cell_triangles,
        topology_policy,
        topology_fallback_offset,
        topology_ms,
        marching_cubes_ms,
        source_triangles,
        clipping_ms,
    ) = {
        let sample_count = lattice_resolution
            .iter()
            .try_fold(1_usize, |count, resolution| {
                count
                    .checked_mul(*resolution as usize)
                    .context("V7 sample count overflow")
            })?;
        let mut field = vec![0.0_f32; sample_count];
        let mut lattice_colors = vec![0.0_f32; sample_count];
        let mut topology_ms = 0.0_f64;
        let mut error = [0_i8; 1024];
        let status = unsafe {
            fpt_metal_sample_topology_grid(
                c_path(metallib)?.as_ptr(),
                &sampling_config,
                lattice_resolution[0],
                lattice_resolution[1],
                lattice_resolution[2],
                field.as_mut_ptr(),
                field.len(),
                lattice_colors.as_mut_ptr(),
                lattice_colors.len(),
                &mut topology_ms,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        ensure!(
            status == 0,
            "Metal topology sampling failed: {}",
            bridge_error(&error)
        );
        ensure!(
            field.iter().all(|value| value.is_finite()),
            "Metal topology grid contains non-finite samples"
        );
        let field_min = field.iter().copied().fold(f32::INFINITY, f32::min);
        let field_max = field.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut policy = "mandelbulber-distance-threshold".to_string();
        let mut fallback = 0.0_f32;
        if config.sdf_id == crate::ffi::SDF_MANDELBULBER && field_min >= 0.0 {
            fallback = field_min + threshold;
            for sample in &mut field {
                *sample -= fallback;
            }
            policy = "distance-floor-fallback".to_string();
        }
        let marching_started = Instant::now();
        let triangles = extract_surface(
            &field,
            &lattice_colors,
            lattice_resolution.map(|resolution| resolution as usize),
            output_offset,
            output_step,
            material,
        );
        let marching_ms = marching_started.elapsed().as_secs_f64() * 1000.0;
        let clipping_started = Instant::now();
        let cells = clip_surface(&triangles, output_resolution);
        let clipping_ms = clipping_started.elapsed().as_secs_f64() * 1000.0;
        ensure!(
            !cells.is_empty(),
            "V7 marching cubes produced no surface cells (sample range {field_min}..{field_max})"
        );
        (
            cells,
            policy,
            fallback,
            topology_ms,
            marching_ms,
            triangles.len() as u64,
            clipping_ms,
        )
    };
    ensure!(
        !cell_triangles.is_empty(),
        "V7 marching cubes produced no surface cells"
    );
    let (boundary_cells, boundary_face_cells) = boundary_cell_counts(
        cell_triangles.values().map(|surface| &surface.coordinate),
        output_resolution,
    );

    let material_started = Instant::now();
    let mut points = Vec::<[f32; 4]>::with_capacity(cell_triangles.len());
    for surface in cell_triangles.values() {
        let coordinate = surface.coordinate;
        points.push([
            (bounds.min[0]
                + (coordinate[0] as f32 + 0.5) / output_resolution[0] as f32
                    * (bounds.max[0] - bounds.min[0]))
                * world_scale,
            (bounds.min[1]
                + (coordinate[1] as f32 + 0.5) / output_resolution[1] as f32
                    * (bounds.max[1] - bounds.min[1]))
                * world_scale,
            (bounds.min[2]
                + (coordinate[2] as f32 + 0.5) / output_resolution[2] as f32
                    * (bounds.max[2] - bounds.min[2]))
                * world_scale,
            0.0,
        ]);
    }
    let mut materials = vec![VoxelCell::default(); points.len()];
    let mut error = [0_i8; 1024];
    let status = unsafe {
        fpt_metal_sample_materials(
            c_path(metallib)?.as_ptr(),
            &sampling_config,
            points.as_ptr().cast(),
            points.len(),
            materials.as_mut_ptr().cast(),
            materials.len() * std::mem::size_of::<VoxelCell>(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    ensure!(
        status == 0,
        "Metal material sampling failed: {}",
        bridge_error(&error)
    );
    let metal_material_ms = material_started.elapsed().as_secs_f64() * 1000.0;

    let mut cells = Vec::with_capacity(cell_triangles.len());
    let mut triangles = Vec::new();
    for (((_, surface), mut material), _) in cell_triangles.into_iter().zip(materials).zip(0..) {
        let coordinate = surface.coordinate;
        let color = surface.primary_color();
        let local_triangles = surface.triangles;
        let quantize = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
        material.packed_color = quantize(color[0])
            | (quantize(color[1]) << 8)
            | (quantize(color[2]) << 16)
            | VoxelCell::OCCUPIED_MASK;
        let first_triangle =
            u32::try_from(triangles.len()).context("V7 triangle offset exceeds u32")?;
        let triangle_count =
            u32::try_from(local_triangles.len()).context("V7 cell triangle count exceeds u32")?;
        triangles.extend(local_triangles);
        cells.push(FptvoxTriangleCell {
            coordinate,
            cell: material,
            first_triangle,
            triangle_count,
        });
    }
    let clipped_triangles = triangles.len() as u64;
    Ok(Fptvox7Build {
        surface: FptvoxTriangleSurface {
            resolution: output_resolution,
            sampling_resolution,
            bounds,
            coordinate_system: CoordinateSystem::YUpRightHanded,
            cells,
            triangles,
        },
        summary: Fptvox7BuildSummary {
            topology_policy,
            topology_fallback_offset,
            topology_threshold_scale: threshold_scale,
            metal_topology_ms: topology_ms,
            marching_cubes_ms,
            clipping_ms,
            metal_material_ms,
            source_triangles,
            clipped_triangles,
            occupied_cells: points.len(),
            boundary_cells,
            boundary_face_cells,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ray_triangle(origin: [f32; 3], direction: [f32; 3], triangle: [[f32; 3]; 3]) -> Option<f32> {
        let subtract = |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|axis| a[axis] - b[axis]);
        let cross = |a: [f32; 3], b: [f32; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let dot = |a: [f32; 3], b: [f32; 3]| {
            a.iter()
                .zip(b)
                .map(|(left, right)| left * right)
                .sum::<f32>()
        };
        let edge_a = subtract(triangle[1], triangle[0]);
        let edge_b = subtract(triangle[2], triangle[0]);
        let p = cross(direction, edge_b);
        let determinant = dot(edge_a, p);
        if determinant.abs() <= 1.0e-8 {
            return None;
        }
        let inverse = determinant.recip();
        let offset = subtract(origin, triangle[0]);
        let u = dot(offset, p) * inverse;
        if !(-1.0e-5..=1.00001).contains(&u) {
            return None;
        }
        let q = cross(offset, edge_a);
        let v = dot(direction, q) * inverse;
        if v < -1.0e-5 || u + v > 1.00001 {
            return None;
        }
        let depth = dot(edge_b, q) * inverse;
        (depth >= -1.0e-4).then_some(depth)
    }

    #[test]
    fn sphere_field_extracts_sorted_bounded_triangles() {
        let resolution = 20_usize;
        let divisor = (resolution - 1) as f32;
        let mut field = Vec::with_capacity(resolution.pow(3));
        for z in 0..resolution {
            for y in 0..resolution {
                for x in 0..resolution {
                    let p = [
                        x as f32 / divisor - 0.5,
                        y as f32 / divisor - 0.5,
                        z as f32 / divisor - 0.5,
                    ];
                    field.push((p.iter().map(|value| value * value).sum::<f32>()).sqrt() - 0.3);
                }
            }
        }
        let colors = vec![0.0; field.len()];
        let source_triangles = extract_surface(
            &field,
            &colors,
            [resolution; 3],
            [0.0; 3],
            [1.0 / (resolution - 1) as f32; 3],
            None,
        );
        let cells = clip_surface(&source_triangles, [16; 3]);
        assert!(source_triangles.len() > 100);
        assert!(cells.len() > 100);
        assert!(cells.values().all(|cell| !cell.triangles.is_empty()));
        assert!(
            cells
                .values()
                .flat_map(|cell| &cell.triangles)
                .flat_map(|triangle| triangle.vertices)
                .all(|vertex| vertex >> 30 == 0)
        );
    }

    #[test]
    fn anisotropic_resolutions_preserve_bounds_aspect_ratio() {
        let bounds = Aabb::new([-1.0, -0.5, -0.25], [1.0, 0.5, 0.25]);
        assert_eq!(aspect_resolutions(bounds, 192), [192, 96, 48]);

        let thin = Aabb::new([0.0, 0.0, 0.0], [8.0, 0.001, 4.0]);
        assert_eq!(aspect_resolutions(thin, 128), [128, 2, 64]);
    }

    #[test]
    fn mandelbulber_mesh_resolutions_preserve_uniform_step_truncation() {
        let bounds = Aabb::new(
            [
                0.0004729109350591898,
                -1.3265780210494995,
                -5.904452323913574,
            ],
            [4.048012733459473, 2.7209620475769043, -1.8569122552871704],
        );
        assert_eq!(mandelbulber_mesh_resolutions(bounds, 96), [95, 96, 96]);

        let equal = Aabb::new(
            [1.680698275566101, 2.0853164196014404, -3.2698488235473633],
            [1.7585076093673706, 2.16312575340271, -3.1920394897460938],
        );
        assert_eq!(mandelbulber_mesh_resolutions(equal, 96), [96; 3]);
    }

    #[test]
    fn boundary_counts_deduplicate_cells_touching_multiple_faces() {
        let coordinates = [[0, 0, 0], [3, 3, 3], [1, 1, 1]];
        let (boundary_cells, faces) = boundary_cell_counts(coordinates.iter(), [4, 4, 4]);
        assert_eq!(boundary_cells, 2);
        assert_eq!(faces, [1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn automatic_bounds_expansion_preserves_voxel_size_and_respects_limits() {
        let bounds = Aabb::new([-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]);
        let (expanded, output, sampling, margin) =
            expand_bounds_preserving_voxel_size(bounds, [100, 80, 60], [200, 160, 120], 0.1)
                .expect("expanded bounds");
        assert_eq!(output, [120, 96, 72]);
        assert_eq!(sampling, [240, 192, 144]);
        assert!((margin - 0.1).abs() < 1.0e-6);
        for (actual, expected) in expanded.min.into_iter().zip([-1.2, -2.4, -3.6]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }
        for (actual, expected) in expanded.max.into_iter().zip([1.2, 2.4, 3.6]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }

        let (_, output, sampling, margin) =
            expand_bounds_preserving_voxel_size(bounds, [480; 3], [500; 3], 0.1)
                .expect("capacity-limited expansion");
        assert_eq!(output, [491; 3]);
        assert_eq!(sampling, [511; 3]);
        assert!((margin - 0.011).abs() < 1.0e-5);
    }

    #[test]
    fn exact_triangle_transport_handles_boundaries_ties_and_negative_rays() {
        let triangle = [[0.0, 0.0, 0.5], [1.0, 0.0, 0.5], [0.0, 1.0, 0.5]];
        let cases = [
            ([0.25, 0.25, -1.0], [0.0, 0.0, 1.0], 1.5),
            ([0.5, 0.5, -1.0], [0.0, 0.0, 1.0], 1.5),
            ([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], 1.5),
            ([0.25, 0.25, 0.5], [0.0, 0.0, 1.0], 0.0),
            ([0.25, 0.25, 2.0], [0.0, 0.0, -1.0], 1.5),
        ];
        for (origin, direction, expected) in cases {
            let depth = ray_triangle(origin, direction, triangle).expect("probe must hit");
            assert!(
                (depth - expected).abs() <= 1.0e-6,
                "{origin:?} -> {direction:?}"
            );
        }
        assert!(ray_triangle([1.01, 1.01, -1.0], [0.0, 0.0, 1.0], triangle).is_none());
        assert!(ray_triangle([0.25, 0.25, 0.5], [1.0, 0.0, 0.0], triangle).is_none());
    }

    #[test]
    fn normalized_mesh_uses_exact_v7_cell_clipping_and_material_color() {
        let red = [0.75, 0.25, 0.125];
        let triangle = [
            MeshSurfaceVertex {
                position: [0.2, 0.2, 0.5],
                color: red,
            },
            MeshSurfaceVertex {
                position: [0.8, 0.2, 0.5],
                color: red,
            },
            MeshSurfaceVertex {
                position: [0.2, 0.8, 0.5],
                color: red,
            },
        ];
        let surface = build_triangle_surface_from_normalized_mesh(
            [triangle],
            1,
            8,
            16,
            Aabb::new([-1.0; 3], [1.0; 3]),
            SurfaceMaterial::default(),
        )
        .expect("build normalized triangle surface");
        assert!(!surface.cells.is_empty());
        assert!(!surface.triangles.is_empty());
        assert_eq!(surface.resolution, [8; 3]);
        assert!(surface.cells.windows(2).all(|pair| {
            let linear = |cell: &FptvoxTriangleCell| {
                cell.coordinate[0] + cell.coordinate[1] * 8 + cell.coordinate[2] * 8 * 8
            };
            linear(&pair[0]) < linear(&pair[1])
        }));
        let packed = surface.cells[0].cell.packed_color;
        assert_eq!(packed & 255, 191);
        assert_eq!((packed >> 8) & 255, 64);
        assert_eq!((packed >> 16) & 255, 32);
    }

    #[test]
    fn indexed_mesh_reuses_one_source_triangle_across_cells() {
        let triangle = [
            MeshSurfaceVertex {
                position: [0.05, 0.05, 0.5],
                color: [0.5; 3],
            },
            MeshSurfaceVertex {
                position: [0.95, 0.05, 0.5],
                color: [0.5; 3],
            },
            MeshSurfaceVertex {
                position: [0.05, 0.95, 0.5],
                color: [0.5; 3],
            },
        ];
        let clipped = build_triangle_surface_from_normalized_mesh(
            [triangle],
            1,
            8,
            16,
            Aabb::new([-1.0; 3], [1.0; 3]),
            SurfaceMaterial::default(),
        )
        .expect("build clipped surface");
        let indexed = build_indexed_triangle_surface_from_normalized_mesh_3d(
            [triangle],
            [8; 3],
            [16; 3],
            Aabb::new([-1.0; 3], [1.0; 3]),
            SurfaceMaterial::default(),
        )
        .expect("build indexed surface");
        assert_eq!(indexed.triangles.len(), 1);
        assert_eq!(indexed.references.len(), indexed.cells.len());
        assert!(indexed.references.len() < clipped.triangles.len());
        assert!(indexed.references.iter().all(|reference| *reference == 0));
    }
}
