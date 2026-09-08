use fpt_metal::{
    Aabb, CoordinateSystem, FPTVOX_APPEARANCE_MAGIC, FPTVOX_APPEARANCE_SIZE,
    FPTVOX_APPEARANCE_VERSION, FPTVOX_BOUNDED_PATCH_MAGIC, FPTVOX_BOUNDED_PATCH_RECORD_SIZE,
    FPTVOX_BOUNDED_PATCH_VERSION, FPTVOX_CAMERA_MAGIC, FPTVOX_CAMERA_SIZE,
    FPTVOX_ENVIRONMENT_HEADER_SIZE, FPTVOX_ENVIRONMENT_LUT_VALUES, FPTVOX_ENVIRONMENT_MAGIC,
    FPTVOX_ENVIRONMENT_SIZE, FPTVOX_HEADER_SIZE, FPTVOX_INDEXED_TRIANGLE_BVH_CELL_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_HEADER_SIZE, FPTVOX_INDEXED_TRIANGLE_BVH_MAGIC,
    FPTVOX_INDEXED_TRIANGLE_BVH_NODE_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_REFERENCE_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_BVH_VERSION,
    FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE,
    FPTVOX_INDEXED_TRIANGLE_MAGIC, FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL,
    FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE,
    FPTVOX_INDEXED_TRIANGLE_VERSION, FPTVOX_MAGIC, FPTVOX_MATERIAL_HEADER_SIZE,
    FPTVOX_MATERIAL_MAGIC, FPTVOX_MATERIAL_RECORD_SIZE, FPTVOX_PLANE_MAGIC,
    FPTVOX_PLANE_PAIR_MAGIC, FPTVOX_PLANE_PAIR_RECORD_SIZE, FPTVOX_PLANE_PAIR_VERSION,
    FPTVOX_PLANE_RECORD_SIZE, FPTVOX_PLANE_VERSION, FPTVOX_RECORD_SIZE, FPTVOX_SURFACE_MAGIC,
    FPTVOX_SURFACE_RECORD_SIZE, FPTVOX_SURFACE_VERSION, FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE,
    FPTVOX_TRIANGLE_BVH_HEADER_SIZE, FPTVOX_TRIANGLE_BVH_MAGIC,
    FPTVOX_TRIANGLE_BVH_NODE_RECORD_SIZE, FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE,
    FPTVOX_TRIANGLE_BVH_VERSION, FPTVOX_TRIANGLE_CELL_RECORD_SIZE,
    FPTVOX_TRIANGLE_COLOR_HEADER_SIZE, FPTVOX_TRIANGLE_COLOR_MAGIC,
    FPTVOX_TRIANGLE_COLOR_RECORD_SIZE, FPTVOX_TRIANGLE_COLOR_VERSION, FPTVOX_TRIANGLE_HEADER_SIZE,
    FPTVOX_TRIANGLE_MAGIC, FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE, FPTVOX_TRIANGLE_MATERIAL_MAGIC,
    FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE, FPTVOX_TRIANGLE_MATERIAL_VERSION,
    FPTVOX_TRIANGLE_NORMAL_HEADER_SIZE, FPTVOX_TRIANGLE_NORMAL_MAGIC,
    FPTVOX_TRIANGLE_NORMAL_RECORD_SIZE, FPTVOX_TRIANGLE_NORMAL_VERSION,
    FPTVOX_TRIANGLE_RECORD_SIZE, FPTVOX_TRIANGLE_VERSION, FPTVOX_TRIANGLE_VERTEX_COLOR_HEADER_SIZE,
    FPTVOX_TRIANGLE_VERTEX_COLOR_MAGIC, FPTVOX_TRIANGLE_VERTEX_COLOR_RECORD_SIZE,
    FPTVOX_TRIANGLE_VERTEX_COLOR_VERSION, FPTVOX_VERSION, FptvoxAppearance, FptvoxAuthoredMaterial,
    FptvoxBvhCell, FptvoxBvhNode, FptvoxBvhTriangle, FptvoxCamera, FptvoxEnvironment,
    FptvoxIndexedTriangle, FptvoxIndexedTriangleBvhSurface, FptvoxIndexedTriangleCell,
    FptvoxIndexedTriangleSurface, FptvoxTriangle, FptvoxTriangleBvhSurface, FptvoxTriangleCell,
    FptvoxTriangleSurface, FractalErrorCode, SparseVoxel, SurfaceMaterial, VoxelCell, VoxelGrid,
    append_fptvox_appearance, append_fptvox_camera, append_fptvox_environment,
    append_fptvox_materials, append_fptvox_triangle_colors, append_fptvox_triangle_material_ids,
    append_fptvox_triangle_normals, append_fptvox_triangle_vertex_colors, export_fptvox,
    export_fptvox_indexed_triangle_bvh_surface, export_fptvox_indexed_triangle_surface,
    export_fptvox_triangle_bvh_surface, export_fptvox_triangle_surface,
    export_fptvox_with_bounded_patches, export_fptvox_with_normals, export_fptvox_with_plane_pairs,
    export_fptvox_with_planes,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct DecodedFptvox {
    resolution: [u32; 3],
    coordinate_system: u32,
    bounds: Aabb,
    voxels: Vec<SparseVoxel>,
}

#[test]
fn camera_environment_and_material_trailers_are_byte_exact() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("authored-scene-trailers");
    let geometry = export_fptvox(&grid, &output).unwrap();
    let camera = FptvoxCamera {
        position: [1.0, 2.0, 3.0],
        yaw_pitch: [0.25, -0.5],
        roll: 0.75,
        fov_degrees: 61.0,
        image_y_sign: -1.0,
        projection: 2,
    };
    append_fptvox_camera(&output, &camera).unwrap();
    let mut environment = FptvoxEnvironment::default();
    environment.flags = 0x1f;
    environment.hdri_map_type = 2;
    environment.values[9] = 12.5;
    environment.hdri_lut[7] = 0x3c00;
    append_fptvox_environment(&output, &environment).unwrap();
    let material = FptvoxAuthoredMaterial {
        id: 3,
        flags: 1,
        base_color: [0.1, 0.2, 0.3],
        roughness: 0.4,
        specular: 0.5,
        specular_width: 0.6,
        metallic: 0.7,
        reflectance: 0.8,
        transmission: 0.9,
        interior_opacity: 1.1,
        ior: 1.45,
        emission: 2.0,
        transmission_color: [0.3, 0.4, 0.5],
    };
    append_fptvox_materials(&output, &[material]).unwrap();
    let bytes = fs::read(&output).unwrap();
    let camera_offset = geometry.bytes as usize;
    assert_eq!(bytes[camera_offset..camera_offset + 8], FPTVOX_CAMERA_MAGIC);
    assert_eq!(u32_at(&bytes, camera_offset + 8), FPTVOX_CAMERA_SIZE);
    let environment_offset = camera_offset + FPTVOX_CAMERA_SIZE as usize;
    assert_eq!(
        bytes[environment_offset..environment_offset + 8],
        FPTVOX_ENVIRONMENT_MAGIC
    );
    assert_eq!(
        u32_at(&bytes, environment_offset + 8),
        FPTVOX_ENVIRONMENT_SIZE
    );
    assert_eq!(
        u32_at(&bytes, environment_offset + 32 + 9 * 4),
        12.5f32.to_bits()
    );
    let lut_offset = environment_offset + FPTVOX_ENVIRONMENT_HEADER_SIZE as usize;
    assert_eq!(
        &bytes[lut_offset + 14..lut_offset + 16],
        &0x3c00u16.to_le_bytes()
    );
    let material_offset = environment_offset + FPTVOX_ENVIRONMENT_SIZE as usize;
    assert_eq!(
        bytes[material_offset..material_offset + 8],
        FPTVOX_MATERIAL_MAGIC
    );
    assert_eq!(
        u32_at(&bytes, material_offset + 8),
        FPTVOX_MATERIAL_HEADER_SIZE
    );
    assert_eq!(
        u32_at(&bytes, material_offset + 16),
        FPTVOX_MATERIAL_RECORD_SIZE
    );
    assert_eq!(u32_at(&bytes, material_offset + 32), 3);
    assert_eq!(
        bytes.len(),
        material_offset
            + FPTVOX_MATERIAL_HEADER_SIZE as usize
            + FPTVOX_MATERIAL_RECORD_SIZE as usize
    );
    assert_eq!(environment.hdri_lut.len(), FPTVOX_ENVIRONMENT_LUT_VALUES);
    fs::remove_file(output).unwrap();
}

