use fpt_metal::{
    Aabb, CoordinateSystem, FractalErrorCode, FractalScene, MandelbulberScene, SurfaceMaterial,
    VoxelCell, VoxelGrid, VoxelizationParameters, VoxelizationRequest, export_glb, voxelize,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn temporary_glb(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("fpt-{name}-{}-{nonce}.glb", std::process::id()))
}

fn glb_json(bytes: &[u8]) -> Value {
    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
        bytes.len()
    );
    let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    assert_eq!(&bytes[16..20], b"JSON");
    serde_json::from_slice(&bytes[20..20 + json_len]).unwrap()
}

#[test]
fn built_in_voxelization_is_deterministic() {
    let request = VoxelizationRequest {
        scene: FractalScene::menger_sponge(),
        voxelization: VoxelizationParameters::cubic(14, Aabb::new([-1.25; 3], [1.25; 3])),
    };
    let first = voxelize(&request).unwrap();
    let second = voxelize(&request).unwrap();
    assert_eq!(first, second);
    assert!(!first.voxels.is_empty());
    assert!(first.voxels.len() < 14_usize.pow(3));
}

#[test]
fn fract_scene_parses_but_reference_voxelization_requires_metal() {
    let path = fixture_path("scenes/mandelbulber/difs-sphere-generated.fract");
    let scene = MandelbulberScene::load(&path).unwrap();
    assert_eq!(scene.formula_id, 1604);
    let request = VoxelizationRequest {
        scene: FractalScene::mandelbulber(path),
        voxelization: VoxelizationParameters::cubic(8, Aabb::new([-2.0; 3], [2.0; 3])),
    };
    let error = voxelize(&request).unwrap_err();
    assert_eq!(error.code, FractalErrorCode::CpuReferenceUnavailable);
    assert!(error.message.contains("voxel-export"));
}

#[test]
fn dense_cells_round_trip_to_contract_marked_glb() {
    let material = SurfaceMaterial {
        base_color: [0.2, 0.5, 0.8],
        roughness: 0.35,
        specular: 0.7,
        transmission: 0.25,
        ior: 1.45,
        emission_strength: 2.0,
    };
    let occupied = VoxelCell::from_material(material);
    let mut dense = vec![VoxelCell::default(); 8];
    dense[0] = occupied;
    dense[1] = occupied;
    let grid = VoxelGrid::from_dense_cells(
        [2; 3],
        Aabb::new([-1.0; 3], [1.0; 3]),
        CoordinateSystem::YUpRightHanded,
        "constructed",
        "fixture-sha",
        &dense,
    )
    .unwrap();
    assert_eq!(grid.voxels.len(), 2);
    assert_eq!(grid.voxels[0].cell.to_le_bytes(), occupied.to_le_bytes());

    let output = temporary_glb("voxel-contract");
    let summary = export_glb(&grid, &output).unwrap();
    assert_eq!(summary.quads, 6);
    let document = glb_json(&fs::read(&output).unwrap());
    let contract = &document["asset"]["extras"]["fpt_voxel_contract"];
    assert_eq!(contract["version"], 1);
    assert_eq!(contract["resolution"], serde_json::json!([2, 2, 2]));
    assert_eq!(
        contract["bounds_min"],
        serde_json::json!([-1.0, -1.0, -1.0])
    );
    assert_eq!(contract["bounds_max"], serde_json::json!([1.0, 1.0, 1.0]));
    assert_eq!(contract["occupied_cells"], 2);
    assert_eq!(contract["surface_quads"], 6);
    assert_eq!(
        document["meshes"][0]["primitives"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        document["materials"][0]["extras"]["fpt_voxel_cell"]["emission"],
        2.0
    );
    fs::remove_file(output).unwrap();
}
