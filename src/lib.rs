#![recursion_limit = "512"]

//! Reusable fractal scene preparation and voxel artifact interfaces.
//!
//! The stable consumer-facing seam is re-exported from [`voxel`]. Existing
//! renderer modules remain public only so the package binary can share their
//! implementation rather than compiling a second copy.

#[doc(hidden)]
pub mod ffi;
pub mod fptvox;
#[doc(hidden)]
pub mod fptvox7;
#[doc(hidden)]
pub mod mandelbulber;
#[doc(hidden)]
pub mod scene;
#[doc(hidden)]
pub mod tools;
pub mod voxel;

pub use fptvox::{
    FPTVOX_APPEARANCE_AUX_LIGHT_ENABLED, FPTVOX_APPEARANCE_AUX_LIGHT_SHADOWS,
    FPTVOX_APPEARANCE_MAGIC, FPTVOX_APPEARANCE_MAIN_LIGHT_ENABLED,
    FPTVOX_APPEARANCE_MAIN_LIGHT_SHADOWS, FPTVOX_APPEARANCE_SIZE,
    FPTVOX_APPEARANCE_SPECULAR_PLASTIC, FPTVOX_APPEARANCE_THREE_COLOR_BACKGROUND,
    FPTVOX_APPEARANCE_VERSION, FPTVOX_BOUNDED_PATCH_MAGIC, FPTVOX_BOUNDED_PATCH_RECORD_SIZE,
    FPTVOX_BOUNDED_PATCH_VERSION, FPTVOX_CAMERA_MAGIC, FPTVOX_CAMERA_SIZE, FPTVOX_CAMERA_VERSION,
    FPTVOX_ENVIRONMENT_BASIC_FOG, FPTVOX_ENVIRONMENT_CLOUDS, FPTVOX_ENVIRONMENT_HDRI,
    FPTVOX_ENVIRONMENT_HEADER_SIZE, FPTVOX_ENVIRONMENT_ITERATION_FOG,
    FPTVOX_ENVIRONMENT_LUT_HEIGHT, FPTVOX_ENVIRONMENT_LUT_VALUES, FPTVOX_ENVIRONMENT_LUT_WIDTH,
    FPTVOX_ENVIRONMENT_MAGIC, FPTVOX_ENVIRONMENT_SIZE, FPTVOX_ENVIRONMENT_VERSION,
    FPTVOX_ENVIRONMENT_VOLUMETRIC_FOG, FPTVOX_HEADER_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_CELL_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_BVH_HEADER_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_MAGIC, FPTVOX_INDEXED_TRIANGLE_BVH_NODE_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_REFERENCE_RECORD_SIZE,
    FPTVOX_INDEXED_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_BVH_VERSION,
    FPTVOX_INDEXED_TRIANGLE_CELL_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_HEADER_SIZE,
    FPTVOX_INDEXED_TRIANGLE_MAGIC, FPTVOX_INDEXED_TRIANGLE_MAX_REFERENCES_PER_CELL,
    FPTVOX_INDEXED_TRIANGLE_RECORD_SIZE, FPTVOX_INDEXED_TRIANGLE_REFERENCE_SIZE,
    FPTVOX_INDEXED_TRIANGLE_VERSION, FPTVOX_MAGIC, FPTVOX_MATERIAL_HEADER_SIZE,
    FPTVOX_MATERIAL_MAGIC, FPTVOX_MATERIAL_RECORD_SIZE, FPTVOX_MATERIAL_VERSION,
    FPTVOX_PLANE_MAGIC, FPTVOX_PLANE_PAIR_MAGIC, FPTVOX_PLANE_PAIR_RECORD_SIZE,
    FPTVOX_PLANE_PAIR_VERSION, FPTVOX_PLANE_RECORD_SIZE, FPTVOX_PLANE_VERSION, FPTVOX_RECORD_SIZE,
    FPTVOX_SURFACE_MAGIC, FPTVOX_SURFACE_RECORD_SIZE, FPTVOX_SURFACE_VERSION,
    FPTVOX_TRIANGLE_BVH_CELL_RECORD_SIZE, FPTVOX_TRIANGLE_BVH_HEADER_SIZE,
    FPTVOX_TRIANGLE_BVH_MAGIC, FPTVOX_TRIANGLE_BVH_NODE_RECORD_SIZE,
    FPTVOX_TRIANGLE_BVH_TRIANGLE_RECORD_SIZE, FPTVOX_TRIANGLE_BVH_VERSION,
    FPTVOX_TRIANGLE_CELL_RECORD_SIZE, FPTVOX_TRIANGLE_COLOR_HEADER_SIZE,
    FPTVOX_TRIANGLE_COLOR_MAGIC, FPTVOX_TRIANGLE_COLOR_RECORD_SIZE, FPTVOX_TRIANGLE_COLOR_VERSION,
    FPTVOX_TRIANGLE_HEADER_SIZE, FPTVOX_TRIANGLE_MAGIC, FPTVOX_TRIANGLE_MATERIAL_HEADER_SIZE,
    FPTVOX_TRIANGLE_MATERIAL_MAGIC, FPTVOX_TRIANGLE_MATERIAL_RECORD_SIZE,
    FPTVOX_TRIANGLE_MATERIAL_VERSION, FPTVOX_TRIANGLE_RECORD_SIZE, FPTVOX_TRIANGLE_VERSION,
    FPTVOX_TRIANGLE_VERTEX_COLOR_HEADER_SIZE, FPTVOX_TRIANGLE_VERTEX_COLOR_MAGIC,
    FPTVOX_TRIANGLE_VERTEX_COLOR_RECORD_SIZE, FPTVOX_TRIANGLE_VERTEX_COLOR_VERSION, FPTVOX_VERSION,
    FptvoxAppearance, FptvoxAuthoredMaterial, FptvoxBvhCell, FptvoxBvhNode, FptvoxBvhTriangle,
    FptvoxCamera, FptvoxEnvironment, FptvoxExportSummary, FptvoxIndexedTriangle,
    FptvoxIndexedTriangleBvhSurface, FptvoxIndexedTriangleCell, FptvoxIndexedTriangleSurface,
    FptvoxTriangle, FptvoxTriangleBvhSurface, FptvoxTriangleCell, FptvoxTriangleSurface,
    append_fptvox_appearance, append_fptvox_camera, append_fptvox_environment,
    append_fptvox_materials, append_fptvox_triangle_colors, append_fptvox_triangle_material_ids,
    append_fptvox_triangle_vertex_colors, export_fptvox,
    export_fptvox_indexed_triangle_bvh_surface, export_fptvox_indexed_triangle_surface,
    export_fptvox_triangle_bvh_surface, export_fptvox_triangle_surface,
    export_fptvox_with_bounded_patches, export_fptvox_with_normals, export_fptvox_with_plane_pairs,
    export_fptvox_with_planes,
};
pub use mandelbulber::{
    MandelbulberFormulaSlot, MandelbulberGradientStop, MandelbulberMaterial, MandelbulberScene,
};
pub use voxel::{
    Aabb, BuiltinFractal, BuiltinFractalScene, CellSampling, CoordinateSystem,
    FPT_VOXEL_CONTRACT_NAME, FPT_VOXEL_CONTRACT_VERSION, FractalError, FractalErrorCode,
    FractalScene, GlbExportSummary, MandelbulberSceneSource, SparseVoxel, SurfaceMaterial,
    VoxelCell, VoxelGrid, VoxelizationParameters, VoxelizationRequest, export_glb, voxelize,
    voxelize_to_glb,
};

#[cfg(test)]
static METAL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn metal_test_guard() -> std::sync::MutexGuard<'static, ()> {
    METAL_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