#[test]
fn triangle_color_trailer_is_fixed_size_and_byte_exact() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("triangle-colors");
    let geometry = export_fptvox(&grid, &output).unwrap();
    let colors = [0x0033_2211, 0x00cc_bbaa];
    let expected_bytes = u64::from(FPTVOX_TRIANGLE_COLOR_HEADER_SIZE)
        + colors.len() as u64 * u64::from(FPTVOX_TRIANGLE_COLOR_RECORD_SIZE);
    assert_eq!(
        append_fptvox_triangle_colors(&output, &colors).unwrap(),
        expected_bytes
    );
    let bytes = fs::read(&output).unwrap();
    let offset = geometry.bytes as usize;
    assert_eq!(bytes.len(), offset + expected_bytes as usize);
    assert_eq!(bytes[offset..offset + 8], FPTVOX_TRIANGLE_COLOR_MAGIC);
    assert_eq!(
        u32_at(&bytes, offset + 8),
        FPTVOX_TRIANGLE_COLOR_HEADER_SIZE
    );
    assert_eq!(u32_at(&bytes, offset + 12), FPTVOX_TRIANGLE_COLOR_VERSION);
    assert_eq!(
        u32_at(&bytes, offset + 16),
        FPTVOX_TRIANGLE_COLOR_RECORD_SIZE
    );
    assert_eq!(u32_at(&bytes, offset + 20), 0);
    assert_eq!(u64_at(&bytes, offset + 24), colors.len() as u64);
    assert_eq!(u32_at(&bytes, offset + 32), colors[0]);
    assert_eq!(u32_at(&bytes, offset + 36), colors[1]);
    fs::remove_file(output).unwrap();
}

