// Diagnostic adapter to an external Mandelbulber build. No formula implementation
// is copied here; linked binaries inherit the external project's GPL obligations.
#include <QCoreApplication>
#include <cmath>
#include <fstream>
#include <iomanip>
#include <iostream>
#include "src/calculate_distance.hpp"
#include "src/camera_target.hpp"
#include "src/compute_fractal.hpp"
#include "src/fractal_container.hpp"
#include "src/fractal.h"
#include "src/fractparams.hpp"
#include "src/initparameters.hpp"
#include "src/nine_fractals.hpp"
#include "src/parameters.hpp"
#include "src/projection_3d.hpp"
#include "src/settings.hpp"
#include "src/system_data.hpp"
#include "src/render_worker.hpp"
#include "src/render_data.hpp"
#include "src/common_math.h"

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    systemData.locale = QLocale::c();
    systemData.locale.setNumberOptions(QLocale::OmitGroupSeparator);
    systemData.decimalPoint = ".";
    QLocale::setDefault(systemData.locale);
    if (argc != 4 && argc != 5) {
        std::cerr << "usage: native_distance_probe scene.fract points.tsv NEW-output.tsv [primary|boolean-primary|boolean-normal|march|hybrid-march|analytic-field|metal85-delta|orbit85|rays|ifs10-config]\n";
        return 2;
    }
    if (std::ifstream(argv[3]).good()) return 3;
    auto parameters = std::make_shared<cParameterContainer>();
    auto formulas = std::make_shared<cFractalContainer>();
    parameters->SetContainerName("main");
    InitParams(parameters);
    for (int i = 0; i < NUMBER_OF_FRACTALS; ++i) {
        formulas->at(i)->SetContainerName(QString("fractal") + QString::number(i));
        InitFractalParams(formulas->at(i));
    }
    DefineFractalList(&newFractalList);
    cSettings settings(cSettings::formatFullText);
    settings.BeQuiet(true);
    if (!settings.LoadFromFile(argv[1]) || !settings.Decode(parameters, formulas)) return 4;
    // Replay main-container CLI overrides through the same QString Set path.
    // This is only for auditing effective native capture configuration.
    const QString overridePath = qEnvironmentVariable("FPT_NATIVE_PROBE_OVERRIDES_FILE");
    if (!overridePath.isEmpty()) {
        std::ifstream overrides(overridePath.toStdString());
        if (!overrides) return 14;
        std::string line;
        while (std::getline(overrides, line)) {
            auto split = line.find('=');
            if (split == std::string::npos || split == 0) return 14;
            parameters->Set(QString::fromStdString(line.substr(0, split)).trimmed(),
                QString::fromStdString(line.substr(split + 1)).trimmed());
        }
        if (!overrides.eof()) return 14;
    }
    cSettings resolved(cSettings::formatFullText);
    resolved.CreateText(parameters, formulas);
    if (!resolved.SaveToFile(QString(argv[3]) + ".settings")) return 12;
    sParamRender config(parameters);
    cNineFractals fractals(formulas, parameters);
    std::string mode = argc == 5 ? argv[4] : "distance";
    bool metal_delta = mode == "metal85-delta", orbit = mode == "orbit85";
    bool rays = mode == "rays";
    bool boolean_points = mode == "boolean-primary" || mode == "boolean-normal";
    bool primary = mode == "primary" || mode == "boolean-primary";
    bool hybrid_march = mode == "hybrid-march";
    bool march = mode == "march" || hybrid_march;
    bool analytic_field = mode == "analytic-field";
    bool ifs_config = mode == "ifs10-config";
    if (ifs_config && parameters->Get<int>("formula_1") != 10) return 11;
    if (analytic_field && fractals.GetDEType(-1) != fractal::analyticDEType) return 11;
    if (argc == 5 && !boolean_points && !march && !analytic_field && !primary && !rays && !ifs_config && ((!metal_delta && !orbit) || parameters->Get<int>("formula_1") != 85)) return 11;
    // This adapter intentionally has no textures, primitive or object-tree data.
    if (config.objectsTreeEnable || (config.booleanOperatorsEnabled && !boolean_points)) {
        std::cerr << "probe only supports plain single/hybrid fractal fields\n";
        return 5;
    }
    if (boolean_points) {
        if (!config.booleanOperatorsEnabled) return 15;
        for (const auto &key : parameters->GetListOfParameters()) {
            if ((key.startsWith("primitive_") && key.endsWith("_enabled"))
                || key.endsWith("_use_displacement_texture")
                || key.endsWith("_perlin_noise_displacement_enable")
                || key.endsWith("_texture_fractalize")) {
                if (parameters->Get<bool>(key)) return 15;
            }
        }
    }
    std::ifstream input(argv[2]);
    std::ofstream output(argv[3]);
    if (!input || !output) return 6;
    output << std::setprecision(17);
    if (march) {
        // Explicit plain-field contracts only; do not infer support from a
        // successfully linked native worker without its scene dependencies.
        if (hybrid_march) {
            if (!parameters->Get<bool>("hybrid_fractal_enable")
                || parameters->Get<int>("formula_1") != 7
                || parameters->Get<int>("formula_2") != 1045) return 15;
            for (int i = 3; i <= NUMBER_OF_FRACTALS; ++i) {
                if (parameters->Get<int>(QString("formula_%1").arg(i)) != 0) return 15;
            }
        } else if (parameters->Get<int>("formula_1") != 11
            || parameters->Get<bool>("hybrid_fractal_enable")) return 15;
        for (const auto &key : parameters->GetListOfParameters()) {
            if ((key.startsWith("primitive_") && key.endsWith("_enabled"))
                || key.endsWith("_use_displacement_texture")
                || key.endsWith("_perlin_noise_displacement_enable")
                || key.endsWith("_texture_fractalize")) {
                if (parameters->Get<bool>(key)) return 15;
            }
        }
        const int height = parameters->Get<int>("image_height");
        if (height <= 0 || config.maxRaymarchingSteps <= 0 || config.maxRaymarchingSteps > 10000) return 15;
        config.resolution = 1.0 / height;
        auto render = std::make_shared<sParamRender>(config);
        auto field = std::make_shared<cNineFractals>(formulas, parameters);
        auto data = std::make_shared<sRenderData>();
        data->objectData.resize(1);
        data->objectData[0].materialId = 1;
        data->objectData[0].objectType = fractal::objFractal;
        data->materials.emplace(1, cMaterial());
        data->statistics.histogramIterations.Resize(config.N + 1);
        data->statistics.histogramStepCount.Resize(config.maxRaymarchingSteps + 32);
        cRenderWorker worker(render, field, nullptr, data, nullptr);
        std::vector<cRenderWorker::sStep> steps(config.maxRaymarchingSteps + 1);
        double dx, dy, dz, seed;
        size_t count = 0;
        while (input >> dx >> dy >> dz >> seed) {
            CVector3 direction(dx,dy,dz);
            if (!std::isfinite(direction.Length()) || direction.Length() < 0.99
                || direction.Length() > 1.01 || !std::isfinite(seed)
                || seed < 0 || seed > 2147483646 || seed != std::floor(seed) || count >= 4096) return 15;
            // Seed zero deliberately disables native step jitter; positive
            // seeds retain its exact implementation, independently per ray.
            for (bool refine : {false, true}) {
                gRandomSeed = unsigned(seed);
                int used = 0;
                cRenderWorker::sRayMarchingIn in;
                in.start = config.camera;
                in.direction = direction;
                in.maxScan = config.viewDistanceMax;
                in.binaryEnable = refine;
                cRenderWorker::sRayMarchingInOut io{steps.data(), &used};
                cRenderWorker::sRayMarchingOut hit;
                worker.RayMarching(in, &io, &hit);
                if (!std::isfinite(hit.depth) || hit.point.IsNotANumber()) return 8;
                if (refine) output << '\t';
                output << hit.found << '\t' << hit.point.x << '\t' << hit.point.y
                    << '\t' << hit.point.z << '\t' << hit.depth << '\t' << hit.lastDist
                    << '\t' << hit.distThresh << '\t' << used;
            }
            output << '\n';
            ++count;
        }
        if (!input.eof() || count == 0) return 9;
        std::cerr << "native original marcher samples: " << count << '\n';
        return output.good() ? 0 : 10;
    }
    if (ifs_config) {
        const auto &ifs = fractals.GetFractal(0)->IFS;
        auto vector = [&](const std::string &key, const CVector3 &v) {
            output << key << '\t' << v.x << '\t' << v.y << '\t' << v.z << '\n';
        };
        auto matrix = [&](const std::string &key, const CRotationMatrix &r) {
            auto m = r.GetMatrix();
            output << key << '\t' << m.m11 << '\t' << m.m12 << '\t' << m.m13
                << '\t' << m.m21 << '\t' << m.m22 << '\t' << m.m23
                << '\t' << m.m31 << '\t' << m.m32 << '\t' << m.m33 << '\n';
        };
        vector("camera", config.camera);
        vector("target", config.target);
        vector("top", config.topVector);
        output << "controls\t" << config.N << '\t' << config.minN << '\t'
            << config.maxRaymarchingSteps << '\t' << config.DEFactor << '\t'
            << config.detailLevel << '\t' << config.fov << '\t' << config.viewDistanceMax << '\n';
        output << "bailout\t" << fractals.GetBailout(0) << '\t' << fractals.UseAdditionalBailoutCond(0) << '\n';
        output << "flags\t" << ifs.absX << '\t' << ifs.absY << '\t' << ifs.absZ
            << '\t' << ifs.rotationEnabled << '\t' << ifs.edgeEnabled << '\t' << ifs.mengerSpongeMode << '\n';
        output << "scale\t" << ifs.scale << '\n';
        vector("offset", CVector3(ifs.offset.x, ifs.offset.y, ifs.offset.z));
        vector("edge", ifs.edge);
        matrix("mainRot", ifs.mainRot);
        for (int i = 0; i < IFS_VECTOR_COUNT; ++i) {
            std::string key = "plane" + std::to_string(i);
            output << key << '\t' << ifs.enabled[i] << '\t' << ifs.distance[i] << '\t' << ifs.intensity[i] << '\n';
            vector(key + "Direction", CVector3(ifs.direction[i].x, ifs.direction[i].y, ifs.direction[i].z));
            matrix(key + "Rot", ifs.rot[i]);
        }
        return output.good() ? 0 : 10;
    }
    cCameraTarget camera(config.camera, config.target, config.topVector);
    CVector3 angles = camera.GetRotation();
    CRotationMatrix rotation;
    rotation.RotateZ(angles.x);
    rotation.RotateX(angles.y);
    rotation.RotateY(angles.z);
    rotation.RotateZ(-config.sweetSpotHAngle);
    rotation.RotateX(config.sweetSpotVAngle);
    double x, y, z, threshold;
    size_t count = 0;
    while (input >> x >> y >> z >> threshold) {
        if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(z)
            || !std::isfinite(threshold) || (!orbit && threshold <= 0)) return 7;
        if (rays) {
            // Input XY is the native normalized image plane, not pixel indices.
            CVector3 direction = CalculateViewVector(CVector2<double>(x,y),
                config.fov, config.perspectiveType, rotation);
            output << direction.x << '\t' << direction.y << '\t' << direction.z << '\n';
            ++count;
            continue;
        }
        if (orbit) {
            if (threshold < -1 || threshold > 4096 || threshold != std::floor(threshold)) return 13;
            sFractalIn in(CVector3(x,y,z), config.minN, int(threshold), 1, 0, &config.common, -1, true);
            sFractalOut out{};
            Compute<fractal::calcModeDeltaDE1>(fractals, nullptr, in, &out);
            output << out.z.Length() << '\t' << out.iters << '\t' << out.maxiter
                << '\t' << out.z.x << '\t' << out.z.y << '\t' << out.z.z << '\n';
            ++count;
            continue;
        }
        if (analytic_field) {
            // Raw upstream orbit, before clipping/interior distance policy.
            // Input is already in fractal coordinates; no scene transform here.
            sFractalIn in(CVector3(x,y,z), config.minN, -1, 1, 0, &config.common, -1, false);
            sFractalOut out{};
            Compute<fractal::calcModeNormal>(fractals, nullptr, in, &out);
            if (!std::isfinite(out.distance) || !std::isfinite(out.z.Length())) return 8;
            output << out.distance << '\t' << out.z.Length() << '\t' << out.iters
                << '\t' << out.maxiter << '\n';
            ++count;
            continue;
        }
        sDistanceOut result{};
        if (metal_delta) {
            // Diagnostic only: ask the native evaluator to use the wider
            // production Metal delta. All orbit arithmetic remains native.
            config.advancedQuality = true;
            config.deltaDERelativeDelta = std::max(1e-7,
                5e-7 * CVector3(x, y, z).Length()) / threshold;
        }
        // Normal-calculation mode is the one used by native CalculateNormals.
        double distance = CalculateDistance(config, fractals,
            sDistanceIn(CVector3(x, y, z), threshold, !primary), &result, nullptr);
        if (!std::isfinite(distance)) return 8;
        output << distance << '\t' << result.iters << '\n';
        ++count;
    }
    if (!input.eof() || count == 0) return 9;
    std::cerr << "native " << (primary ? "primary" : "normal") << "-mode samples: " << count << '\n';
    return output.good() ? 0 : 10;
}
