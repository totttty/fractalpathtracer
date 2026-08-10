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
pub mod mandelbulber;
#[doc(hidden)]
pub mod scene;
#[doc(hidden)]
pub mod tools;
pub mod voxel;

pub use fptvox::{
    FPTVOX_HEADER_SIZE, FPTVOX_MAGIC, FPTVOX_RECORD_SIZE, FPTVOX_VERSION, FptvoxExportSummary,
    export_fptvox,
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