#[test]
fn triangle_vertex_color_trailer_preserves_all_three_vertices() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("triangle-vertex-colors");
    let geometry = export_fptvox(&grid, &output).unwrap();
    let colors = [[0x0000_00ff, 0x0000_ff00, 0x00ff_0000]];
    append_fptvox_triangle_vertex_colors(&output, &colors).unwrap();
    let bytes = fs::read(&output).unwrap();
    let offset = geometry.bytes as usize;
    assert_eq!(
        bytes[offset..offset + 8],
        FPTVOX_TRIANGLE_VERTEX_COLOR_MAGIC
    );
    assert_eq!(
        u32_at(&bytes, offset + 8),
        FPTVOX_TRIANGLE_VERTEX_COLOR_HEADER_SIZE
    );
    assert_eq!(
        u32_at(&bytes, offset + 12),
        FPTVOX_TRIANGLE_VERTEX_COLOR_VERSION
    );
    assert_eq!(
        u32_at(&bytes, offset + 16),
        FPTVOX_TRIANGLE_VERTEX_COLOR_RECORD_SIZE
    );
    for (vertex, color) in colors[0].iter().enumerate() {
        assert_eq!(u32_at(&bytes, offset + 32 + vertex * 4), *color);
    }
    fs::remove_file(output).unwrap();
}

#[test]
fn triangle_material_trailer_is_fixed_size_and_byte_exact() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("triangle-materials");
    let geometry = export_fptvox(&grid, &output).unwrap();
    let material_ids = [3u32, 7u32];
    let expected_bytes = u64::from(FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE)
        + material_ids.len() as u64 * u64::from(FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE);
    assert_eq!(
        append_fptvox_triangle_material_ids(&output, &material_ids).unwrap(),
        expected_bytes
    );
    let bytes = fs::read(&output).unwrap();
    let offset = geometry.bytes as usize;
    assert_eq!(bytes[offset..offset + 8], FPTVOX_TRIANGLE_MATERIAL_MAGIC);
    assert_eq!(
        u32_at(&bytes, offset + 8),
        FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE
    );
    assert_eq!(
        u32_at(&bytes, offset + 12),
        FPTVOX_TRIANGLE_MATERIAL_VERSION
    );
    assert_eq!(
        u32_at(&bytes, offset + 16),
        FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE
    );
    assert_eq!(u32_at(&bytes, offset + 20), 0);
    assert_eq!(u64_at(&bytes, offset + 24), material_ids.len() as u64);
    assert_eq!(u32_at(&bytes, offset + 32), 3);
    assert_eq!(u32_at(&bytes, offset + 36), 7);
    fs::remove_file(output).unwrap();
}

#[test]
fn appearance_trailer_is_fixed_size_and_byte_exact() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("appearance");
    let geometry = export_fptvox(&grid, &output).unwrap();
    let appearance = FptvoxAppearance {
        flags: 0x35,
        background_colors: [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6], [0.7, 0.8, 0.9]],
        background_brightness: 1.25,
        background_gamma: 2.0,
        main_light_direction: [1.0, 0.0, -1.0],
        main_light_intensity: 3.0,
        main_light_color: [0.8, 0.7, 0.6],
        main_light_soft_shadow_radians: 0.05,
        auxiliary_light_position: [2.0, 3.0, 4.0],
        auxiliary_light_intensity: 5.0,
        auxiliary_light_color: [0.3, 0.4, 0.5],
        image_gamma: 2.2,
        image_brightness: 1.1,
        image_contrast: 0.9,
        image_saturation: 1.2,
        material_shading: 0.75,
        material_specular: 4.0,
        material_specular_width: 0.1,
        material_roughness: 0.02,
        material_reflectance: 0.25,
        secondary_environment_strength: 0.45,
        primary_surface_triangle_count: 1234,
    };
    assert_eq!(append_fptvox_appearance(&output, &appearance).unwrap(), 256);
    let bytes = fs::read(&output).unwrap();
    let offset = geometry.bytes as usize;
    assert_eq!(bytes.len(), offset + FPTVOX_APPEARANCE_SIZE as usize);
    assert_eq!(bytes[offset..offset + 8], FPTVOX_APPEARANCE_MAGIC);
    assert_eq!(u32_at(&bytes, offset + 8), FPTVOX_APPEARANCE_SIZE);
    assert_eq!(u32_at(&bytes, offset + 12), FPTVOX_APPEARANCE_VERSION);
    assert_eq!(u32_at(&bytes, offset + 16), appearance.flags);
    assert_eq!(f32_at(&bytes, offset + 24), 0.1);
    assert_eq!(f32_at(&bytes, offset + 24 + 34 * 4), 0.25);
    assert_eq!(f32_at(&bytes, offset + 24 + 35 * 4), 0.45);
    assert_eq!(f32_at(&bytes, offset + 24 + 36 * 4), 1234.0);
    assert!(bytes[offset + 24 + 41 * 4..].iter().all(|byte| *byte == 0));
    fs::remove_file(output).unwrap();
}

fn temporary_artifact(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("fpt-{name}-{}-{nonce}.fptvox", std::process::id()))
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(u32_at(bytes, offset))
}

