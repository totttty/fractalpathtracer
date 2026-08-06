//! Backend-neutral Mandelbulber orbit semantics.
//!
//! Formula kernels mutate [`OrbitState`]. Iteration scheduling, constant-C
//! addition, bailout, and analytic distance finalization live here so Metal,
//! CPU validation, and future generated backends share one contract.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitState {
    pub z: [f64; 4],
    pub c: [f64; 4],
    pub const_c: [f64; 4],
    pub old_z: [f64; 4],
    pub radius: f64,
    pub derivative: f64,
    pub derivative0: f64,
    pub distance: f64,
    pub pseudo_kleinian_de: f64,
    pub actual_scale: f64,
    pub actual_scale_a: f64,
    pub color: f64,
    pub color_hybrid: f64,
    pub temp1000: f64,
    pub pos_neg: f64,
    pub iteration: u32,
}

impl OrbitState {
    pub fn new(point: [f64; 4], initial_scale: f64) -> Self {
        Self {
            z: point,
            c: point,
            const_c: point,
            old_z: point,
            radius: length4(point),
            derivative: 1.0,
            derivative0: 0.0,
            distance: 1000.0,
            pseudo_kleinian_de: 1.0,
            actual_scale: initial_scale,
            actual_scale_a: 0.0,
            color: 1.0,
            color_hybrid: 0.0,
            temp1000: 1000.0,
            pos_neg: 1.0,
            iteration: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum AnalyticFinalizer {
    Logarithmic,
    Linear,
    Ifs,
    PseudoKleinian,
    JosKleinian,
    CustomDistance,
    MaxAxis,
    None,
    Undefined,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinalizerParameters {
    pub spheres_enabled: bool,
    pub folding_value: f64,
    pub tweak005: f64,
    pub offset1: f64,
}

impl Default for FinalizerParameters {
    fn default() -> Self {
        Self {
            spheres_enabled: false,
            folding_value: 0.0,
            tweak005: f64::INFINITY,
            offset1: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstantAddition {
    pub julia: Option<[f64; 4]>,
    pub multiplier: [f64; 4],
    pub swap_xy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KaleidoscopicIfs {
    pub absolute: [bool; 3],
    pub enabled: [bool; 9],
    pub directions: [[f64; 3]; 9],
    pub rotations: [[[f64; 3]; 3]; 9],
    pub distances: [f64; 9],
    pub intensities: [f64; 9],
    pub main_rotation: [[f64; 3]; 3],
    pub rotation_enabled: bool,
    pub offset: [f64; 3],
    pub scale: f64,
    pub edge: [f64; 3],
    pub edge_enabled: bool,
    pub menger_sponge_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JosKleinian {
    pub analytic_scale: f64,
    pub color_enabled: bool,
    pub color_grid_enabled: bool,
    pub color_add_enabled: bool,
    pub color_factors: [f64; 4],
    pub color_mix: f64,
    pub color_start: u32,
    pub color_stop: u32,
    pub addition: [f64; 4],
    pub addition_p: [f64; 4],
    pub constant_c: [f64; 4],
    pub offset_zero: [f64; 4],
    pub box_size: [f64; 4],
    pub scale_three: [f64; 4],
    pub folding_value: f64,
    pub max_radius_squared: f64,
    pub offset: f64,
    pub grid_y_enabled: bool,
    pub sphere_inversion_enabled: bool,
    pub start_c: u32,
    pub start_t: u32,
    pub stop_c: u32,
    pub stop_t: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PseudoKleinian {
    pub analytic_offset: f64,
    pub analytic_scale: f64,
    pub analytic_tweak: f64,
    pub color_grid_enabled: bool,
    pub color_add_enabled: bool,
    pub color_factors: [f64; 4],
    pub color_mix: f64,
    pub color_start: u32,
    pub color_stop: u32,
    pub box_color: [f64; 3],
    pub folding_limit: f64,
    pub folding_value: f64,
    pub addition: [f64; 4],
    pub box_size: [f64; 4],
    pub fold_size: [f64; 4],
    pub inversion_addition: [f64; 4],
    pub grid_phase: [f64; 4],
    pub offset_zero: [f64; 4],
    pub prism_offset: [f64; 4],
    pub grid_scale: [f64; 4],
    pub grid_multiplier: [f64; 4],
    pub rotation: [[f64; 3]; 3],
    pub max_radius_squared: f64,
    pub minimum_radius: f64,
    pub prism_scale: f64,
    pub z_fold_scale: f64,
    pub grid_z_enabled: bool,
    pub squared_grid_enabled: bool,
    pub box_fold_enabled: bool,
    pub fold_z_enabled: bool,
    pub tglad_fold_enabled: bool,
    pub negate_z: bool,
    pub prism_enabled: bool,
    pub rotation_enabled: bool,
    pub negate_w: bool,
    pub sphere_inversion_enabled: bool,
    pub start_a: u32,
    pub start_c: u32,
    pub start_e: u32,
    pub start_p: u32,
    pub start_r: u32,
    pub start_t: u32,
    pub start_x: u32,
    pub stop_one: u32,
    pub stop_a: u32,
    pub stop_c: u32,
    pub stop_e: u32,
    pub stop_p: u32,
    pub stop_r: u32,
    pub stop_t: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifsMsltoeDonut {
    pub factor: f64,
    pub number: f64,
    pub ring_radius: f64,
    pub ring_thickness: f64,
    pub color_enabled: bool,
    pub color_add: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDeLinearCube {
    pub mix_enabled: bool,
    pub euclidean_enabled: bool,
    pub mix: f64,
    pub scale: f64,
    pub offset: f64,
    pub color_enabled: bool,
    pub color_scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsGrid {
    pub size: f64,
    pub z_scale: f64,
    pub rotation_enabled: bool,
    pub rotation: [[f64; 3]; 3],
    pub square_cross_section: bool,
    pub radius: f64,
    pub color_enabled: bool,
    pub color_add_enabled: bool,
    pub color_base: [f64; 4],
    pub color_iteration_scale: f64,
    pub color_start: u32,
    pub color_stop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsBox {
    pub half_size: [f64; 4],
    pub distance_offset: f64,
    pub color_enabled: bool,
    pub color_add_enabled: bool,
    pub color_axis_enabled: bool,
    pub color_base: [f64; 4],
    pub color_iteration_scale: f64,
    pub color_octant_offset: f64,
    pub color_start: u32,
    pub color_stop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsEllipsoid {
    pub radii: [f64; 3],
    pub color_enabled: bool,
    pub color_add_enabled: bool,
    pub color_base: [f64; 4],
    pub color_iteration_scale: f64,
    pub color_start: u32,
    pub color_stop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsSphere {
    pub radius: f64,
    pub analytic_offset: f64,
    pub four_dimensional: bool,
    pub color_enabled: bool,
    pub color_add_enabled: bool,
    pub color_base: [f64; 4],
    pub color_iteration_scale: f64,
    pub color_start: u32,
    pub color_stop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsChessboard {
    pub color_disabled: bool,
    pub color_add_enabled: bool,
    pub color_three_dimensional: bool,
    pub color_offset: [f64; 4],
    pub color_repeats: [f64; 4],
    pub box_half_size: [f64; 4],
    pub plane_enabled: bool,
    pub mutate_orbit: bool,
    pub mutate_start: u32,
    pub mutate_stop: u32,
    pub replace_distance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsHextgrid2 {
    pub pre_transform_enabled: bool,
    pub pre_scale: f64,
    pub pre_offset: [f64; 4],
    pub negate_abs: [bool; 3],
    pub size: f64,
    pub z_scale: f64,
    pub rotation_enabled: bool,
    pub rotation: [[f64; 3]; 3],
    pub square_cross_section: bool,
    pub radius: f64,
    pub analytic_offset: f64,
    pub color_enabled: bool,
    pub color_add_enabled: bool,
    pub color_base: [f64; 4],
    pub color_iteration_scale: f64,
    pub color_start: u32,
    pub color_stop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransfDifsTorusV4 {
    pub transform_enabled: bool,
    pub transform_start: u32,
    pub transform_stop: u32,
    pub scale: f64,
    pub fold_start: u32,
    pub fold_stop: u32,
    pub absolute_enabled: bool,
    pub absolute_axes: [bool; 3],
    pub offset: [f64; 4],
    pub rotation_enabled: bool,
    pub rotation_start: u32,
    pub rotation_stop: u32,
    pub rotation: [[f64; 3]; 3],
    pub angle: f64,
    pub angle_sine: f64,
    pub angle_cosine: f64,
    pub major_radius: f64,
    pub hollow_enabled: bool,
    pub square_cross_section: bool,
    pub shell_radius: f64,
    pub surface_offset: f64,
    pub analytic_offset: f64,
    pub color_enabled: bool,
    pub color_add_enabled: bool,
    pub color_base: [f64; 4],
    pub color_iteration_scale: f64,
    pub color_start: u32,
    pub color_stop: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DifsMenger {
    pub box_fold: [f64; 4],
    pub absolute_enabled: [bool; 3],
    pub absolute_ranges: [[u32; 2]; 3],
    pub folds_enabled: bool,
    pub xy_fold_enabled: bool,
    pub xy_fold_range: [u32; 2],
    pub xyz_fold_enabled: bool,
    pub xyz_fold_range: [u32; 2],
    pub polyfold_enabled: bool,
    pub polyfold_range: [u32; 2],
    pub polyfold_sides: i32,
    pub diagonal_one_enabled: bool,
    pub diagonal_one_range: [u32; 2],
    pub x_offset_enabled: bool,
    pub x_offset_range: [u32; 2],
    pub x_offset: f64,
    pub y_offset_enabled: bool,
    pub y_offset_range: [u32; 2],
    pub y_offset: f64,
    pub diagonal_two_enabled: bool,
    pub diagonal_two_range: [u32; 2],
    pub reverse_x_range: [u32; 2],
    pub reverse_x_offset: f64,
    pub reverse_y_range: [u32; 2],
    pub reverse_y_offset: f64,
    pub scale_range: [u32; 2],
    pub scale: f64,
    pub scale_vary_enabled: bool,
    pub scale_vary_range: [u32; 2],
    pub scale_vary: f64,
    pub scale_target: f64,
    pub offset: [f64; 4],
    pub rotation_enabled: bool,
    pub rotation_range: [u32; 2],
    pub rotation: [[f64; 3]; 3],
    pub menger_range: [u32; 2],
    pub menger_initial_scale: f64,
    pub menger_iterations: i32,
    pub menger_offset: [f64; 4],
    pub menger_fold_offset: f64,
    pub menger_unconditional_fold: bool,
    pub menger_scale: f64,
    pub color_enabled: bool,
    pub color_detail_enabled: bool,
    pub color_replace: bool,
    pub color_range: [u32; 2],
    pub color_iteration_scale: f64,
    pub color_base: f64,
    pub color_factors: [f64; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mandelbox {
    pub scale: f64,
    pub folding_limit: f64,
    pub folding_value: f64,
    pub fixed_radius_squared: f64,
    pub minimum_radius_squared: f64,
    pub minimum_radius_factor: f64,
    pub offset: [f64; 4],
    pub color_factor: [f64; 3],
    pub color_sphere_min: f64,
    pub color_sphere_fixed: f64,
    pub rotations_enabled: bool,
    pub main_rotation_enabled: bool,
    pub main_rotation: [[f64; 3]; 3],
    pub rotations: [[[[f64; 3]; 3]; 3]; 2],
    pub inverse_rotations: [[[[f64; 3]; 3]; 3]; 2],
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum FormulaKernel {
    KaleidoscopicIfs(Box<KaleidoscopicIfs>),
    Mandelbulb {
        power: f64,
        alpha_angle: f64,
        beta_angle: f64,
    },
    MandelbulbPower2,
    Hypercomplex,
    Quaternion,
    JosKleinian(Box<JosKleinian>),
    PseudoKleinian(Box<PseudoKleinian>),
    DifsMsltoeDonut(DifsMsltoeDonut),
    TransfDeLinearCube(TransfDeLinearCube),
    TransfDifsGrid(Box<TransfDifsGrid>),
    TransfDifsBox(Box<TransfDifsBox>),
    TransfDifsEllipsoid(Box<TransfDifsEllipsoid>),
    TransfDifsSphere(Box<TransfDifsSphere>),
    TransfDifsChessboard(Box<TransfDifsChessboard>),
    TransfDifsHextgrid2(Box<TransfDifsHextgrid2>),
    TransfDifsTorusV4(Box<TransfDifsTorusV4>),
    DifsMenger(Box<DifsMenger>),
    QuickDudley {
        derivative_scale: f64,
        derivative_offset: f64,
    },
    Mandelbox(Box<Mandelbox>),
    MandelboxFast {
        scale: f64,
        fixed_radius_squared: f64,
        minimum_radius_squared: f64,
        minimum_radius_factor: f64,
        main_rotation_enabled: bool,
        main_rotation: [[f64; 3]; 3],
    },
}

impl FormulaKernel {
    pub fn evaluate(&self, state: &mut OrbitState) {
        match self {
            Self::KaleidoscopicIfs(parameters) => kaleidoscopic_ifs(state, **parameters),
            Self::Mandelbulb {
                power,
                alpha_angle,
                beta_angle,
            } => mandelbulb(state, *power, *alpha_angle, *beta_angle),
            Self::MandelbulbPower2 => mandelbulb_power2(state),
            Self::Hypercomplex => hypercomplex(state),
            Self::Quaternion => quaternion(state),
            Self::JosKleinian(parameters) => jos_kleinian(state, **parameters),
            Self::PseudoKleinian(parameters) => pseudo_kleinian(state, **parameters),
            Self::DifsMsltoeDonut(parameters) => difs_msltoe_donut(state, *parameters),
            Self::TransfDeLinearCube(parameters) => transf_de_linear_cube(state, *parameters),
            Self::TransfDifsGrid(parameters) => transf_difs_grid(state, **parameters),
            Self::TransfDifsBox(parameters) => transf_difs_box(state, **parameters),
            Self::TransfDifsEllipsoid(parameters) => transf_difs_ellipsoid(state, **parameters),
            Self::TransfDifsSphere(parameters) => transf_difs_sphere(state, **parameters),
            Self::TransfDifsChessboard(parameters) => transf_difs_chessboard(state, **parameters),
            Self::TransfDifsHextgrid2(parameters) => transf_difs_hextgrid2(state, **parameters),
            Self::TransfDifsTorusV4(parameters) => transf_difs_torus_v4(state, **parameters),
            Self::DifsMenger(parameters) => difs_menger(state, **parameters),
            Self::QuickDudley {
                derivative_scale,
                derivative_offset,
            } => quick_dudley(state, *derivative_scale, *derivative_offset),
            Self::Mandelbox(parameters) => mandelbox(state, **parameters),
            Self::MandelboxFast {
                scale,
                fixed_radius_squared,
                minimum_radius_squared,
                minimum_radius_factor,
                main_rotation_enabled,
                main_rotation,
            } => mandelbox_fast(
                state,
                *scale,
                *fixed_radius_squared,
                *minimum_radius_squared,
                *minimum_radius_factor,
                *main_rotation_enabled,
                *main_rotation,
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrbitProgram {
    pub max_iterations: u32,
    pub bailout: f64,
    pub add_constant: Option<ConstantAddition>,
    pub finalizer: AnalyticFinalizer,
    pub finalizer_parameters: FinalizerParameters,
    pub kernel: FormulaKernel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HybridOrbitSlot {
    pub kernel: FormulaKernel,
    pub add_constant: Option<ConstantAddition>,
    pub weight: f64,
    pub check_for_bailout: bool,
    pub bailout: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HybridOrbitProgram {
    pub sequence: Vec<u8>,
    pub slots: [Option<HybridOrbitSlot>; 9],
    pub finalizer: AnalyticFinalizer,
    pub finalizer_parameters: FinalizerParameters,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitSample {
    pub distance: f64,
    pub radius: f64,
    pub derivative: f64,
    pub iterations: u32,
    pub escaped: bool,
    pub z: [f64; 4],
    pub color: f64,
}

impl OrbitProgram {
    pub fn evaluate(&self, point: [f64; 4]) -> OrbitSample {
        let initial_scale = match &self.kernel {
            FormulaKernel::KaleidoscopicIfs(parameters) => parameters.scale,
            FormulaKernel::Mandelbulb { .. }
            | FormulaKernel::MandelbulbPower2
            | FormulaKernel::Hypercomplex
            | FormulaKernel::Quaternion
            | FormulaKernel::JosKleinian(_)
            | FormulaKernel::PseudoKleinian(_)
            | FormulaKernel::DifsMsltoeDonut(_)
            | FormulaKernel::TransfDeLinearCube(_)
            | FormulaKernel::TransfDifsGrid(_)
            | FormulaKernel::TransfDifsBox(_)
            | FormulaKernel::TransfDifsEllipsoid(_)
            | FormulaKernel::TransfDifsSphere(_)
            | FormulaKernel::TransfDifsChessboard(_)
            | FormulaKernel::TransfDifsHextgrid2(_)
            | FormulaKernel::TransfDifsTorusV4(_)
            | FormulaKernel::DifsMenger(_)
            | FormulaKernel::QuickDudley { .. }
            | FormulaKernel::Mandelbox(_)
            | FormulaKernel::MandelboxFast { .. } => 1.0,
        };
        let mut state = OrbitState::new(point, initial_scale);
        let mut escaped = false;
        let mut iterations = 0;
        for iteration in 0..self.max_iterations {
            state.iteration = iteration;
            state.old_z = state.z;
            self.kernel.evaluate(&mut state);
            if let Some(addition) = self.add_constant {
                add_constant(&mut state, addition);
            }
            state.radius = length4(state.z);
            iterations = iteration + 1;
            if !state.radius.is_finite() {
                state.z = state.old_z;
                state.radius = length4(state.z);
                break;
            }
            if state.radius > self.bailout {
                escaped = true;
                break;
            }
        }
        OrbitSample {
            distance: finalize_distance(self.finalizer, self.finalizer_parameters, &state),
            radius: state.radius,
            derivative: state.derivative,
            iterations,
            escaped,
            z: state.z,
            color: state.color,
        }
    }
}

impl HybridOrbitProgram {
    pub fn evaluate(&self, point: [f64; 4]) -> OrbitSample {
        let mut state = OrbitState::new(point, 1.0);
        let mut escaped = false;
        let mut iterations = 0;
        for (iteration, &slot_index) in self.sequence.iter().enumerate() {
            let slot = self.slots[usize::from(slot_index)]
                .as_ref()
                .expect("hybrid sequence only references populated slots");
            state.iteration = iteration as u32;
            state.old_z = state.z;
            let previous_z = state.z;
            let previous_derivative = state.derivative;
            let previous_color = state.color;
            if slot.weight > 0.0 {
                slot.kernel.evaluate(&mut state);
            }
            if let Some(addition) = slot.add_constant {
                add_constant(&mut state, addition);
            }
            if slot.weight < 1.0 {
                state.z = smooth_vector(previous_z, state.z, slot.weight);
                let inverse_weight = 1.0 - slot.weight;
                state.derivative =
                    state.derivative * slot.weight + previous_derivative * inverse_weight;
                state.color = state.color * slot.weight + previous_color * inverse_weight;
            }
            state.radius = length4(state.z);
            iterations = iteration as u32 + 1;
            if !state.radius.is_finite() {
                state.z = state.old_z;
                state.radius = length4(state.z);
                break;
            }
            if slot.check_for_bailout && state.radius > slot.bailout {
                escaped = true;
                break;
            }
        }
        OrbitSample {
            distance: finalize_distance(self.finalizer, self.finalizer_parameters, &state),
            radius: state.radius,
            derivative: state.derivative,
            iterations,
            escaped,
            z: state.z,
            color: state.color,
        }
    }
}

pub fn finalize_distance(
    finalizer: AnalyticFinalizer,
    parameters: FinalizerParameters,
    state: &OrbitState,
) -> f64 {
    // Custom-DIFS kernels own aux.dist and do not require a non-zero analytic
    // derivative. In particular, a preceding orbit formula can leave DE at
    // zero at the origin while a later custom transform still computes a
    // valid distance from DE + 1.
    if finalizer == AnalyticFinalizer::CustomDistance {
        return state.distance;
    }
    if state.derivative <= 0.0 {
        return state.radius;
    }
    match finalizer {
        AnalyticFinalizer::Logarithmic => {
            if state.radius > 1.0 {
                0.5 * state.radius * state.radius.ln() / state.derivative
            } else {
                0.0
            }
        }
        AnalyticFinalizer::Linear => (state.radius - parameters.offset1) / state.derivative,
        AnalyticFinalizer::Ifs => (state.radius - 2.0) / state.derivative,
        AnalyticFinalizer::PseudoKleinian => {
            let rxy = state.z[0].hypot(state.z[1]);
            (rxy - state.pseudo_kleinian_de).max((rxy * state.z[2]).abs() / state.radius)
                / state.derivative
        }
        AnalyticFinalizer::JosKleinian => {
            let y = if parameters.spheres_enabled {
                state.z[1].min(parameters.folding_value - state.z[1])
            } else {
                state.z[1]
            };
            y.min(parameters.tweak005) / state.derivative.max(parameters.offset1)
        }
        AnalyticFinalizer::CustomDistance => unreachable!("handled before the derivative guard"),
        AnalyticFinalizer::MaxAxis => {
            state.z[0].abs().max(state.z[1].abs()).max(state.z[2].abs()) / state.derivative
        }
        AnalyticFinalizer::None => -1.0,
        AnalyticFinalizer::Undefined => state.radius,
    }
}

fn add_constant(state: &mut OrbitState, addition: ConstantAddition) {
    let mut constant = addition.julia.unwrap_or(state.const_c);
    if addition.swap_xy {
        constant.swap(0, 1);
    }
    for (target, (value, multiplier)) in state
        .z
        .iter_mut()
        .zip(constant.into_iter().zip(addition.multiplier))
    {
        *target += value * multiplier;
    }
}

fn kaleidoscopic_ifs(state: &mut OrbitState, parameters: KaleidoscopicIfs) {
    for (component, enabled) in state.z[..3].iter_mut().zip(parameters.absolute) {
        if enabled {
            *component = component.abs();
        }
    }
    for index in 0..9 {
        if !parameters.enabled[index] {
            continue;
        }
        let rotated = matrix_vector(parameters.rotations[index], xyz(state.z));
        state.z[..3].copy_from_slice(&rotated);
        let projection = dot3(rotated, parameters.directions[index]);
        if projection < parameters.distances[index] {
            let amount =
                2.0 * (projection - parameters.distances[index]) * parameters.intensities[index];
            for (component, direction) in state.z[..3].iter_mut().zip(parameters.directions[index])
            {
                *component -= direction * amount;
            }
        }
    }
    if parameters.rotation_enabled {
        let relative = subtract3(xyz(state.z), parameters.offset);
        let rotated = add3(
            matrix_vector(parameters.main_rotation, relative),
            parameters.offset,
        );
        state.z[..3].copy_from_slice(&rotated);
    }
    if parameters.edge_enabled {
        for (component, edge) in state.z[..3].iter_mut().zip(parameters.edge) {
            if edge > 0.0 {
                *component = edge - (edge - *component).abs();
            }
        }
    }
    for component in &mut state.z {
        *component *= parameters.scale;
    }
    let offset_scale = parameters.scale - 1.0;
    state.z[0] -= parameters.offset[0] * offset_scale;
    state.z[1] -= parameters.offset[1] * offset_scale;
    if parameters.menger_sponge_mode {
        if state.z[2] > 0.5 * parameters.offset[2] * offset_scale {
            state.z[2] -= parameters.offset[2] * offset_scale;
        }
    } else {
        state.z[2] -= parameters.offset[2] * offset_scale;
    }
    state.derivative *= parameters.scale.abs();
}

fn mandelbulb_power2(state: &mut OrbitState) {
    state.derivative *= 2.0 * state.radius;
    let x2 = state.z[0] * state.z[0];
    let y2 = state.z[1] * state.z[1];
    let z2 = state.z[2] * state.z[2];
    let temp = 1.0 - z2 / (x2 + y2);
    let x = (x2 - y2) * temp;
    let y = 2.0 * state.z[0] * state.z[1] * temp;
    let z = -2.0 * state.z[2] * (x2 + y2).sqrt();
    state.z[0] = x;
    state.z[1] = y;
    state.z[2] = z;
}

fn mandelbulb(state: &mut OrbitState, power: f64, alpha_angle: f64, beta_angle: f64) {
    let theta0 = (state.z[2] / state.radius).asin() + beta_angle;
    let phi0 = state.z[1].atan2(state.z[0]) + alpha_angle;
    let mut radius_power = state.radius.powf(power - 1.0);
    let theta = theta0 * power;
    let phi = phi0 * power;
    state.derivative = radius_power * state.derivative * power + 1.0;
    radius_power *= state.radius;
    let cos_theta = theta.cos();
    state.z[0] = cos_theta * phi.cos() * radius_power;
    state.z[1] = cos_theta * phi.sin() * radius_power;
    state.z[2] = theta.sin() * radius_power;
}

fn hypercomplex(state: &mut OrbitState) {
    state.derivative *= 2.0 * state.radius;
    let [x, y, z, w] = state.z;
    state.z = [
        x * x - y * y - z * z - w * w,
        2.0 * x * y - 2.0 * w * z,
        2.0 * x * z - 2.0 * y * w,
        2.0 * x * w - 2.0 * y * z,
    ];
}

fn quaternion(state: &mut OrbitState) {
    state.derivative *= 2.0 * state.radius;
    let [x, y, z, _w] = state.z;
    state.z[0] = x * x - y * y - z * z;
    state.z[1] = 2.0 * x * y;
    state.z[2] = 2.0 * x * z;
}

fn jos_kleinian(state: &mut OrbitState, parameters: JosKleinian) {
    let old_y = state.z[1];
    let old_derivative = state.derivative;
    if parameters.sphere_inversion_enabled && state.iteration < 1 {
        for (component, offset) in state.z.iter_mut().zip(parameters.offset_zero) {
            *component += offset;
        }
        let radius_squared = dot4(state.z, state.z);
        let factor = parameters.max_radius_squared / radius_squared;
        for index in 0..4 {
            state.z[index] = state.z[index] * factor + parameters.addition[index]
                - parameters.offset_zero[index];
        }
        state.derivative *= factor * parameters.analytic_scale;
    }

    if state.iteration >= parameters.start_c && state.iteration < parameters.stop_c {
        let a = parameters.folding_value;
        let b = parameters.offset;
        let sign = if b > 0.0 {
            1.0
        } else if b < 0.0 {
            -1.0
        } else {
            0.0
        };
        let period = [
            2.0 * parameters.box_size[0],
            a * parameters.box_size[1],
            2.0 * parameters.box_size[2],
        ];
        let offset = [
            -parameters.box_size[0],
            -parameters.box_size[1] + 1.0,
            -parameters.box_size[2],
        ];
        for index in 0..3 {
            let shifted = state.z[index] - offset[index];
            state.z[index] =
                shifted - period[index] * (shifted / period[index]).floor() + offset[index];
        }
        let separation = a
            * (0.5
                + 0.2
                    * (sign * std::f64::consts::PI * (state.z[0] + b * 0.5)
                        / parameters.box_size[0])
                        .sin());
        if state.z[1] >= separation {
            state.z[0] = -b - state.z[0];
            state.z[1] = a - state.z[1];
            state.z[2] = -state.z[2];
            state.z[3] = 0.0;
        }
        let radius_squared = dot4(state.z, state.z);
        if parameters.color_enabled {
            let color_vector = [state.z[0], state.z[1], state.z[2], radius_squared];
            state.color = state.color.min(length4(color_vector));
        }
        let inverse_radius = 1.0 / radius_squared;
        for component in &mut state.z {
            *component *= -inverse_radius;
        }
        state.z[0] = -b - state.z[0];
        state.z[1] += a;
        state.derivative *= inverse_radius.abs();
    }

    if parameters.color_add_enabled
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let mut color_add = parameters.color_factors[0] * (old_derivative / state.derivative).abs()
            + parameters.color_factors[1] * state.z[1].abs()
            + parameters.color_factors[2] * (state.z[1] - old_y).abs();
        if parameters.color_grid_enabled
            && state.iteration >= parameters.start_t
            && state.iteration < parameters.stop_t
        {
            let grid = |coordinate: f64, size: f64, phase: f64, multiplier: f64| -> f64 {
                (((coordinate + size) / size + phase)
                    - ((coordinate + size) / size + phase).round())
                .abs()
                    * multiplier
            };
            let dd = grid(
                state.const_c[0],
                parameters.box_size[0] * parameters.scale_three[0],
                parameters.addition_p[0],
                parameters.constant_c[0],
            );
            let ee = grid(
                state.const_c[2],
                parameters.box_size[2] * parameters.scale_three[2],
                parameters.addition_p[2],
                parameters.constant_c[2],
            );
            let sum = dd + ee;
            let mut squared = dd * dd + ee * ee;
            if parameters.grid_y_enabled {
                squared += grid(
                    state.const_c[1],
                    parameters.box_size[1] * parameters.scale_three[1],
                    parameters.addition_p[1],
                    parameters.constant_c[1],
                );
            }
            let blended = sum * (1.0 - parameters.color_mix) + squared * parameters.color_mix;
            color_add += parameters.color_factors[3] * blended;
        }
        state.color += color_add;
    }
}

fn pseudo_kleinian(state: &mut OrbitState, parameters: PseudoKleinian) {
    let old_z = state.z[2];
    let in_range = |start: u32, stop: u32| state.iteration >= start && state.iteration < stop;

    if parameters.sphere_inversion_enabled && in_range(parameters.start_x, parameters.stop_one) {
        for (component, offset) in state.z.iter_mut().zip(parameters.offset_zero) {
            *component += offset;
        }
        let radius_squared = dot4(state.z, state.z);
        let factor = parameters.max_radius_squared / radius_squared;
        for index in 0..4 {
            state.z[index] = state.z[index] * factor + parameters.inversion_addition[index]
                - parameters.offset_zero[index];
        }
        state.derivative = state.derivative * factor + parameters.analytic_offset;
    }

    if parameters.prism_enabled && in_range(parameters.start_p, parameters.stop_p) {
        state.z[1] = state.z[1].abs();
        state.z[2] = state.z[2].abs();
        let projection =
            (state.z[0] * -0.866_025_403_784_438 + state.z[1] * 0.5) * parameters.prism_scale;
        let amount = projection.max(0.0);
        state.z[0] -= amount * -1.732_050_807_568_877_2;
        state.z[1] = (state.z[1] - amount).abs();
        if state.z[1] > state.z[2] {
            state.z.swap(1, 2);
        }
        let prism = [0.866_025_403_784_438, 1.5, 1.5, 0.0];
        for (index, factor) in prism.into_iter().enumerate() {
            state.z[index] -= parameters.prism_offset[index] * factor;
        }
        if state.z[2] > state.z[0] {
            state.z.swap(0, 2);
        }
        if state.z[0] > 0.0 {
            state.z[1] = state.z[1].max(0.0);
            state.z[2] = state.z[2].max(0.0);
        }
    }

    if parameters.tglad_fold_enabled && in_range(parameters.start_e, parameters.stop_e) {
        for axis in 0..2 {
            let limit = parameters.fold_size[axis];
            state.z[axis] =
                (state.z[axis] + limit).abs() - (state.z[axis] - limit).abs() - state.z[axis];
        }
        if parameters.fold_z_enabled {
            let limit = parameters.fold_size[2];
            state.z[2] = (state.z[2] + limit).abs() - (state.z[2] - limit).abs() - state.z[2];
        }
    }

    if parameters.box_fold_enabled && in_range(parameters.start_a, parameters.stop_a) {
        for axis in 0..2 {
            if state.z[axis].abs() > parameters.folding_limit {
                state.z[axis] = signed(state.z[axis]) * parameters.folding_value - state.z[axis];
                state.color += parameters.box_color[axis];
            }
        }
        let z_limit = parameters.folding_limit * parameters.z_fold_scale;
        let z_value = parameters.folding_value * parameters.z_fold_scale;
        if state.z[2].abs() > z_limit {
            state.z[2] = signed(state.z[2]) * z_value - state.z[2];
            state.color += parameters.box_color[2];
        }
    }

    let mut scale = 1.0;
    if in_range(parameters.start_c, parameters.stop_c) {
        let clamped = [
            state.z[0].clamp(-parameters.box_size[0], parameters.box_size[0]),
            state.z[1].clamp(-parameters.box_size[1], parameters.box_size[1]),
            state.z[2].clamp(-parameters.box_size[2], parameters.box_size[2]),
            state.z[3],
        ];
        for (component, bounded) in state.z.iter_mut().zip(clamped) {
            *component = bounded * 2.0 - *component;
        }
        scale = (parameters.minimum_radius / dot4(state.z, state.z)).max(1.0);
        for component in &mut state.z {
            *component *= scale;
        }
        if parameters.negate_z {
            state.z[2] = -state.z[2];
        }
        if parameters.negate_w {
            state.z[3] = -state.z[3];
        }
        state.derivative *= scale + parameters.analytic_tweak;
        state.pseudo_kleinian_de = parameters.analytic_scale;
    }

    if parameters.rotation_enabled && in_range(parameters.start_r, parameters.stop_r) {
        let rotated = matrix_vector(parameters.rotation, xyz(state.z));
        state.z[..3].copy_from_slice(&rotated);
    }
    for (component, addition) in state.z.iter_mut().zip(parameters.addition) {
        *component += addition;
    }

    if parameters.color_add_enabled
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let mut color_add = parameters.color_factors[0] * scale
            + parameters.color_factors[1] * state.z[2].abs()
            + parameters.color_factors[2] * (state.z[2] - old_z).abs();
        if parameters.color_grid_enabled && in_range(parameters.start_t, parameters.stop_t) {
            let grid = |coordinate: f64, size: f64, phase: f64, multiplier: f64| -> f64 {
                (((coordinate + size) / size + phase)
                    - ((coordinate + size) / size + phase).round())
                .abs()
                    * multiplier
            };
            let mut current = [0.0; 3];
            let mut constant = [0.0; 3];
            for axis in 0..3 {
                let size = 2.0 * parameters.box_size[axis] * parameters.grid_scale[axis];
                current[axis] = grid(
                    state.z[axis],
                    size,
                    parameters.grid_phase[axis],
                    parameters.grid_multiplier[axis],
                );
                constant[axis] = grid(
                    state.const_c[axis],
                    size,
                    parameters.grid_phase[axis],
                    parameters.grid_multiplier[axis],
                );
            }
            let (mut current_value, mut constant_value) = if parameters.squared_grid_enabled {
                (
                    current[0] * current[0] + current[1] * current[1],
                    constant[0] * constant[0] + constant[1] * constant[1],
                )
            } else {
                (current[0] + current[1], constant[0] + constant[1])
            };
            if parameters.grid_z_enabled {
                current_value += current[2];
                constant_value += constant[2];
            }
            let blended = constant_value * (1.0 - parameters.color_mix)
                + current_value * parameters.color_mix;
            color_add += parameters.color_factors[3] * blended;
        }
        state.color += color_add;
    }
}

fn signed(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn difs_msltoe_donut(state: &mut OrbitState, parameters: DifsMsltoeDonut) {
    let original = state.z;
    let sector = std::f64::consts::TAU / parameters.number;
    let radial = parameters.ring_radius - state.z[0].hypot(state.z[1]);
    let radial_squared = radial * radial;
    let mut threshold = radial_squared + state.z[2] * state.z[2]
        - parameters.ring_thickness * parameters.ring_thickness;
    let theta = sector * (state.z[1].atan2(state.z[0]) / sector).round();
    if threshold > 0.03 {
        let (sine, cosine) = theta.sin_cos();
        state.z[0] = cosine * original[0] + sine * state.z[1] - parameters.ring_radius;
        state.z[2] = -sine * original[0] + cosine * state.z[1];
        state.z[1] = original[2];
        for component in &mut state.z {
            *component *= parameters.factor;
        }
        state.derivative *= parameters.factor;
    } else {
        for component in &mut state.z {
            *component /= threshold;
        }
    }
    threshold = (radial_squared + original[2] * original[2]).sqrt() - parameters.ring_thickness;
    state.distance = state.distance.min(threshold / (state.derivative + 1.0));
    if parameters.color_enabled {
        state.color += parameters.color_add;
    }
}

fn transf_de_linear_cube(state: &mut OrbitState, parameters: TransfDeLinearCube) {
    let max_axis = state.z[0].abs().max(state.z[1].abs()).max(state.z[2].abs());
    let euclidean = length4(state.z);
    let radius = if parameters.mix_enabled {
        max_axis * (1.0 - parameters.mix) + euclidean * parameters.mix
    } else if parameters.euclidean_enabled {
        euclidean
    } else {
        max_axis
    };
    state.distance = parameters.scale * radius / state.derivative - parameters.offset / 100.0;
    if parameters.color_enabled {
        state.color = parameters.color_scale * state.derivative / radius;
    }
}

fn transf_difs_grid(state: &mut OrbitState, parameters: TransfDifsGrid) {
    let mut sample = state.z;
    sample[2] /= parameters.z_scale;
    if parameters.rotation_enabled {
        let rotated = matrix_vector(parameters.rotation, xyz(sample));
        sample[..3].copy_from_slice(&rotated);
    }

    let x_floor = (sample[0] - parameters.size * (sample[0] / parameters.size + 0.5).floor()).abs();
    let y_floor = (sample[1] - parameters.size * (sample[1] / parameters.size + 0.5).floor()).abs();
    let grid_xy = x_floor.min(y_floor);
    let grid = if parameters.square_cross_section {
        grid_xy.abs().max(sample[2].abs())
    } else {
        grid_xy.hypot(sample[2])
    };
    let previous_distance = state.distance;
    state.distance = state
        .distance
        .min((grid - parameters.radius) / (state.derivative + 1.0));

    if parameters.color_enabled
        && previous_distance != state.distance
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let mut add = parameters.color_base[0]
            + f64::from(state.iteration) * parameters.color_iteration_scale;
        if grid_xy != x_floor {
            add += parameters.color_base[1];
        }
        if parameters.color_add_enabled {
            state.color += add;
        } else {
            state.color = add;
        }
    }
}

fn transf_difs_box(state: &mut OrbitState, parameters: TransfDifsBox) {
    let mut clamped = std::array::from_fn(|axis| state.z[axis].abs() - parameters.half_size[axis]);
    for component in &mut clamped[..3] {
        *component = component.max(0.0);
    }
    let previous_distance = state.distance;
    state.distance = state
        .distance
        .min(length4(clamped) / (state.derivative + 1.0) - parameters.distance_offset);

    if parameters.color_enabled
        && previous_distance != state.distance
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let mut add = parameters.color_base[0]
            + f64::from(state.iteration) * parameters.color_iteration_scale;
        if parameters.color_axis_enabled {
            if clamped[0] > clamped[1].max(clamped[2]) {
                add += parameters.color_base[1];
            }
            if clamped[1] > clamped[0].max(clamped[2]) {
                add += parameters.color_base[2];
            }
            if clamped[2] > clamped[1].max(clamped[0]) {
                add += parameters.color_base[3];
            }
            if parameters.color_octant_offset != 0.0 {
                let xy = state.z[0] * state.z[1];
                if (xy > 0.0 && state.z[2] > 0.0) || (xy < 0.0 && state.z[2] < 0.0) {
                    add += parameters.color_octant_offset;
                }
            }
        }
        if parameters.color_add_enabled {
            state.color += add;
        } else {
            state.color = add;
        }
    }
}

fn transf_difs_ellipsoid(state: &mut OrbitState, parameters: TransfDifsEllipsoid) {
    let scaled = std::array::from_fn(|axis| state.z[axis] / parameters.radii[axis]);
    let scaled_twice = std::array::from_fn(|axis| scaled[axis] / parameters.radii[axis]);
    let radius = length3(scaled);
    let second_radius = length3(scaled_twice);
    let ellipsoid_distance = radius * (radius - 1.0) / second_radius;
    let previous_distance = state.distance;
    state.distance = state
        .distance
        .min(ellipsoid_distance / (state.derivative + 1.0));

    if parameters.color_enabled
        && previous_distance != state.distance
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let add = parameters.color_base[3]
            + f64::from(state.iteration) * parameters.color_iteration_scale;
        if parameters.color_add_enabled {
            state.color += add;
        } else {
            state.color = add;
        }
    }
}

fn transf_difs_sphere(state: &mut OrbitState, parameters: TransfDifsSphere) {
    let radius = if parameters.four_dimensional {
        length4(state.z)
    } else {
        length3(xyz(state.z))
    };
    let sphere_distance = radius - parameters.radius;
    let previous_distance = state.distance;
    state.distance = state
        .distance
        .min(sphere_distance / (state.derivative + parameters.analytic_offset));
    state.derivative0 = sphere_distance;

    if parameters.color_enabled
        && previous_distance != state.distance
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let add = parameters.color_base[1]
            + f64::from(state.iteration) * parameters.color_iteration_scale;
        if parameters.color_add_enabled {
            state.color += add;
        } else {
            state.color = add;
        }
    }
}

fn transf_difs_chessboard(state: &mut OrbitState, parameters: TransfDifsChessboard) {
    let mut transformed = state.z;
    if !parameters.color_disabled {
        let colored: [f64; 4] =
            std::array::from_fn(|axis| transformed[axis] + parameters.color_offset[axis]);
        let repeat = std::array::from_fn::<_, 3, _>(|axis| {
            (parameters.color_repeats[axis] * colored[axis]).floor()
        });
        let total = if parameters.color_three_dimensional {
            repeat[0] + repeat[1] + repeat[2]
        } else {
            repeat[0] + repeat[1]
        };
        let color = (total * 0.5 - (total * 0.5).floor()) * 2.0;
        if parameters.color_add_enabled {
            state.color += color;
        } else {
            state.color = color;
        }
    }

    let distance = if parameters.plane_enabled {
        transformed[2]
    } else {
        for (component, half_size) in transformed.iter_mut().zip(parameters.box_half_size) {
            *component = component.abs() - half_size;
        }
        transformed[0].max(transformed[1].max(transformed[2]))
    };
    if parameters.mutate_orbit
        && state.iteration >= parameters.mutate_start
        && state.iteration < parameters.mutate_stop
    {
        state.z = transformed;
    }
    if parameters.replace_distance {
        state.distance = distance;
    } else {
        state.distance = state.distance.min(distance);
    }
}

fn transf_difs_hextgrid2(state: &mut OrbitState, parameters: TransfDifsHextgrid2) {
    if parameters.pre_transform_enabled {
        for component in &mut state.z {
            *component *= parameters.pre_scale;
        }
        state.derivative *= parameters.pre_scale.abs();
        for (component, offset) in state.z.iter_mut().zip(parameters.pre_offset) {
            *component += offset;
        }
        for (axis, enabled) in parameters.negate_abs.into_iter().enumerate() {
            if enabled {
                state.z[axis] = -state.z[axis].abs();
            }
        }
    }

    let mut sample = state.z;
    if parameters.rotation_enabled {
        let rotated = matrix_vector(parameters.rotation, xyz(sample));
        sample[..3].copy_from_slice(&rotated);
    }
    sample[2] /= parameters.z_scale;
    let cos_pi_six = (std::f64::consts::PI / 6.0).cos();
    let sin_pi_six = 0.5;
    let y_floor = (sample[1] - parameters.size * (sample[1] / parameters.size + 0.5).floor()).abs();
    let x_floor = (sample[0]
        - parameters.size * 1.5 / cos_pi_six
            * (sample[0] / parameters.size / 1.5 * cos_pi_six + 0.5).floor())
    .abs();
    let grid_max = y_floor.max(x_floor * cos_pi_six + y_floor * sin_pi_six);
    let grid_min = (grid_max - parameters.size * 0.5).min(y_floor);
    let hex_distance = if parameters.square_cross_section {
        grid_min.abs().max(sample[2].abs())
    } else {
        grid_min.hypot(sample[2])
    };
    let previous_distance = state.distance;
    state.distance = state
        .distance
        .min((hex_distance - parameters.radius) / (state.derivative + parameters.analytic_offset));
    if parameters.color_enabled
        && previous_distance != state.distance
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let add = parameters.color_base[1]
            + f64::from(state.iteration) * parameters.color_iteration_scale;
        if parameters.color_add_enabled {
            state.color += add;
        } else {
            state.color = add;
        }
    }
}

fn transf_difs_torus_v4(state: &mut OrbitState, parameters: TransfDifsTorusV4) {
    if parameters.transform_enabled
        && state.iteration >= parameters.transform_start
        && state.iteration < parameters.transform_stop
    {
        for component in &mut state.z {
            *component *= parameters.scale;
        }
        state.derivative *= parameters.scale.abs();
        if state.iteration >= parameters.fold_start && state.iteration < parameters.fold_stop {
            if parameters.absolute_enabled {
                for (axis, enabled) in parameters.absolute_axes.into_iter().enumerate() {
                    if enabled {
                        state.z[axis] = state.z[axis].abs();
                    }
                }
            }
            for (component, offset) in state.z.iter_mut().zip(parameters.offset) {
                *component -= offset;
            }
        }
        if parameters.rotation_enabled
            && state.iteration >= parameters.rotation_start
            && state.iteration < parameters.rotation_stop
        {
            let rotated = matrix_vector(parameters.rotation, xyz(state.z));
            state.z[..3].copy_from_slice(&rotated);
        }
    }

    if parameters.angle != 0.0 {
        let original_x = state.z[0];
        state.z[0] = state.z[0] * parameters.angle_cosine - state.z[1] * parameters.angle_sine;
        state.z[1] = original_x * parameters.angle_sine + state.z[1] * parameters.angle_cosine;
        state.z[0] = state.z[0].abs();
        let folded_x = state.z[0];
        state.z[0] = state.z[0] * parameters.angle_cosine - state.z[1] * parameters.angle_sine;
        state.z[1] = folded_x * parameters.angle_sine + state.z[1] * parameters.angle_cosine;
    }

    let radial = state.z[1].hypot(state.z[0]) - parameters.major_radius;
    let surface_adjustment = if parameters.hollow_enabled {
        parameters.shell_radius - parameters.surface_offset
    } else {
        -parameters.surface_offset
    };
    let mut cross_distance = if parameters.square_cross_section {
        radial.abs().max(state.z[2].abs()) + surface_adjustment
    } else {
        radial.hypot(state.z[2]) + surface_adjustment
    };
    let pre_shell_distance = cross_distance;
    if parameters.hollow_enabled {
        cross_distance = (cross_distance.abs() - parameters.shell_radius).max(0.0);
    }
    let torus_distance = cross_distance.max(-state.z[1]);
    let previous_distance = state.distance;
    state.distance = state
        .distance
        .min(torus_distance / (state.derivative + parameters.analytic_offset));
    if parameters.color_enabled
        && previous_distance != state.distance
        && state.iteration >= parameters.color_start
        && state.iteration < parameters.color_stop
    {
        let mut add = parameters.color_base[0]
            + parameters.color_iteration_scale * f64::from(state.iteration);
        if cross_distance >= pre_shell_distance + parameters.shell_radius {
            add += parameters.color_base[2];
        }
        if torus_distance == -state.z[1] {
            add += parameters.color_base[3];
        }
        if parameters.color_add_enabled {
            state.color += add;
        } else {
            state.color = add;
        }
    }
}

fn difs_menger(state: &mut OrbitState, parameters: DifsMenger) {
    let old_z = state.z;
    for axis in 0..3 {
        if parameters.absolute_enabled[axis]
            && iteration_in(state.iteration, parameters.absolute_ranges[axis])
        {
            state.z[axis] = state.z[axis].abs();
        }
    }

    if parameters.folds_enabled {
        if parameters.xy_fold_enabled && iteration_in(state.iteration, parameters.xy_fold_range) {
            state.z[0] -= parameters.box_fold[0];
            state.z[1] -= parameters.box_fold[1];
        }
        if parameters.xyz_fold_enabled && iteration_in(state.iteration, parameters.xyz_fold_range) {
            for (component, fold) in state.z.iter_mut().zip(parameters.box_fold) {
                *component -= fold;
            }
        }
        if parameters.polyfold_enabled && iteration_in(state.iteration, parameters.polyfold_range) {
            state.z[0] = state.z[0].abs();
            let sector = std::f64::consts::PI / f64::from(parameters.polyfold_sides);
            let angle = ((state.z[1].atan2(state.z[0]) + sector) % (2.0 * sector) - sector).abs();
            let length = state.z[0].hypot(state.z[1]);
            state.z[0] = angle.cos() * length;
            state.z[1] = angle.sin() * length;
        }
        if parameters.diagonal_one_enabled
            && iteration_in(state.iteration, parameters.diagonal_one_range)
            && state.z[0] > state.z[1]
        {
            state.z.swap(0, 1);
        }
        if parameters.x_offset_enabled
            && iteration_in(state.iteration, parameters.x_offset_range)
            && state.z[0] < parameters.x_offset
        {
            state.z[0] = (state.z[0] - parameters.x_offset).abs() + parameters.x_offset;
        }
        if parameters.y_offset_enabled
            && iteration_in(state.iteration, parameters.y_offset_range)
            && state.z[1] < parameters.y_offset
        {
            state.z[1] = (state.z[1] - parameters.y_offset).abs() + parameters.y_offset;
        }
        if parameters.diagonal_two_enabled
            && iteration_in(state.iteration, parameters.diagonal_two_range)
            && state.z[0] > state.z[1]
        {
            state.z.swap(0, 1);
        }
    }

    if iteration_in(state.iteration, parameters.reverse_x_range) {
        state.z[0] -= parameters.reverse_x_offset;
    }
    if iteration_in(state.iteration, parameters.reverse_y_range) {
        state.z[1] -= parameters.reverse_y_offset;
    }
    if iteration_in(state.iteration, parameters.scale_range) {
        let scale = state.actual_scale_a + parameters.scale;
        for component in &mut state.z {
            *component *= scale;
        }
        state.derivative = state.derivative * scale.abs() + 1.0;
        if parameters.scale_vary_enabled
            && iteration_in(state.iteration, parameters.scale_vary_range)
        {
            let vary =
                parameters.scale_vary * (state.actual_scale_a.abs() - parameters.scale_target);
            state.actual_scale_a -= vary;
        }
    }
    if iteration_in(state.iteration, parameters.reverse_x_range) {
        state.z[0] += parameters.reverse_x_offset;
    }
    if iteration_in(state.iteration, parameters.reverse_y_range) {
        state.z[1] += parameters.reverse_y_offset;
    }
    for (component, offset) in state.z.iter_mut().zip(parameters.offset) {
        *component += offset;
    }
    if parameters.rotation_enabled && iteration_in(state.iteration, parameters.rotation_range) {
        let rotated = matrix_vector(parameters.rotation, xyz(state.z));
        state.z[..3].copy_from_slice(&rotated);
    }

    let previous_distance = state.distance;
    let mut sample = old_z;
    if iteration_in(state.iteration, parameters.menger_range) {
        for component in &mut sample[..3] {
            *component /= parameters.menger_initial_scale;
        }
        state.pseudo_kleinian_de = 1.0;
        let mut radius = 0.0;
        for _ in 0..parameters.menger_iterations.max(0) {
            if radius >= 10.0 {
                break;
            }
            for (component, offset) in sample.iter_mut().zip(parameters.menger_offset) {
                *component = (*component + offset).abs();
            }
            if sample[1] > sample[0] {
                sample.swap(0, 1);
            }
            if sample[1] > sample[2] {
                sample.swap(1, 2);
            }
            let fold = 1.0 / parameters.menger_fold_offset;
            if parameters.menger_unconditional_fold || sample[1] < fold {
                sample[1] = (sample[1] - fold).abs() + fold;
            }
            for component in &mut sample[..3] {
                *component = (*component - 1.0) * parameters.menger_scale + 1.0;
            }
            sample[3] *= parameters.menger_scale;
            state.pseudo_kleinian_de *= parameters.menger_scale;
            radius = length4(sample);
        }
        sample[0] = sample[0].abs() - 1.0;
        sample[1] = sample[1].abs() - 1.0;
        sample[2] = sample[2].abs() - 1.0;
        sample[3] = sample[3].abs();
        let mut distance = sample[0].max(sample[1].max(sample[2]));
        if distance > 0.0 {
            sample[0] = sample[0].max(0.0);
            sample[1] = sample[1].max(0.0);
            sample[2] = sample[2].max(0.0);
            distance = length4(sample);
        }
        distance *= parameters.menger_initial_scale;
        distance /= state.pseudo_kleinian_de;
        state.distance = state.distance.min(distance / state.derivative);
    }

    if parameters.color_enabled
        && previous_distance != state.distance
        && iteration_in(state.iteration, parameters.color_range)
    {
        let mut color =
            f64::from(state.iteration) * parameters.color_iteration_scale + parameters.color_base;
        if parameters.color_detail_enabled {
            for component in &mut sample {
                *component = component.abs();
            }
            color += parameters.color_factors[0] * sample[0] * sample[1];
            color += parameters.color_factors[1] * sample[0].max(sample[1]);
        }
        if parameters.color_replace {
            state.color = color;
        } else {
            state.color += color;
        }
    }
}

fn iteration_in(iteration: u32, range: [u32; 2]) -> bool {
    iteration >= range[0] && iteration < range[1]
}

fn quick_dudley(state: &mut OrbitState, derivative_scale: f64, derivative_offset: f64) {
    state.derivative = state.derivative * 2.0 * state.radius * derivative_scale + derivative_offset;
    let [x, y, z, _w] = state.z;
    state.z[0] = x * x - 2.0 * y * z;
    state.z[1] = z * z + 2.0 * x * y;
    state.z[2] = y * y + 2.0 * x * z;
}

fn mandelbox_fast(
    state: &mut OrbitState,
    scale: f64,
    fixed_radius_squared: f64,
    minimum_radius_squared: f64,
    minimum_radius_factor: f64,
    main_rotation_enabled: bool,
    main_rotation: [[f64; 3]; 3],
) {
    for component in &mut state.z {
        *component = (*component + 1.0).abs() - (*component - 1.0).abs() - *component;
    }
    let radius_squared = state.z.iter().map(|value| value * value).sum::<f64>();
    if radius_squared < minimum_radius_squared {
        for component in &mut state.z {
            *component *= minimum_radius_factor;
        }
        state.derivative *= minimum_radius_factor;
    } else if radius_squared < fixed_radius_squared {
        let factor = fixed_radius_squared / radius_squared;
        for component in &mut state.z {
            *component *= factor;
        }
        state.derivative *= factor;
    }
    if main_rotation_enabled {
        let rotated = matrix_vector(main_rotation, xyz(state.z));
        state.z[..3].copy_from_slice(&rotated);
    }
    for component in &mut state.z {
        *component *= scale;
    }
    state.derivative = state.derivative * scale.abs() + 1.0;
}

fn mandelbox(state: &mut OrbitState, parameters: Mandelbox) {
    if parameters.rotations_enabled {
        for axis in 0..3 {
            let mut rotated = matrix_vector(parameters.rotations[0][axis], xyz(state.z));
            if rotated[axis] > parameters.folding_limit {
                rotated[axis] = parameters.folding_value - rotated[axis];
                let folded = matrix_vector(parameters.inverse_rotations[0][axis], rotated);
                state.z[..3].copy_from_slice(&folded);
                state.color += parameters.color_factor[axis];
            } else {
                rotated = matrix_vector(parameters.rotations[1][axis], xyz(state.z));
                if rotated[axis] < -parameters.folding_limit {
                    rotated[axis] = -parameters.folding_value - rotated[axis];
                    let folded = matrix_vector(parameters.inverse_rotations[1][axis], rotated);
                    state.z[..3].copy_from_slice(&folded);
                    state.color += parameters.color_factor[axis];
                }
            }
        }
    } else {
        for axis in 0..3 {
            if state.z[axis].abs() > parameters.folding_limit {
                state.z[axis] = state.z[axis].signum() * parameters.folding_value - state.z[axis];
                state.color += parameters.color_factor[axis];
            }
        }
    }

    let radius_squared = state.z.iter().map(|value| value * value).sum::<f64>();
    for (component, offset) in state.z.iter_mut().zip(parameters.offset) {
        *component += offset;
    }
    if radius_squared < parameters.minimum_radius_squared {
        for component in &mut state.z {
            *component *= parameters.minimum_radius_factor;
        }
        state.derivative *= parameters.minimum_radius_factor;
        state.color += parameters.color_sphere_min;
    } else if radius_squared < parameters.fixed_radius_squared {
        let factor = parameters.fixed_radius_squared / radius_squared;
        for component in &mut state.z {
            *component *= factor;
        }
        state.derivative *= factor;
        state.color += parameters.color_sphere_fixed;
    }
    for (component, offset) in state.z.iter_mut().zip(parameters.offset) {
        *component -= offset;
    }
    if parameters.main_rotation_enabled {
        let rotated = matrix_vector(parameters.main_rotation, xyz(state.z));
        state.z[..3].copy_from_slice(&rotated);
    }
    for component in &mut state.z {
        *component *= parameters.scale;
    }
    state.derivative = state.derivative * parameters.scale.abs() + 1.0;
}

fn xyz(value: [f64; 4]) -> [f64; 3] {
    [value[0], value[1], value[2]]
}

fn length4(value: [f64; 4]) -> f64 {
    dot4(value, value).sqrt()
}

fn length3(value: [f64; 3]) -> f64 {
    dot3(value, value).sqrt()
}

fn dot4(left: [f64; 4], right: [f64; 4]) -> f64 {
    left.into_iter().zip(right).map(|(a, b)| a * b).sum()
}

fn smooth_vector(first: [f64; 4], second: [f64; 4], weight: f64) -> [f64; 4] {
    if weight <= 0.0 {
        return first;
    }
    if weight >= 1.0 {
        return second;
    }
    let inverse_weight = 1.0 - weight;
    let interpolated_length = length4(first) * inverse_weight + length4(second) * weight;
    let blended =
        std::array::from_fn(|index| first[index] * inverse_weight + second[index] * weight);
    let blended_length = length4(blended);
    if blended_length > 0.0 {
        blended.map(|component| component * interpolated_length / blended_length)
    } else {
        first
    }
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn add3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn subtract3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn matrix_vector(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    [
        dot3(matrix[0], vector),
        dot3(matrix[1], vector),
        dot3(matrix[2], vector),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_analytic_finalizers_have_defined_results() {
        let mut state = OrbitState::new([2.0, 3.0, 4.0, 0.0], 1.0);
        state.derivative = 2.0;
        state.distance = 0.25;
        for finalizer in [
            AnalyticFinalizer::Logarithmic,
            AnalyticFinalizer::Linear,
            AnalyticFinalizer::Ifs,
            AnalyticFinalizer::PseudoKleinian,
            AnalyticFinalizer::JosKleinian,
            AnalyticFinalizer::CustomDistance,
            AnalyticFinalizer::MaxAxis,
            AnalyticFinalizer::None,
            AnalyticFinalizer::Undefined,
        ] {
            assert!(
                finalize_distance(finalizer, FinalizerParameters::default(), &state).is_finite()
            );
        }
    }

    #[test]
    fn custom_distance_does_not_depend_on_the_analytic_derivative() {
        let mut state = OrbitState::new([0.0; 4], 1.0);
        state.derivative = 0.0;
        state.distance = 0.125;
        assert_eq!(
            finalize_distance(
                AnalyticFinalizer::CustomDistance,
                FinalizerParameters::default(),
                &state
            ),
            0.125
        );
    }

    #[test]
    fn compact_difs_kernels_match_distance_color_and_mutation_equations() {
        let mut box_state = OrbitState::new([2.0, -0.5, 1.5, 0.0], 1.0);
        box_state.iteration = 2;
        FormulaKernel::TransfDifsBox(Box::new(TransfDifsBox {
            half_size: [1.0, 1.0, 1.0, 0.0],
            distance_offset: 0.1,
            color_enabled: true,
            color_add_enabled: true,
            color_axis_enabled: true,
            color_base: [0.1, 0.2, 0.3, 0.4],
            color_iteration_scale: 0.05,
            color_octant_offset: 0.25,
            color_start: 0,
            color_stop: 3,
        }))
        .evaluate(&mut box_state);
        assert!((box_state.distance - (1.25_f64.sqrt() / 2.0 - 0.1)).abs() < 1.0e-12);
        assert!((box_state.color - 1.4).abs() < 1.0e-12);

        let mut ellipsoid_state = OrbitState::new([1.0, 0.0, 0.0, 0.0], 1.0);
        ellipsoid_state.iteration = 3;
        FormulaKernel::TransfDifsEllipsoid(Box::new(TransfDifsEllipsoid {
            radii: [2.0, 1.0, 1.0],
            color_enabled: true,
            color_add_enabled: false,
            color_base: [0.0, 0.0, 0.0, 0.4],
            color_iteration_scale: 0.1,
            color_start: 0,
            color_stop: 4,
        }))
        .evaluate(&mut ellipsoid_state);
        assert!((ellipsoid_state.distance + 0.5).abs() < 1.0e-12);
        assert!((ellipsoid_state.color - 0.7).abs() < 1.0e-12);

        let mut sphere_state = OrbitState::new([0.0, 0.0, 0.0, 0.5], 1.0);
        sphere_state.iteration = 1;
        FormulaKernel::TransfDifsSphere(Box::new(TransfDifsSphere {
            radius: 0.9,
            analytic_offset: 0.25,
            four_dimensional: true,
            color_enabled: true,
            color_add_enabled: true,
            color_base: [0.0, 0.2, 0.0, 0.0],
            color_iteration_scale: 0.05,
            color_start: 0,
            color_stop: 2,
        }))
        .evaluate(&mut sphere_state);
        assert!((sphere_state.distance + 0.32).abs() < 1.0e-12);
        assert!((sphere_state.derivative0 + 0.4).abs() < 1.0e-12);
        assert!((sphere_state.color - 1.25).abs() < 1.0e-12);

        let mut chessboard_state = OrbitState::new([1.5, -0.5, 0.2, 0.0], 1.0);
        FormulaKernel::TransfDifsChessboard(Box::new(TransfDifsChessboard {
            color_disabled: false,
            color_add_enabled: false,
            color_three_dimensional: true,
            color_offset: [0.0; 4],
            color_repeats: [1.0; 4],
            box_half_size: [1.0, 1.0, 0.1, 0.0],
            plane_enabled: false,
            mutate_orbit: true,
            mutate_start: 0,
            mutate_stop: 2,
            replace_distance: true,
        }))
        .evaluate(&mut chessboard_state);
        for (actual, expected) in chessboard_state.z.into_iter().zip([0.5, -0.5, 0.1, 0.0]) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        assert_eq!(chessboard_state.distance, 0.5);
        assert_eq!(chessboard_state.color, 0.0);
    }

    #[test]
    fn power2_kernel_matches_the_source_equations() {
        let mut state = OrbitState::new([1.0, 2.0, 3.0, 0.0], 1.0);
        FormulaKernel::MandelbulbPower2.evaluate(&mut state);
        assert_eq!(state.derivative, 2.0 * 14.0_f64.sqrt());
        assert!((state.z[0] - 2.4).abs() < 1.0e-12);
        assert!((state.z[1] + 3.2).abs() < 1.0e-12);
        assert!((state.z[2] + 6.0 * 5.0_f64.sqrt()).abs() < 1.0e-12);
    }

    #[test]
    fn classic_mandelbulb_kernel_matches_the_source_equations() {
        let mut state = OrbitState::new([1.0, 2.0, 3.0, 0.0], 1.0);
        FormulaKernel::Mandelbulb {
            power: 2.0,
            alpha_angle: 0.0,
            beta_angle: 0.0,
        }
        .evaluate(&mut state);
        let radius = 14.0_f64.sqrt();
        assert!((state.derivative - (2.0 * radius + 1.0)).abs() < 1.0e-12);
        assert!((length4(state.z) - 14.0).abs() < 1.0e-12);
    }

    #[test]
    fn hypercomplex_and_quaternion_match_the_source_equations() {
        let mut hyper = OrbitState::new([1.0, 2.0, 3.0, 4.0], 1.0);
        FormulaKernel::Hypercomplex.evaluate(&mut hyper);
        assert_eq!(hyper.z, [-28.0, -20.0, -10.0, -4.0]);
        assert_eq!(hyper.derivative, 2.0 * 30.0_f64.sqrt());

        let mut quaternion = OrbitState::new([1.0, 2.0, 3.0, 4.0], 1.0);
        FormulaKernel::Quaternion.evaluate(&mut quaternion);
        assert_eq!(quaternion.z, [-12.0, 4.0, 6.0, 4.0]);
        assert_eq!(quaternion.derivative, 2.0 * 30.0_f64.sqrt());
    }

    #[test]
    fn quick_dudley_uses_the_analytic_de_parameters() {
        let mut state = OrbitState::new([1.0, 2.0, 3.0, 0.0], 1.0);
        FormulaKernel::QuickDudley {
            derivative_scale: 1.25,
            derivative_offset: 0.5,
        }
        .evaluate(&mut state);
        assert_eq!(state.z, [-11.0, 13.0, 10.0, 0.0]);
        assert!((state.derivative - (2.0 * 14.0_f64.sqrt() * 1.25 + 0.5)).abs() < 1.0e-12);
    }

    #[test]
    fn mandelbox_fast_uses_derived_radius_factors() {
        let mut state = OrbitState::new([0.1, 0.2, 0.3, 0.0], 1.0);
        FormulaKernel::MandelboxFast {
            scale: -1.5,
            fixed_radius_squared: 1.0,
            minimum_radius_squared: 0.25,
            minimum_radius_factor: 4.0,
            main_rotation_enabled: false,
            main_rotation: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
        .evaluate(&mut state);
        for (actual, expected) in state.z.into_iter().zip([-0.6, -1.2, -1.8, 0.0]) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        assert_eq!(state.derivative, 7.0);
    }

    #[test]
    fn full_mandelbox_matches_the_unrotated_fold_equations() {
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut state = OrbitState::new([0.1, 0.2, 0.3, 0.0], 1.0);
        FormulaKernel::Mandelbox(Box::new(Mandelbox {
            scale: -1.5,
            folding_limit: 1.0,
            folding_value: 2.0,
            fixed_radius_squared: 1.0,
            minimum_radius_squared: 0.25,
            minimum_radius_factor: 4.0,
            offset: [0.0; 4],
            color_factor: [0.03, 0.05, 0.07],
            color_sphere_min: 0.2,
            color_sphere_fixed: 0.2,
            rotations_enabled: false,
            main_rotation_enabled: false,
            main_rotation: identity,
            rotations: [[identity; 3]; 2],
            inverse_rotations: [[identity; 3]; 2],
        }))
        .evaluate(&mut state);
        for (actual, expected) in state.z.into_iter().zip([-0.6, -1.2, -1.8, 0.0]) {
            assert!((actual - expected).abs() < 1.0e-12);
        }
        assert_eq!(state.derivative, 7.0);
        assert!((state.color - 1.2).abs() < 1.0e-12);
    }
}
