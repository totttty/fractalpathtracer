use fpt_metal::{
    Aabb, CoordinateSystem, FPTVOX_HEADER_SIZE, FPTVOX_MAGIC, FPTVOX_RECORD_SIZE, FPTVOX_VERSION,
    FractalErrorCode, SparseVoxel, SurfaceMaterial, VoxelCell, VoxelGrid, export_fptvox,
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