fn decode_for_test(bytes: &[u8]) -> DecodedFptvox {
    assert!(bytes.len() >= FPTVOX_HEADER_SIZE as usize);
    assert_eq!(bytes[0..8], FPTVOX_MAGIC);
    assert_eq!(u32_at(bytes, 8), FPTVOX_HEADER_SIZE);
    assert_eq!(u32_at(bytes, 12), FPTVOX_VERSION);
    let resolution = [u32_at(bytes, 16), u32_at(bytes, 20), u32_at(bytes, 24)];
    let coordinate_system = u32_at(bytes, 28);
    let bounds = Aabb::new(
        [f32_at(bytes, 32), f32_at(bytes, 36), f32_at(bytes, 40)],
        [f32_at(bytes, 44), f32_at(bytes, 48), f32_at(bytes, 52)],
    );
    let voxel_count = usize::try_from(u64_at(bytes, 56)).unwrap();
    assert_eq!(
        bytes.len(),
        FPTVOX_HEADER_SIZE as usize + voxel_count * FPTVOX_RECORD_SIZE as usize
    );
    let mut voxels = Vec::with_capacity(voxel_count);
    for index in 0..voxel_count {
        let offset = FPTVOX_HEADER_SIZE as usize + index * FPTVOX_RECORD_SIZE as usize;
        voxels.push(SparseVoxel {
            coordinate: [
                u32_at(bytes, offset),
                u32_at(bytes, offset + 4),
                u32_at(bytes, offset + 8),
            ],
            cell: VoxelCell {
                packed_color: u32_at(bytes, offset + 12),
                packed_properties: u32_at(bytes, offset + 16),
                emission: f32_at(bytes, offset + 20),
            },
        });
    }
    DecodedFptvox {
        resolution,
        coordinate_system,
        bounds,
        voxels,
    }
}

fn fixture_grid(voxels: Vec<SparseVoxel>, resolution: [u32; 3]) -> VoxelGrid {
    VoxelGrid {
        contract_version: 1,
        resolution,
        bounds: Aabb::new([-1.0, -2.0, -3.0], [4.0, 5.0, 6.0]),
        coordinate_system: CoordinateSystem::YUpRightHanded,
        source_label: "fptvox-test".into(),
        source_sha256: "not-serialized".into(),
        voxels,
    }
}

#[test]
fn header_and_record_are_byte_exact() {
    let voxel = SparseVoxel {
        coordinate: [1, 2, 3],
        cell: VoxelCell {
            packed_color: 0x8122_3344,
            packed_properties: 0xaabb_ccdd,
            emission: 1.5,
        },
    };
    let grid = fixture_grid(vec![voxel], [2, 3, 4]);
    let output = temporary_artifact("exact");
    let summary = export_fptvox(&grid, &output).unwrap();
    let bytes = fs::read(&output).unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(b"FPTVOX1\0");
    expected.extend_from_slice(&64_u32.to_le_bytes());
    expected.extend_from_slice(&1_u32.to_le_bytes());
    for value in [2_u32, 3, 4, 1] {
        expected.extend_from_slice(&value.to_le_bytes());
    }
    for value in [-1.0_f32, -2.0, -3.0, 4.0, 5.0, 6.0] {
        expected.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    expected.extend_from_slice(&1_u64.to_le_bytes());
    for value in [1_u32, 2, 3, 0x8122_3344, 0xaabb_ccdd, 1.5_f32.to_bits()] {
        expected.extend_from_slice(&value.to_le_bytes());
    }
    assert_eq!(bytes, expected);
    assert_eq!(summary.voxel_count, 1);
    assert_eq!(summary.bytes, 88);
    fs::remove_file(output).unwrap();
}

#[test]
fn triangle_surface_is_byte_exact_and_versioned() {
    let cell = VoxelCell {
        packed_color: 0x8122_3344,
        packed_properties: 0xaabb_ccdd,
        emission: 1.5,
    };
    let surface = FptvoxTriangleSurface {
        resolution: [2, 3, 4],
        sampling_resolution: [5, 6, 7],
        bounds: Aabb::new([-1.0, -2.0, -3.0], [4.0, 5.0, 6.0]),
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells: vec![FptvoxTriangleCell {
            coordinate: [1, 2, 3],
            cell,
            first_triangle: 0,
            triangle_count: 1,
        }],
        triangles: vec![FptvoxTriangle {
            vertices: [0, 1023, 1023 << 10],
        }],
    };
    let output = temporary_artifact("triangle-exact");
    let summary = export_fptvox_triangle_surface(&surface, &output).unwrap();
    let bytes = fs::read(&output).unwrap();

    assert_eq!(bytes[0..8], FPTVOX_TRIANGLE_MAGIC);
    assert_eq!(u32_at(&bytes, 8), FPTVOX_TRIANGLE_HEADER_SIZE);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_TRIANGLE_VERSION);
    assert_eq!(
        [u32_at(&bytes, 16), u32_at(&bytes, 20), u32_at(&bytes, 24)],
        [2, 3, 4]
    );
    assert_eq!(
        [u32_at(&bytes, 64), u32_at(&bytes, 68), u32_at(&bytes, 72)],
        [5, 6, 7]
    );
    assert_eq!(u32_at(&bytes, 76), FPTVOX_TRIANGLE_CELL_RECORD_SIZE);
    assert_eq!(u64_at(&bytes, 80), 1);
    assert_eq!(u32_at(&bytes, 88), FPTVOX_TRIANGLE_RECORD_SIZE);
    assert_eq!(u32_at(&bytes, 92), 0);
    let cell_offset = FPTVOX_TRIANGLE_HEADER_SIZE as usize;
    assert_eq!(u32_at(&bytes, cell_offset + 12), 0x8122_3344);
    assert_eq!(u32_at(&bytes, cell_offset + 16), 0xaabb_ccdd);
    assert_eq!(f32_at(&bytes, cell_offset + 20), 1.5);
    assert_eq!(u32_at(&bytes, cell_offset + 24), 0);
    assert_eq!(u32_at(&bytes, cell_offset + 28), 1);
    let triangle_offset = cell_offset + FPTVOX_TRIANGLE_CELL_RECORD_SIZE as usize;
    assert_eq!(
        [
            u32_at(&bytes, triangle_offset),
            u32_at(&bytes, triangle_offset + 4),
            u32_at(&bytes, triangle_offset + 8)
        ],
        [0, 1023, 1023 << 10]
    );
    assert_eq!(summary.voxel_count, 1);
    assert_eq!(summary.bytes, 140);
    assert_eq!(bytes.len(), 140);
    fs::remove_file(output).unwrap();
}

