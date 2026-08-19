use fpt_metal::{
    Aabb, CoordinateSystem, FPTVOX_BOUNDED_PATCH_MAGIC, FPTVOX_BOUNDED_PATCH_RECORD_SIZE,
    FPTVOX_BOUNDED_PATCH_VERSION, FPTVOX_HEADER_SIZE, FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE, FPTVOX_INDEXED_TRIANGLE_MAGIC,
    FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL, FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE, FPTVOX_INDEXED_TRIANGLE_VERSION, FPTVOX_MAGIC,
    FPTVOX_PLANE_MAGIC, FPTVOX_PLANE_PAIR_MAGIC, FPTVOX_PLANE_PAIR_RECORD_SIZE,
    FPTVOX_PLANE_PAIR_VERSION, FPTVOX_PLANE_RECORD_SIZE, FPTVOX_PLANE_VERSION, FPTVOX_RECORD_SIZE,
    FPTVOX_SURFACE_MAGIC, FPTVOX_SURFACE_RECORD_SIZE, FPTVOX_SURFACE_VERSION,
    FPTVOX_TRIANGLE_CELL_RECORD_SIZE, FPTVOX_TRIANGLE_HEADER_SIZE, FPTVOX_TRIANGLE_MAGIC,
    FPTVOX_TRIANGLE_RECORD_SIZE, FPTVOX_TRIANGLE_VERSION, FPTVOX_VERSION, FptvoxIndexedTriangle,
    FptvoxIndexedTriangleCell, FptvoxIndexedTriangleSurface, FptvoxTriangle, FptvoxTriangleCell,
    FptvoxTriangleSurface, FractalErrorCode, SparseVoxel, SurfaceMaterial, VoxelCell, VoxelGrid,
    export_fptvox, export_fptvox_indexed_triangle_surface, export_fptvox_triangle_surface,
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
        references: vec![0; reference_count as usize],
    };
    let output = temporary_artifact("indexed-triangle-unsafe-fanout");
    let error = export_fptvox_indexed_triangle_surface(&surface, &output).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::Artifact);
    assert!(!output.exists());
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
