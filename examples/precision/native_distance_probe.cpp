// Diagnostic adapter to an external Mandelbulber build. No formula implementation
// is copied here; linked binaries inherit the external project's GPL obligations.
#include <QCoreApplication>
#include <cmath>
#include <fstream>
#include <iomanip>
#include <iostream>
#include "src/calculate_distance.hpp"
#include "src/compute_fractal.hpp"
#include "src/fractal_container.hpp"
#include "src/fractparams.hpp"
#include "src/initparameters.hpp"
#include "src/nine_fractals.hpp"
#include "src/parameters.hpp"
#include "src/settings.hpp"
#include "src/system_data.hpp"

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    systemData.locale = QLocale::c();
    systemData.locale.setNumberOptions(QLocale::OmitGroupSeparator);
    systemData.decimalPoint = ".";
    QLocale::setDefault(systemData.locale);
    if (argc != 4 && argc != 5) {
        std::cerr << "usage: native_distance_probe scene.fract points.tsv NEW-output.tsv [metal85-delta|orbit85]\n";
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
    cSettings resolved(cSettings::formatFullText);
    resolved.CreateText(parameters, formulas);
    if (!resolved.SaveToFile(QString(argv[3]) + ".settings")) return 12;
    sParamRender config(parameters);
    cNineFractals fractals(formulas, parameters);
    std::string mode = argc == 5 ? argv[4] : "distance";
    bool metal_delta = mode == "metal85-delta", orbit = mode == "orbit85";
    if (argc == 5 && ((!metal_delta && !orbit) || parameters->Get<int>("formula_1") != 85)) return 11;
    // This adapter intentionally has no textures, primitive or object-tree data.
    if (config.objectsTreeEnable || config.booleanOperatorsEnabled || config.limitsEnabled) {
        std::cerr << "probe only supports plain single/hybrid fractal fields\n";
        return 5;
    }
    std::ifstream input(argv[2]);
    std::ofstream output(argv[3]);
    if (!input || !output) return 6;
    output << std::setprecision(17);
    double x, y, z, threshold;
    size_t count = 0;
    while (input >> x >> y >> z >> threshold) {
        if (!std::isfinite(x) || !std::isfinite(y) || !std::isfinite(z)
            || !std::isfinite(threshold) || (!orbit && threshold <= 0)) return 7;
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
            sDistanceIn(CVector3(x, y, z), threshold, true), &result, nullptr);
        if (!std::isfinite(distance)) return 8;
        output << distance << '\t' << result.iters << '\n';
        ++count;
    }
    if (!input.eof() || count == 0) return 9;
    std::cerr << "native normal-mode samples: " << count << '\n';
    return output.good() ? 0 : 10;
}