#[test]
fn indexed_triangle_surface_is_byte_exact_and_versioned() {
    let cell = VoxelCell {
        packed_color: 0x8122_3344,
        packed_properties: 0xaabb_ccdd,
        emission: 1.5,
    };
    let surface = FptvoxIndexedTriangleSurface {
        resolution: [2, 3, 4],
        sampling_resolution: [5, 6, 7],
        bounds: Aabb::new([-1.0, -2.0, -3.0], [4.0, 5.0, 6.0]),
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells: vec![FptvoxIndexedTriangleCell {
            coordinate: [1, 2, 3],
            cell,
            first_reference: 0,
            reference_count: 1,
        }],
        triangles: vec![FptvoxIndexedTriangle {
            vertices: [[0, 1, 2], [3, 4, 5], [6, 7, 65535]],
        }],
        triangle_colors: vec![0x0033_2211],
        triangle_vertex_colors: vec![[0x0000_00ff, 0x0000_ff00, 0x00ff_0000]],
        triangle_material_ids: vec![1],
        triangle_shading_normals: vec![0],
        references: vec![0],
    };
    let output = temporary_artifact("indexed-triangle-exact");
    let summary = export_fptvox_indexed_triangle_surface(&surface, &output).unwrap();
    let bytes = fs::read(&output).unwrap();

    assert_eq!(bytes[0..8], FPTVOX_INDEXED_TRIANGLE_MAGIC);
    assert_eq!(u32_at(&bytes, 8), FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_INDEXED_TRIANGLE_VERSION);
    assert_eq!(u32_at(&bytes, 76), FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE);
    assert_eq!(u64_at(&bytes, 80), 1);
    assert_eq!(u32_at(&bytes, 88), FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE);
    assert_eq!(u64_at(&bytes, 96), 1);
    assert_eq!(u32_at(&bytes, 104), FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE);
    let cell_offset = FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE as usize;
    assert_eq!(u32_at(&bytes, cell_offset + 24), 0);
    assert_eq!(u32_at(&bytes, cell_offset + 28), 1);
    let triangle_offset = cell_offset + FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE as usize;
    assert_eq!(
        &bytes[triangle_offset..triangle_offset + 18],
        &[0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 255, 255]
    );
    assert_eq!(u32_at(&bytes, triangle_offset + 20), 0);
    assert_eq!(summary.voxel_count, 1);
    assert_eq!(summary.bytes, 168);
    assert_eq!(bytes.len(), 168);
    fs::remove_file(output).unwrap();
}

#[test]
fn indexed_triangle_normal_trailer_is_byte_exact() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("triangle-normal-trailer");
    let geometry = export_fptvox(&grid, &output).unwrap();
    let packed_normals = [0u32, (1u32 << 30) | 1u32 | (2u32 << 10) | (3u32 << 20)];
    let appended = append_fptvox_triangle_normals(&output, &packed_normals).unwrap();
    let bytes = fs::read(&output).unwrap();
    let offset = geometry.bytes as usize;

    assert_eq!(bytes[offset..offset + 8], FPTVOX_TRIANGLE_NORMAL_MAGIC);
    assert_eq!(
        u32_at(&bytes, offset + 8),
        FPTVOX_TRIANGLE_NORMAL_HEADER_SIZE
    );
    assert_eq!(u32_at(&bytes, offset + 12), FPTVOX_TRIANGLE_NORMAL_VERSION);
    assert_eq!(
        u32_at(&bytes, offset + 16),
        FPTVOX_TRIANGLE_NORMAL_RECORD_SIZE
    );
    assert_eq!(u32_at(&bytes, offset + 20), 0);
    assert_eq!(u64_at(&bytes, offset + 24), 2);
    assert_eq!(u32_at(&bytes, offset + 32), packed_normals[0]);
    assert_eq!(u32_at(&bytes, offset + 36), packed_normals[1]);
    assert_eq!(appended, 40);
    assert_eq!(bytes.len(), offset + 40);
    fs::remove_file(output).unwrap();

    let invalid_output = temporary_artifact("triangle-normal-noncanonical");
    export_fptvox(&grid, &invalid_output).unwrap();
    let error = append_fptvox_triangle_normals(&invalid_output, &[1u32]).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    fs::remove_file(invalid_output).unwrap();
}

