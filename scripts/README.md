# Experiment and regression runners

The `run_*_experiment.sh` scripts added with the procedural-compiler work are
research and reproduction tooling. They are not invoked by the renderer,
selected automatically, or required for an ordinary build.

Production backend selection is implemented in `src/main.rs` and considers
only:

- the optimized direct evaluator;
- generated topology distance with interpreted accepted-hit surface work;
- generated topology distance with analytic accepted-hit surface work.

The new runners are grouped as follows:

| Area | Scripts |
| --- | --- |
| Compiler baselines | `run_geometry_split_experiment.sh`, `run_flat_union_experiment.sh`, `run_typed_soa_experiment.sh`, `run_canonical_ir_experiment.sh` |
| Generated MSL and selection | `run_runtime_source_control_experiment.sh`, `run_topology_specialization_experiment.sh`, `run_topology_hardening_experiment.sh`, `run_generated_surface_experiment.sh`, `run_backend_selection_experiment.sh`, `run_generated_selector_workload_matrix.sh` |
| Function-stitching research | `run_function_stitching_experiment.sh`, `run_stitched_surface_experiment.sh`, `run_stitch_cache_experiment.sh`, `run_stitch_differential_fuzz.sh`, `run_stitch_vocabulary_experiment.sh`, `run_stitch_state_experiment.sh`, `run_stitch_threshold_experiment.sh`, `run_stitch_split_experiment.sh`, `run_stitch_fusion_experiment.sh`, `run_interval_jacobian_experiment.sh` |
| Alternative generated representations | `run_compact_canonical_codegen_experiment.sh`, `run_shared_transform_dag_experiment.sh`, `run_affine_index_experiment.sh`, `run_dual_generated_library_experiment.sh`, `run_tiny_linked_helper_experiment.sh` |
| Regional and field acceleration | `run_bound_grid_experiment.sh`, `run_cage_bound_experiment.sh`, `run_directional_grid_experiment.sh`, `run_directional_fp16_experiment.sh`, `run_directional_crossover_experiment.sh`, `run_regional_program_experiment.sh`, `run_regional_hardening_experiment.sh`, `run_regional_flat_union_experiment.sh` |
| Voxel construction and traversal research | `run_direct_voxel_build_experiment.sh`, `run_brick_rejection_experiment.sh`, `run_voxel_coverage_experiment.sh`, `run_exact_voxel_material_experiment.sh`, `run_exact_voxel_normal_experiment.sh`, `run_precision_voxel_offset_experiment.sh`, `run_precision_face_offset_experiment.sh`, `run_leaf_refinement_experiment.sh`, `run_fractal_leaf_experiment.sh`, `run_voxel_optimization_regression.sh` |

Every runner writes timestamped data below `reports/`, which is intentionally
gitignored. The committed review evidence is the compact package under
`docs/evidence/procedural-jit/`.

Run scripts from the repository root. Most build the release binary unless
`SKIP_BUILD=1` is provided; consult the variables at the top of a runner before
launching a long matrix.