#[test]
fn indexed_triangle_surface_rejects_unsafe_cell_fanout() {
    let reference_count = FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL + 1;
    let surface = FptvoxIndexedTriangleSurface {
        resolution: [1; 3],
        sampling_resolution: [2; 3],
        bounds: Aabb::new([0.0; 3], [1.0; 3]),
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells: vec![FptvoxIndexedTriangleCell {
            coordinate: [0; 3],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
            first_reference: 0,
            reference_count,
        }],
        triangles: vec![FptvoxIndexedTriangle {
            vertices: [[0; 3], [0, 0, 1], [0, 1, 0]],
        }],
        triangle_colors: vec![0x0033_2211],
        triangle_vertex_colors: vec![[0x0000_00ff, 0x0000_ff00, 0x00ff_0000]],
        triangle_material_ids: vec![1],
        triangle_shading_normals: vec![0],
        references: vec![0; reference_count as usize],
    };
    let output = temporary_artifact("indexed-triangle-unsafe-fanout");
    let error = export_fptvox_indexed_triangle_surface(&surface, &output).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    assert!(!output.exists());
}

#[test]
fn triangle_bvh_surface_is_byte_exact_and_versioned() {
    let cell = VoxelCell {
        packed_color: 0x8122_3344,
        packed_properties: 0xaabb_ccdd,
        emission: 1.5,
    };
    let triangle = FptvoxTriangle {
        vertices: [0, 1023, 1023 << 10],
    };
    let surface = FptvoxTriangleBvhSurface {
        resolution: [1; 3],
        sampling_resolution: [5, 6, 7],
        bounds: Aabb::new([0.0; 3], [1.0; 3]),
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells: vec![FptvoxBvhCell {
            coordinate: [0; 3],
            cell,
            first_node: 0,
            node_count: 1,
        }],
        triangles: vec![
            FptvoxBvhTriangle {
                triangle,
                original_order: 0,
            },
            FptvoxBvhTriangle {
                triangle,
                original_order: 1,
            },
        ],
        nodes: vec![FptvoxBvhNode {
            bounds: [0, 0, 0, 255, 255, 255],
            triangle_count: 2,
            first_triangle: 0,
            escape: 1,
        }],
    };
    let output = temporary_artifact("triangle-bvh-exact");
    let summary = export_fptvox_triangle_bvh_surface(&surface, &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0..8], FPTVOX_TRIANGLE_BVH_MAGIC);
    assert_eq!(u32_at(&bytes, 8), FPTVOX_TRIANGLE_BVH_HEADER_SIZE);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_TRIANGLE_BVH_VERSION);
    assert_eq!(u32_at(&bytes, 76), FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE);
    assert_eq!(u64_at(&bytes, 80), 2);
    assert_eq!(u32_at(&bytes, 88), FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE);
    assert_eq!(u64_at(&bytes, 96), 1);
    assert_eq!(u32_at(&bytes, 104), FPTVOX_TRIANGLE_BVH_NODE_RECORD_SIZE);
    let cell_offset = FPTVOX_TRIANGLE_BVH_HEADER_SIZE as usize;
    let triangle_offset = cell_offset + FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE as usize;
    let node_offset = triangle_offset + 2 * FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE as usize;
    assert_eq!(u32_at(&bytes, triangle_offset + 12), 0);
    assert_eq!(u32_at(&bytes, triangle_offset + 28), 1);
    assert_eq!(
        &bytes[node_offset..node_offset + 6],
        &[0, 0, 0, 255, 255, 255]
    );
    assert_eq!(summary.voxel_count, 1);
    assert_eq!(summary.bytes, 192);
    assert_eq!(bytes.len(), 192);
    fs::remove_file(output).unwrap();
}

#[test]
fn indexed_triangle_bvh_surface_is_byte_exact_and_versioned() {
    let cell = VoxelCell {
        packed_color: 0x8122_3344,
        packed_properties: 0xaabb_ccdd,
        emission: 1.5,
    };
    let surface = FptvoxIndexedTriangleBvhSurface {
        resolution: [1; 3],
        sampling_resolution: [5, 6, 7],
        bounds: Aabb::new([0.0; 3], [1.0; 3]),
        coordinate_system: CoordinateSystem::YUpRightHanded,
        cells: vec![FptvoxBvhCell {
            coordinate: [0; 3],
            cell,
            first_node: 0,
            node_count: 1,
        }],
        triangles: vec![FptvoxIndexedTriangle {
            vertices: [[0, 0, 0], [u16::MAX, 0, 0], [0, u16::MAX, 0]],
        }],
        triangle_colors: vec![0x0033_2211],
        triangle_vertex_colors: vec![[0x0000_00ff, 0x0000_ff00, 0x00ff_0000]],
        triangle_material_ids: vec![1],
        triangle_shading_normals: vec![0],
        references: vec![0, 0],
        nodes: vec![FptvoxBvhNode {
            bounds: [0, 0, 0, 255, 255, 255],
            triangle_count: 2,
            first_triangle: 0,
            escape: 1,
        }],
    };
    let output = temporary_artifact("indexed-triangle-bvh-exact");
    let summary = export_fptvox_indexed_triangle_bvh_surface(&surface, &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0..8], FPTVOX_INDEXED_TRIANGLE_BVH_MAGIC);
    assert_eq!(u32_at(&bytes, 8), FPTVOX_INDEXED_TRIANGLE_BVH_HEADER_SIZE);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_INDEXED_TRIANGLE_BVH_VERSION);
    assert_eq!(
        u32_at(&bytes, 76),
        FPTVOX_INDEXED_TRIANGLE_BVH_CELL_RECORD_SIZE
    );
    assert_eq!(u64_at(&bytes, 80), 1);
    assert_eq!(
        u32_at(&bytes, 88),
        FPTVOX_INDEXED_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE
    );
    assert_eq!(u64_at(&bytes, 96), 2);
    assert_eq!(
        u32_at(&bytes, 104),
        FPTVOX_INDEXED_TRIANGLE_BVH_REFERENCE_RECORD_SIZE
    );
    assert_eq!(u64_at(&bytes, 112), 1);
    assert_eq!(
        u32_at(&bytes, 120),
        FPTVOX_INDEXED_TRIANGLE_BVH_NODE_RECORD_SIZE
    );
    assert_eq!(summary.voxel_count, 1);
    assert_eq!(summary.bytes, 204);
    assert_eq!(bytes.len(), 204);
    fs::remove_file(output).unwrap();
}

#[test]
fn surface_payload_is_byte_exact_and_versioned() {
    let voxel = SparseVoxel {
        coordinate: [1, 2, 3],
        cell: VoxelCell {
            packed_color: 0x8122_3344,
            packed_properties: 0xaabb_ccdd,
            emission: 1.5,
        },
    };
    let grid = fixture_grid(vec![voxel], [2, 3, 4]);
    let output = temporary_artifact("surface-exact");
    let summary = export_fptvox_with_normals(&grid, &[0x5678_1234], &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0..8], FPTVOX_SURFACE_MAGIC);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_SURFACE_VERSION);
    assert_eq!(bytes.len(), 64 + FPTVOX_SURFACE_RECORD_SIZE as usize);
    assert_eq!(u32_at(&bytes, 64 + 24), 0x5678_1234);
    assert_eq!(summary.bytes, 92);
    fs::remove_file(output).unwrap();
}

#[test]
fn surface_payload_rejects_a_mismatched_normal_count() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0, 0, 0],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("surface-invalid-count");
    let error = export_fptvox_with_normals(&grid, &[], &output).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    assert!(!output.exists());
}

#[test]
fn plane_payload_is_byte_exact_and_versioned() {
    let voxel = SparseVoxel {
        coordinate: [1, 2, 3],
        cell: VoxelCell {
            packed_color: 0x8122_3344,
            packed_properties: 0xaabb_ccdd,
            emission: 1.5,
        },
    };
    let grid = fixture_grid(vec![voxel], [2, 3, 4]);
    let output = temporary_artifact("plane-exact");
    let summary = export_fptvox_with_planes(&grid, &[0x9abc_def0], &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0..8], FPTVOX_PLANE_MAGIC);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_PLANE_VERSION);
    assert_eq!(bytes.len(), 64 + FPTVOX_PLANE_RECORD_SIZE as usize);
    assert_eq!(u32_at(&bytes, 64 + 24), 0x9abc_def0);
    assert_eq!(summary.bytes, 92);
    fs::remove_file(output).unwrap();
}

#[test]
fn plane_payload_rejects_a_mismatched_count() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0, 0, 0],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("plane-invalid-count");
    let error = export_fptvox_with_planes(&grid, &[], &output).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    assert!(!output.exists());
}

#[test]
fn plane_pair_payload_is_byte_exact_and_versioned() {
    let voxel = SparseVoxel {
        coordinate: [1, 2, 3],
        cell: VoxelCell {
            packed_color: 0x8122_3344,
            packed_properties: 0xaabb_ccdd,
            emission: 1.5,
        },
    };
    let grid = fixture_grid(vec![voxel], [2, 3, 4]);
    let output = temporary_artifact("plane-pair-exact");
    let summary =
        export_fptvox_with_plane_pairs(&grid, &[[0x9abc_def0, 0x1234_5678]], &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0..8], FPTVOX_PLANE_PAIR_MAGIC);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_PLANE_PAIR_VERSION);
    assert_eq!(bytes.len(), 64 + FPTVOX_PLANE_PAIR_RECORD_SIZE as usize);
    assert_eq!(u32_at(&bytes, 64 + 24), 0x9abc_def0);
    assert_eq!(u32_at(&bytes, 64 + 28), 0x1234_5678);
    assert_eq!(summary.bytes, 96);
    fs::remove_file(output).unwrap();
}

#[test]
fn plane_pair_payload_rejects_invalid_counts_and_primary_plane() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0, 0, 0],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("plane-pair-invalid");
    let count_error = export_fptvox_with_plane_pairs(&grid, &[], &output).unwrap_err();
    assert_eq!(count_error.code, FractalErrorCode::Artifact);
    let primary_error =
        export_fptvox_with_plane_pairs(&grid, &[[0, 0x1234_5678]], &output).unwrap_err();
    assert_eq!(primary_error.code, FractalErrorCode::Artifact);
    assert!(!output.exists());
}

#[test]
fn bounded_patch_payload_is_byte_exact_and_versioned() {
    let voxel = SparseVoxel {
        coordinate: [1, 2, 3],
        cell: VoxelCell {
            packed_color: 0x8122_3344,
            packed_properties: 0xaabb_ccdd,
            emission: 1.5,
        },
    };
    let grid = fixture_grid(vec![voxel], [2, 3, 4]);
    let output = temporary_artifact("bounded-patch-exact");
    let patches = [[0x9abc_def0, 0x1234_5678, 0x8070_4030]];
    let summary = export_fptvox_with_bounded_patches(&grid, &patches, &output).unwrap();
    let bytes = fs::read(&output).unwrap();
    assert_eq!(bytes[0..8], FPTVOX_BOUNDED_PATCH_MAGIC);
    assert_eq!(u32_at(&bytes, 12), FPTVOX_BOUNDED_PATCH_VERSION);
    assert_eq!(bytes.len(), 64 + FPTVOX_BOUNDED_PATCH_RECORD_SIZE as usize);
    for (word, expected) in patches[0].iter().enumerate() {
        assert_eq!(u32_at(&bytes, 64 + 24 + word * 4), *expected);
    }
    assert_eq!(summary.bytes, 100);
    fs::remove_file(output).unwrap();
}

#[test]
fn bounded_patch_payload_rejects_invalid_records() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [0, 0, 0],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [1; 3],
    );
    let output = temporary_artifact("bounded-patch-invalid");
    assert_eq!(
        export_fptvox_with_bounded_patches(&grid, &[], &output)
            .unwrap_err()
            .code,
        FractalErrorCode::Artifact
    );
    assert_eq!(
        export_fptvox_with_bounded_patches(&grid, &[[0, 0, 0]], &output)
            .unwrap_err()
            .code,
        FractalErrorCode::Artifact
    );
    assert_eq!(
        export_fptvox_with_bounded_patches(&grid, &[[1, 0, 0x0001_0000]], &output)
            .unwrap_err()
            .code,
        FractalErrorCode::Artifact
    );
    assert!(!output.exists());
}

#[test]
fn output_is_deterministic_and_round_trips_in_grid_order() {
    let first_cell = VoxelCell::from_material(SurfaceMaterial::default());
    let second_cell = VoxelCell::from_material(SurfaceMaterial {
        base_color: [0.2, 0.4, 0.8],
        roughness: 0.3,
        specular: 0.7,
        transmission: 0.5,
        ior: 1.45,
        emission_strength: 3.25,
    });
    let grid = fixture_grid(
        vec![
            SparseVoxel {
                coordinate: [0, 0, 0],
                cell: first_cell,
            },
            SparseVoxel {
                coordinate: [1, 1, 1],
                cell: second_cell,
            },
        ],
        [2; 3],
    );
    let first = temporary_artifact("deterministic-a");
    let second = temporary_artifact("deterministic-b");
    export_fptvox(&grid, &first).unwrap();
    export_fptvox(&grid, &second).unwrap();
    let first_bytes = fs::read(&first).unwrap();
    assert_eq!(first_bytes, fs::read(&second).unwrap());

    let decoded = decode_for_test(&first_bytes);
    assert_eq!(decoded.resolution, grid.resolution);
    assert_eq!(decoded.coordinate_system, 1);
    assert_eq!(decoded.bounds, grid.bounds);
    assert_eq!(decoded.voxels, grid.voxels);
    fs::remove_file(first).unwrap();
    fs::remove_file(second).unwrap();
}

#[test]
fn invalid_coordinates_are_rejected_before_file_creation() {
    let grid = fixture_grid(
        vec![SparseVoxel {
            coordinate: [2, 0, 0],
            cell: VoxelCell::from_material(SurfaceMaterial::default()),
        }],
        [2; 3],
    );
    let output = temporary_artifact("invalid-coordinate");
    let error = export_fptvox(&grid, &output).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    assert!(!output.exists());
}

#[test]
fn impossible_voxel_counts_are_rejected() {
    let cell = VoxelCell::from_material(SurfaceMaterial::default());
    let grid = fixture_grid(
        vec![
            SparseVoxel {
                coordinate: [0; 3],
                cell,
            },
            SparseVoxel {
                coordinate: [0; 3],
                cell,
            },
        ],
        [1; 3],
    );
    let output = temporary_artifact("invalid-count");
    let error = export_fptvox(&grid, &output).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    assert!(error.message.contains("voxel count"));
    assert!(!output.exists());
}

#[test]
fn cli_infers_fptvox_from_output_suffix() {
    let output = temporary_artifact("cli");
    let result = Command::new(env!("CARGO_BIN_EXE_fpt-metal"))
        .args([
            "voxel-export",
            "builtin:menger-sponge",
            "--out",
            output.to_str().unwrap(),
            "--voxel-resolution",
            "8",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["format"], "fptvox");
    assert_eq!(&fs::read(&output).unwrap()[0..8], &FPTVOX_MAGIC);
    fs::remove_file(output).unwrap();
}
