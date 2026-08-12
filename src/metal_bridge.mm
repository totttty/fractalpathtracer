#include "metal_bridge.h"

#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <Metal/Metal.h>
#import <CoreGraphics/CoreGraphics.h>
#import <ImageIO/ImageIO.h>
#import <CoreVideo/CoreVideo.h>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdarg>
#include <cctype>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

namespace {

std::mutex &diagnostic_gpu_mutex() {
    static std::mutex mutex;
    return mutex;
}

static_assert(sizeof(FptAffineTransform) == 64u,
              "FptAffineTransform layout must match Rust and Metal");
static_assert(sizeof(FptIndexedPrimitive) == 32u,
              "FptIndexedPrimitive layout must match Rust and Metal");
static_assert(sizeof(FptRenderConfig) == 30448u,
              "FptRenderConfig layout must match Rust and Metal");
static_assert(sizeof(FptDiagnosticConfig) == 24u,
              "FptDiagnosticConfig layout must match Rust and Metal");

void set_error(char *error, size_t error_len, const char *fmt, ...) {
    if (!error || error_len == 0) return;
    va_list args;
    va_start(args, fmt);
    vsnprintf(error, error_len, fmt, args);
    va_end(args);
}

MTLCompileOptions *runtime_compile_options() {
    MTLCompileOptions *options = [MTLCompileOptions new];
    options.languageVersion = MTLLanguageVersion2_4;
    if (@available(macOS 15.0, *)) {
        options.mathMode = MTLMathModeFast;
    } else {
        options.fastMathEnabled = YES;
    }
    return options;
}

NSCache<NSString *, id<MTLLibrary>> *runtime_source_library_cache() {
    static NSCache<NSString *, id<MTLLibrary>> *cache = nil;
    static dispatch_once_t once_token;
    dispatch_once(&once_token, ^{
        cache = [NSCache new];
        cache.name = @"com.fpt-metal.runtime-source-libraries";
        cache.countLimit = 32u;
        cache.totalCostLimit = 64u * 1024u * 1024u;
    });
    return cache;
}

id<MTLLibrary> compile_runtime_source_library(
    id<MTLDevice> device,
    NSString *metal_source,
    bool *cache_hit,
    NSError **error) {
    if (cache_hit) *cache_hit = false;
    NSString *cache_key = [NSString stringWithFormat:@"%llu:%@",
        device.registryID, metal_source];
    NSCache<NSString *, id<MTLLibrary>> *cache = runtime_source_library_cache();
    id<MTLLibrary> library = [cache objectForKey:cache_key];
    if (library) {
        if (cache_hit) *cache_hit = true;
        return library;
    }
    library = [device newLibraryWithSource:metal_source
                                   options:runtime_compile_options()
                                     error:error];
    if (library) {
        [cache setObject:library
                  forKey:cache_key
                    cost:metal_source.length * sizeof(unichar)];
    }
    return library;
}

void salt_runtime_source_for_benchmark(std::string &source) {
    const char *salt = std::getenv("FPT_RUNTIME_SOURCE_CACHE_SALT");
    if (!salt || salt[0] == '\0') return;
    source += "\n// FPT_RUNTIME_SOURCE_CACHE_SALT: ";
    source += salt;
    source += "\n";
}

bool build_topology_stitched_library(
    id<MTLDevice> device,
    id<MTLLibrary> stitch_host_library,
    const char *archive_path,
    const FptRenderConfig &config,
    id<MTLLibrary> __strong *stitched_library,
    id<MTLFunction> __strong *stitched_function,
    id<MTLFunction> __strong *stitched_surface_function,
    id<MTLBinaryArchive> __strong *binary_archive,
    bool *populate_binary_archive,
    std::string &failure) {
    if (@available(macOS 12.0, *)) {
        const uint32_t instruction_count = std::min<uint32_t>(
            config.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
        if (config.sdf_id != FPT_SDF_PROGRAM || instruction_count == 0u) {
            failure = "function stitching requires a typed geometry program";
            return false;
        }

        NSMutableArray<id<MTLFunction>> *functions = [NSMutableArray array];
        NSMutableArray<NSString *> *instruction_names = [NSMutableArray array];
        NSMutableArray<MTLFunctionStitchingFunctionNode *> *nodes =
            [NSMutableArray array];
        MTLFunctionStitchingInputNode *source_input =
            [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:0u];
        MTLFunctionStitchingInputNode *config_input =
            [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:1u];

        const bool lean_distance_state =
            config.sdf_stitch_distance_only != FPT_SDF_STITCH_STATE_FULL;
        const bool split_distance_graph =
            config.sdf_stitch_split_graph != 0u;
        const uint32_t fusion_mode = config.sdf_stitch_fusion;
        if (fusion_mode != 0u && lean_distance_state) {
            failure = "stitch fusion requires the full surface state";
            return false;
        }
        NSString *init_function_name = lean_distance_state
            ? @"fpt_stitch_distance_init" : @"fpt_stitch_init";
        NSString *finish_function_name = lean_distance_state
            ? @"fpt_stitch_distance_finish" : @"fpt_stitch_finish";
        NSString *surface_function_name = lean_distance_state
            ? @"fpt_stitch_interpreted_surface"
            : @"fpt_stitch_finish_surface";
        id<MTLFunction> init_function = [stitch_host_library
            newFunctionWithName:init_function_name];
        id<MTLFunction> finish_function = [stitch_host_library
            newFunctionWithName:finish_function_name];
        id<MTLFunction> finish_surface_function = [stitch_host_library
            newFunctionWithName:surface_function_name];
        id<MTLFunction> split_barrier_function = split_distance_graph
            ? [stitch_host_library
                newFunctionWithName:@"fpt_stitch_distance_barrier"]
            : nil;
        if (!init_function || !finish_function || !finish_surface_function) {
            failure = "stitch-host init or finish function is missing";
            return false;
        }
        if (split_distance_graph && !split_barrier_function) {
            failure = "stitch-host split barrier function is missing";
            return false;
        }
        [functions addObject:init_function];
        [functions addObject:finish_function];
        [functions addObject:finish_surface_function];
        if (split_barrier_function) [functions addObject:split_barrier_function];
        MTLFunctionStitchingFunctionNode *current =
            [[MTLFunctionStitchingFunctionNode alloc]
                initWithName:init_function_name
                   arguments:@[source_input]
         controlDependencies:@[]];
        [nodes addObject:current];

        auto is_translated_sphere_union = [&](uint32_t index) {
            return index + 1u < instruction_count &&
                config.sdf_program[index].opcode == FPT_SDF_OP_TRANSLATE &&
                config.sdf_program[index + 1u].opcode == FPT_SDF_OP_SPHERE &&
                config.sdf_program[index + 1u].flags == 0u;
        };
        bool fused_one_pair = false;
        for (uint32_t index = 0u; index < instruction_count;) {
            uint32_t consumed = 1u;
            std::string function_name;
            const bool fuse_double = fusion_mode == 3u &&
                is_translated_sphere_union(index) &&
                is_translated_sphere_union(index + 2u);
            const bool fuse_pair = is_translated_sphere_union(index) &&
                (fusion_mode == 2u ||
                 (fusion_mode == 1u && !fused_one_pair));
            if (fuse_double) {
                function_name = "fpt_stitch_two_translated_sphere_unions_" +
                    std::to_string(index);
                consumed = 4u;
            } else if (fuse_pair) {
                function_name = "fpt_stitch_translated_sphere_union_" +
                    std::to_string(index);
                consumed = 2u;
                fused_one_pair = true;
            }
            const FptSdfInstruction &instruction = config.sdf_program[index];
            if (function_name.empty()) {
                function_name = lean_distance_state
                    ? "fpt_stitch_distance_" : "fpt_stitch_";
                switch (instruction.opcode) {
                    case FPT_SDF_OP_ABS:
                        function_name += "abs_";
                        break;
                    case FPT_SDF_OP_TRANSLATE:
                        function_name += "translate_";
                        break;
                    case FPT_SDF_OP_SCALE:
                        function_name += "scale_";
                        break;
                    case FPT_SDF_OP_ROTATE_X:
                        function_name += "rotate_x_";
                        break;
                    case FPT_SDF_OP_ROTATE_Y:
                        function_name += "rotate_y_";
                        break;
                    case FPT_SDF_OP_ROTATE_Z:
                        function_name += "rotate_z_";
                        break;
                    case FPT_SDF_OP_REPEAT:
                        function_name += "repeat_";
                        break;
                    case FPT_SDF_OP_SORT_DESC:
                        function_name += "sort_desc_";
                        break;
                    case FPT_SDF_OP_SPHERE:
                        function_name += "sphere_";
                        break;
                    case FPT_SDF_OP_BOX:
                        function_name += "box_";
                        break;
                    case FPT_SDF_OP_PLANE:
                        function_name += "plane_";
                        break;
                    default:
                        failure = "function stitching does not support geometry opcode " +
                            std::to_string(instruction.opcode) + " at instruction " +
                            std::to_string(index);
                        return false;
                }
                if (instruction.opcode == FPT_SDF_OP_SPHERE ||
                    instruction.opcode == FPT_SDF_OP_BOX ||
                    instruction.opcode == FPT_SDF_OP_PLANE) {
                    if (instruction.flags > 2u) {
                        failure = "function stitching does not support combine mode " +
                            std::to_string(instruction.flags) + " at instruction " +
                            std::to_string(index);
                        return false;
                    }
                    constexpr const char *combine_names[] = {
                        "union_", "intersection_", "subtract_"};
                    function_name += combine_names[instruction.flags];
                }
                function_name += std::to_string(index);
            }
            NSString *name = [NSString stringWithUTF8String:function_name.c_str()];
            id<MTLFunction> function =
                [stitch_host_library newFunctionWithName:name];
            if (!function) {
                failure = "stitch-host function is missing: " + function_name;
                return false;
            }
            [functions addObject:function];
            [instruction_names addObject:name];
            current = [[MTLFunctionStitchingFunctionNode alloc]
                initWithName:name
                   arguments:@[current, config_input]
         controlDependencies:@[]];
            [nodes addObject:current];
            index += consumed;
        }

        MTLFunctionStitchingFunctionNode *finish =
            [[MTLFunctionStitchingFunctionNode alloc]
                initWithName:finish_function_name
                   arguments:@[current]
         controlDependencies:@[]];
        NSArray<id<MTLFunctionStitchingAttribute>> *attributes =
            config.sdf_function_stitching == 2u
                ? @[[MTLFunctionStitchingAttributeAlwaysInline new]]
                : @[];
        MTLStitchedLibraryDescriptor *descriptor = nil;
        if (split_distance_graph) {
            const uint32_t split_index = instruction_count / 2u;
            if (split_index == 0u || split_index == instruction_count) {
                failure = "split stitching requires at least two instructions";
                return false;
            }

            MTLFunctionStitchingInputNode *prefix_source =
                [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:0u];
            MTLFunctionStitchingInputNode *prefix_config =
                [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:1u];
            NSMutableArray<MTLFunctionStitchingFunctionNode *> *prefix_nodes =
                [NSMutableArray array];
            MTLFunctionStitchingFunctionNode *prefix_current =
                [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:init_function_name
                       arguments:@[prefix_source]
             controlDependencies:@[]];
            [prefix_nodes addObject:prefix_current];
            for (uint32_t index = 0u; index < split_index; ++index) {
                prefix_current = [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:instruction_names[index]
                       arguments:@[prefix_current, prefix_config]
             controlDependencies:@[]];
                [prefix_nodes addObject:prefix_current];
            }
            MTLFunctionStitchingFunctionNode *prefix_barrier =
                [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:@"fpt_stitch_distance_barrier"
                       arguments:@[prefix_current]
             controlDependencies:@[]];

            MTLFunctionStitchingInputNode *suffix_state =
                [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:0u];
            MTLFunctionStitchingInputNode *suffix_config =
                [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:1u];
            NSMutableArray<MTLFunctionStitchingFunctionNode *> *suffix_nodes =
                [NSMutableArray array];
            MTLFunctionStitchingFunctionNode *suffix_current = nil;
            id<MTLFunctionStitchingNode> suffix_argument = suffix_state;
            for (uint32_t index = split_index; index < instruction_count; ++index) {
                suffix_current = [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:instruction_names[index]
                       arguments:@[suffix_argument, suffix_config]
             controlDependencies:@[]];
                [suffix_nodes addObject:suffix_current];
                suffix_argument = suffix_current;
            }

            MTLFunctionStitchingGraph *prefix_graph =
                [[MTLFunctionStitchingGraph alloc]
                    initWithFunctionName:@"fpt_stitch_distance_prefix"
                                   nodes:prefix_nodes
                              outputNode:prefix_barrier
                              attributes:@[]];
            MTLFunctionStitchingGraph *suffix_graph =
                [[MTLFunctionStitchingGraph alloc]
                    initWithFunctionName:@"fpt_stitch_distance_suffix"
                                   nodes:suffix_nodes
                              outputNode:suffix_current
                              attributes:@[]];
            MTLStitchedLibraryDescriptor *segment_descriptor =
                [MTLStitchedLibraryDescriptor new];
            segment_descriptor.functions = functions;
            segment_descriptor.functionGraphs = @[prefix_graph, suffix_graph];
            NSError *segment_error = nil;
            id<MTLLibrary> segment_library = [device
                newLibraryWithStitchedDescriptor:segment_descriptor
                                           error:&segment_error];
            if (!segment_library) {
                failure = "Metal split-segment stitching failed: " + std::string(
                    segment_error.localizedDescription.UTF8String ?: "unknown error");
                return false;
            }
            id<MTLFunction> prefix_function = [segment_library
                newFunctionWithName:@"fpt_stitch_distance_prefix"];
            id<MTLFunction> suffix_function = [segment_library
                newFunctionWithName:@"fpt_stitch_distance_suffix"];
            if (!prefix_function || !suffix_function) {
                failure = "split stitched segment function is missing";
                return false;
            }

            MTLFunctionStitchingInputNode *outer_source =
                [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:0u];
            MTLFunctionStitchingInputNode *outer_config =
                [[MTLFunctionStitchingInputNode alloc] initWithArgumentIndex:1u];
            MTLFunctionStitchingFunctionNode *prefix_call =
                [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:@"fpt_stitch_distance_prefix"
                       arguments:@[outer_source, outer_config]
             controlDependencies:@[]];
            MTLFunctionStitchingFunctionNode *suffix_call =
                [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:@"fpt_stitch_distance_suffix"
                       arguments:@[prefix_call, outer_config]
             controlDependencies:@[]];
            MTLFunctionStitchingFunctionNode *outer_finish =
                [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:finish_function_name
                       arguments:@[suffix_call]
             controlDependencies:@[]];
            MTLFunctionStitchingGraph *distance_graph =
                [[MTLFunctionStitchingGraph alloc]
                    initWithFunctionName:@"deTopologyStitchedDistance"
                                   nodes:@[prefix_call, suffix_call]
                              outputNode:outer_finish
                              attributes:attributes];
            MTLFunctionStitchingFunctionNode *finish_surface =
                [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:surface_function_name
                       arguments:@[outer_source, outer_config]
             controlDependencies:@[]];
            MTLFunctionStitchingGraph *surface_graph =
                [[MTLFunctionStitchingGraph alloc]
                    initWithFunctionName:@"deTopologyStitchedSurface"
                                   nodes:@[finish_surface]
                              outputNode:finish_surface
                              attributes:attributes];
            descriptor = [MTLStitchedLibraryDescriptor new];
            descriptor.functions = @[
                prefix_function, suffix_function, finish_function,
                finish_surface_function];
            descriptor.functionGraphs = @[distance_graph, surface_graph];
        } else {
            MTLFunctionStitchingGraph *distance_graph =
                [[MTLFunctionStitchingGraph alloc]
                    initWithFunctionName:@"deTopologyStitchedDistance"
                                   nodes:nodes
                              outputNode:finish
                              attributes:attributes];
            MTLFunctionStitchingFunctionNode *finish_surface = lean_distance_state
                ? [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:surface_function_name
                       arguments:@[source_input, config_input]
             controlDependencies:@[]]
                : [[MTLFunctionStitchingFunctionNode alloc]
                    initWithName:surface_function_name
                       arguments:@[current]
             controlDependencies:@[]];
            NSArray<MTLFunctionStitchingFunctionNode *> *surface_nodes =
                lean_distance_state ? @[finish_surface] : nodes;
            MTLFunctionStitchingGraph *surface_graph =
                [[MTLFunctionStitchingGraph alloc]
                    initWithFunctionName:@"deTopologyStitchedSurface"
                                   nodes:surface_nodes
                              outputNode:finish_surface
                              attributes:attributes];
            descriptor = [MTLStitchedLibraryDescriptor new];
            descriptor.functions = functions;
            descriptor.functionGraphs = @[distance_graph, surface_graph];
        }

        NSError *ns_error = nil;
        id<MTLBinaryArchive> archive = nil;
        bool archive_loaded = false;
        if (@available(macOS 15.0, *)) {
            if (archive_path && archive_path[0] != '\0') {
                NSURL *archive_url = [NSURL fileURLWithPath:
                    [NSString stringWithUTF8String:archive_path]];
                MTLBinaryArchiveDescriptor *archive_descriptor =
                    [MTLBinaryArchiveDescriptor new];
                if ([[NSFileManager defaultManager]
                        fileExistsAtPath:archive_url.path]) {
                    archive_descriptor.url = archive_url;
                    archive = [device
                        newBinaryArchiveWithDescriptor:archive_descriptor
                                                 error:&ns_error];
                    archive_loaded = archive != nil;
                    if (!archive) {
                        [[NSFileManager defaultManager]
                            removeItemAtURL:archive_url error:nil];
                        ns_error = nil;
                    }
                }
                if (!archive) {
                    archive_descriptor.url = nil;
                    archive = [device
                        newBinaryArchiveWithDescriptor:archive_descriptor
                                                 error:&ns_error];
                    if (!archive) {
                        failure = "failed to create Metal binary archive: " +
                            std::string(ns_error.localizedDescription.UTF8String ?:
                                        "unknown error");
                        return false;
                    }
                    if (![archive addLibraryWithDescriptor:descriptor
                                                     error:&ns_error]) {
                        failure = "failed to populate stitched-library archive: " +
                            std::string(ns_error.localizedDescription.UTF8String ?:
                                        "unknown error");
                        return false;
                    }
                }
                descriptor.binaryArchives = @[archive];
            }
        }
        *stitched_library =
            [device newLibraryWithStitchedDescriptor:descriptor error:&ns_error];
        if (!*stitched_library) {
            failure = "Metal function stitching failed: " + std::string(
                ns_error.localizedDescription.UTF8String ?: "unknown error");
            return false;
        }
        *stitched_function = [*stitched_library
            newFunctionWithName:@"deTopologyStitchedDistance"];
        *stitched_surface_function = [*stitched_library
            newFunctionWithName:@"deTopologyStitchedSurface"];
        if (!*stitched_function || !*stitched_surface_function) {
            failure = "stitched distance or surface function is missing from library";
            return false;
        }
        if (binary_archive) *binary_archive = archive;
        if (populate_binary_archive) {
            *populate_binary_archive = archive != nil && !archive_loaded;
        }
        return true;
    }
    failure = "Metal function stitching requires macOS 12 or newer";
    return false;
}

id<MTLComputePipelineState> new_compute_pipeline(
    id<MTLDevice> device,
    id<MTLFunction> function,
    NSArray<id<MTLFunction>> *private_functions,
    id<MTLBinaryArchive> binary_archive,
    bool populate_binary_archive,
    NSError **error) {
    if (private_functions.count == 0u && !binary_archive) {
        return [device newComputePipelineStateWithFunction:function error:error];
    }
    MTLComputePipelineDescriptor *descriptor =
        [MTLComputePipelineDescriptor new];
    descriptor.computeFunction = function;
    if (private_functions.count > 0u) {
        MTLLinkedFunctions *linked = [MTLLinkedFunctions linkedFunctions];
        linked.privateFunctions = private_functions;
        descriptor.linkedFunctions = linked;
    }
    if (binary_archive) descriptor.binaryArchives = @[binary_archive];
    if (populate_binary_archive &&
        ![binary_archive addComputePipelineFunctionsWithDescriptor:descriptor
                                                              error:error]) {
        return nil;
    }
    return [device newComputePipelineStateWithDescriptor:descriptor
                                                 options:MTLPipelineOptionNone
                                              reflection:nil
                                                   error:error];
}

bool prepare_compute_binary_archive(
    id<MTLDevice> device,
    const char *archive_path,
    id<MTLBinaryArchive> __strong *binary_archive,
    bool *populate_binary_archive,
    NSError **error) {
    if (binary_archive) *binary_archive = nil;
    if (populate_binary_archive) *populate_binary_archive = false;
    if (!archive_path || archive_path[0] == '\0') return true;
    if (@available(macOS 15.0, *)) {
        NSURL *archive_url = [NSURL fileURLWithPath:
            [NSString stringWithUTF8String:archive_path]];
        MTLBinaryArchiveDescriptor *descriptor = [MTLBinaryArchiveDescriptor new];
        bool loaded = false;
        id<MTLBinaryArchive> archive = nil;
        if ([[NSFileManager defaultManager] fileExistsAtPath:archive_url.path]) {
            descriptor.url = archive_url;
            archive = [device newBinaryArchiveWithDescriptor:descriptor error:error];
            loaded = archive != nil;
            if (!archive) {
                [[NSFileManager defaultManager] removeItemAtURL:archive_url error:nil];
                if (error) *error = nil;
            }
        }
        if (!archive) {
            descriptor.url = nil;
            archive = [device newBinaryArchiveWithDescriptor:descriptor error:error];
        }
        if (!archive) return false;
        if (binary_archive) *binary_archive = archive;
        if (populate_binary_archive) *populate_binary_archive = !loaded;
    }
    return true;
}

id<MTLFunction> new_topology_runtime_function(
    id<MTLLibrary> library,
    NSString *name,
    bool dual_generated_library,
    bool analytic_surface,
    NSError **error) {
    if (!dual_generated_library) return [library newFunctionWithName:name];
    bool enabled = analytic_surface;
    MTLFunctionConstantValues *constants = [MTLFunctionConstantValues new];
    [constants setConstantValue:&enabled type:MTLDataTypeBool atIndex:0u];
    return [library newFunctionWithName:name
                         constantValues:constants
                                  error:error];
}

bool generate_topology_specialized_source(
    const char *shader_source,
    size_t shader_source_len,
    const FptRenderConfig &config,
    std::string &specialized_source,
    std::string &failure) {
    if (!shader_source || shader_source_len == 0u) {
        failure = "embedded Metal source is unavailable";
        return false;
    }
    const uint32_t instruction_count = std::min<uint32_t>(
        config.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    if (config.sdf_id != FPT_SDF_PROGRAM || instruction_count == 0u) {
        failure = "topology specialization requires a typed geometry program";
        return false;
    }

    auto install_bodies = [&](const std::string &distance_body,
                              const std::string &surface_body) {
        constexpr const char *marker =
            "    // FPT_TOPOLOGY_SPECIALIZED_BODY\n"
            "    return deProgramDistance(source, cfg);";
        specialized_source.assign(shader_source, shader_source_len);
        specialized_source.insert(
            0u, "#define FPT_TOPOLOGY_RUNTIME_SOURCE 1\n");
        if ((config.sdf_topology_specialization == 3u ||
             config.sdf_topology_specialization == 4u ||
             config.sdf_topology_specialization == 5u) &&
            config.sdf_stitched_surface == 0u) {
            specialized_source.insert(
                0u, "#define FPT_TOPOLOGY_CANONICAL_RUNTIME_SURFACE 1\n");
        }
        if (config.sdf_stitched_surface != 0u) {
            specialized_source.insert(
                0u, "#define FPT_TOPOLOGY_GENERATED_SURFACE 1\n");
        }
        const size_t position = specialized_source.find(marker);
        if (position == std::string::npos) {
            failure = "topology specialization marker is missing from Metal source";
            return false;
        }
        specialized_source.replace(position, std::strlen(marker), distance_body);
        if (config.sdf_stitched_surface != 0u) {
            constexpr const char *surface_marker =
                "    // FPT_TOPOLOGY_SPECIALIZED_SURFACE_BODY\n"
                "    return programSurfaceInterpreted(source, cfg);";
            const size_t surface_position =
                specialized_source.find(surface_marker);
            if (surface_position == std::string::npos) {
                failure = "topology-specialized surface marker is missing from Metal source";
                return false;
            }
            specialized_source.replace(
                surface_position, std::strlen(surface_marker), surface_body);
        }
        return true;
    };

    const uint32_t canonical_count = std::min<uint32_t>(
        config.sdf_canonical_count, FPT_SDF_FLAT_UNION_MAX_PRIMITIVES);
    if (config.sdf_topology_specialization == 2u && canonical_count > 0u) {
        std::string body = "    float distance = inf;\n";
        std::string surface_body =
            "    ProgramSurface surface = {inf, float3(0.0f)};\n";
        for (uint32_t index = 0u; index < canonical_count; ++index) {
            const FptPrimitiveInstance &primitive =
                config.sdf_canonical_primitives[index];
            if (primitive.opcode != FPT_SDF_OP_SPHERE &&
                primitive.opcode != FPT_SDF_OP_BOX &&
                primitive.opcode != FPT_SDF_OP_PLANE) {
                failure = "canonical IR contains unsupported primitive opcode " +
                    std::to_string(primitive.opcode);
                return false;
            }
            if (index > 0u && primitive._pad0 > 2u) {
                failure = "canonical IR contains unsupported combine mode " +
                    std::to_string(primitive._pad0);
                return false;
            }
            const std::string i = std::to_string(index);
            const std::string instance =
                "cfg.sdf_canonical_primitives[" + i + "]";
            body += "    float candidate" + i +
                " = flatUnionPrimitiveDistance(source, " + instance + ");\n";
            surface_body += "    ProgramSurface candidate" + i +
                " = canonicalPrimitiveSurface(source, " + instance + ");\n";
            if (index == 0u) {
                body += "    distance = candidate" + i + ";\n";
                surface_body += "    surface = candidate" + i + ";\n";
            } else if (primitive._pad0 == 1u) {
                body += "    distance = max(distance, candidate" + i + ");\n";
                surface_body += "    if (candidate" + i +
                    ".distance > surface.distance) surface = candidate" + i + ";\n";
            } else if (primitive._pad0 == 2u) {
                body += "    distance = max(distance, -candidate" + i + ");\n";
                surface_body += "    if (-candidate" + i +
                    ".distance > surface.distance) { surface.distance = -candidate" + i +
                    ".distance; surface.gradient = -candidate" + i + ".gradient; }\n";
            } else {
                body += "    distance = min(distance, candidate" + i + ");\n";
                surface_body += "    if (candidate" + i +
                    ".distance < surface.distance) surface = candidate" + i + ";\n";
            }
        }
        body += "    return distance;";
        surface_body += "    return surface;";
        return install_bodies(body, surface_body);
    }

    if (config.sdf_topology_specialization == 3u && canonical_count > 0u) {
        std::string body = "    float distance = inf;\n";
        std::string surface_body =
            "    ProgramSurface surface = {inf, float3(0.0f)};\n";
        for (uint32_t index = 0u; index < canonical_count; ++index) {
            const FptPrimitiveInstance &primitive =
                config.sdf_canonical_primitives[index];
            if (primitive.opcode != FPT_SDF_OP_SPHERE &&
                primitive.opcode != FPT_SDF_OP_BOX &&
                primitive.opcode != FPT_SDF_OP_PLANE) {
                failure = "compact canonical IR contains unsupported primitive opcode " +
                    std::to_string(primitive.opcode);
                return false;
            }
            if (index > 0u && primitive._pad0 > 2u) {
                failure = "compact canonical IR contains unsupported combine mode " +
                    std::to_string(primitive._pad0);
                return false;
            }
            const std::string i = std::to_string(index);
            const std::string instance =
                "cfg.sdf_canonical_primitives[" + i + "]";
            auto transform_value = [&](uint32_t component) {
                return instance + ".transform[" +
                    std::to_string(component) + "]";
            };
            auto data_value = [&](uint32_t component) {
                return instance + ".data[" + std::to_string(component) + "]";
            };
            const std::string row0 = "float3(" + transform_value(0u) + ", " +
                transform_value(1u) + ", " + transform_value(2u) + ")";
            const std::string row1 = "float3(" + transform_value(4u) + ", " +
                transform_value(5u) + ", " + transform_value(6u) + ")";
            const std::string row2 = "float3(" + transform_value(8u) + ", " +
                transform_value(9u) + ", " + transform_value(10u) + ")";
            const std::string point = "float3(dot(" + row0 +
                ", source) + " + transform_value(3u) + ", dot(" + row1 +
                ", source) + " + transform_value(7u) + ", dot(" + row2 +
                ", source) + " + transform_value(11u) + ")";
            const std::string divisor =
                "max(" + instance + ".distance_scale, 1.0e-6f)";

            body += "    {\n";
            body += "      float3 p = " + point + ";\n";
            body += "      float divisor = " + divisor + ";\n";
            if (primitive.opcode == FPT_SDF_OP_SPHERE) {
                body += "      float candidate = (length(p) - " +
                    data_value(0u) + ") / divisor;\n";
            } else if (primitive.opcode == FPT_SDF_OP_BOX) {
                body += "      float3 q = abs(p) - abs(float3(" +
                    data_value(0u) + ", " + data_value(1u) + ", " +
                    data_value(2u) + "));\n";
                body += "      float candidate = (min(max(q.x, max(q.y, q.z)), "
                        "0.0f) + length(max(q, float3(0.0f)))) / divisor;\n";
            } else {
                body += "      float candidate = (dot(p, float3(" +
                    data_value(0u) + ", " + data_value(1u) + ", " +
                    data_value(2u) + ")) + " + data_value(3u) +
                    ") / divisor;\n";
            }
            if (index == 0u) {
                body += "      distance = candidate;\n";
            } else if (primitive._pad0 == 1u) {
                body += "      distance = max(distance, candidate);\n";
            } else if (primitive._pad0 == 2u) {
                body += "      distance = max(distance, -candidate);\n";
            } else {
                body += "      distance = min(distance, candidate);\n";
            }
            body += "    }\n";

            surface_body += "    {\n";
            surface_body += "      float3 row0 = " + row0 + ";\n";
            surface_body += "      float3 row1 = " + row1 + ";\n";
            surface_body += "      float3 row2 = " + row2 + ";\n";
            surface_body += "      float3 p = float3(dot(row0, source) + " +
                transform_value(3u) + ", dot(row1, source) + " +
                transform_value(7u) + ", dot(row2, source) + " +
                transform_value(11u) + ");\n";
            surface_body += "      float divisor = " + divisor + ";\n";
            if (primitive.opcode == FPT_SDF_OP_SPHERE) {
                surface_body += "      float radius = length(p);\n";
                surface_body += "      float candidate = (radius - " +
                    data_value(0u) + ") / divisor;\n";
                surface_body += "      float3 local_gradient = radius > 1.0e-8f "
                    "? p / radius : float3(0.0f, 1.0f, 0.0f);\n";
            } else if (primitive.opcode == FPT_SDF_OP_BOX) {
                surface_body += "      float3 q = abs(p) - abs(float3(" +
                    data_value(0u) + ", " + data_value(1u) + ", " +
                    data_value(2u) + "));\n";
                surface_body += "      float3 outside = max(q, float3(0.0f));\n";
                surface_body += "      float outside_length = length(outside);\n";
                surface_body += "      float candidate = (min(max(q.x, max(q.y, "
                    "q.z)), 0.0f) + outside_length) / divisor;\n";
                surface_body += "      float3 local_gradient;\n";
                surface_body += "      if (outside_length > 1.0e-8f) "
                    "local_gradient = sign(p) * outside / outside_length;\n";
                surface_body += "      else if (q.x >= q.y && q.x >= q.z) "
                    "local_gradient = float3(sign(p.x), 0.0f, 0.0f);\n";
                surface_body += "      else if (q.y >= q.z) local_gradient = "
                    "float3(0.0f, sign(p.y), 0.0f);\n";
                surface_body += "      else local_gradient = "
                    "float3(0.0f, 0.0f, sign(p.z));\n";
            } else {
                surface_body += "      float3 local_gradient = float3(" +
                    data_value(0u) + ", " + data_value(1u) + ", " +
                    data_value(2u) + ");\n";
                surface_body += "      float candidate = (dot(p, local_gradient) + " +
                    data_value(3u) + ") / divisor;\n";
            }
            surface_body += "      float3 candidate_gradient = "
                "(local_gradient.x * row0 + local_gradient.y * row1 + "
                "local_gradient.z * row2) / divisor;\n";
            if (index == 0u) {
                surface_body += "      surface.distance = candidate; "
                    "surface.gradient = candidate_gradient;\n";
            } else if (primitive._pad0 == 1u) {
                surface_body += "      if (candidate > surface.distance) { "
                    "surface.distance = candidate; surface.gradient = "
                    "candidate_gradient; }\n";
            } else if (primitive._pad0 == 2u) {
                surface_body += "      if (-candidate > surface.distance) { "
                    "surface.distance = -candidate; surface.gradient = "
                    "-candidate_gradient; }\n";
            } else {
                surface_body += "      if (candidate < surface.distance) { "
                    "surface.distance = candidate; surface.gradient = "
                    "candidate_gradient; }\n";
            }
            surface_body += "    }\n";
        }
        body += "    return distance;";
        surface_body += "    return surface;";
        return install_bodies(body, surface_body);
    }

    if (config.sdf_topology_specialization == 4u && canonical_count > 0u) {
        std::vector<uint32_t> transform_representatives;
        std::vector<uint32_t> transform_ids(canonical_count, 0u);
        auto same_transform = [](const FptPrimitiveInstance &left,
                                 const FptPrimitiveInstance &right) {
            if (std::memcmp(left.transform, right.transform,
                            sizeof(left.transform)) != 0) {
                return false;
            }
            return std::memcmp(&left.distance_scale, &right.distance_scale,
                               sizeof(left.distance_scale)) == 0;
        };
        for (uint32_t index = 0u; index < canonical_count; ++index) {
            const FptPrimitiveInstance &primitive =
                config.sdf_canonical_primitives[index];
            if (primitive.opcode != FPT_SDF_OP_SPHERE &&
                primitive.opcode != FPT_SDF_OP_BOX &&
                primitive.opcode != FPT_SDF_OP_PLANE) {
                failure = "shared-transform DAG contains unsupported primitive opcode " +
                    std::to_string(primitive.opcode);
                return false;
            }
            if (index > 0u && primitive._pad0 > 2u) {
                failure = "shared-transform DAG contains unsupported combine mode " +
                    std::to_string(primitive._pad0);
                return false;
            }
            uint32_t transform_id = static_cast<uint32_t>(transform_representatives.size());
            for (uint32_t candidate = 0u;
                 candidate < transform_representatives.size(); ++candidate) {
                if (same_transform(
                        primitive,
                        config.sdf_canonical_primitives[
                            transform_representatives[candidate]])) {
                    transform_id = candidate;
                    break;
                }
            }
            if (transform_id == transform_representatives.size()) {
                transform_representatives.push_back(index);
            }
            transform_ids[index] = transform_id;
        }

        std::string body = "    float distance = inf;\n";
        for (uint32_t transform_id = 0u;
             transform_id < transform_representatives.size(); ++transform_id) {
            const std::string t = std::to_string(transform_id);
            const std::string representative = std::to_string(
                transform_representatives[transform_id]);
            const std::string instance =
                "cfg.sdf_canonical_primitives[" + representative + "]";
            auto transform_value = [&](uint32_t component) {
                return instance + ".transform[" +
                    std::to_string(component) + "]";
            };
            const std::string row0 = "float3(" + transform_value(0u) + ", " +
                transform_value(1u) + ", " + transform_value(2u) + ")";
            const std::string row1 = "float3(" + transform_value(4u) + ", " +
                transform_value(5u) + ", " + transform_value(6u) + ")";
            const std::string row2 = "float3(" + transform_value(8u) + ", " +
                transform_value(9u) + ", " + transform_value(10u) + ")";
            body += "    float3 sharedPoint" + t + " = float3(dot(" + row0 +
                ", source) + " + transform_value(3u) + ", dot(" + row1 +
                ", source) + " + transform_value(7u) + ", dot(" + row2 +
                ", source) + " + transform_value(11u) + ");\n";
            body += "    float sharedDivisor" + t + " = max(" + instance +
                ".distance_scale, 1.0e-6f);\n";
        }
        for (uint32_t index = 0u; index < canonical_count; ++index) {
            const FptPrimitiveInstance &primitive =
                config.sdf_canonical_primitives[index];
            const std::string i = std::to_string(index);
            const std::string t = std::to_string(transform_ids[index]);
            const std::string instance =
                "cfg.sdf_canonical_primitives[" + i + "]";
            auto data_value = [&](uint32_t component) {
                return instance + ".data[" + std::to_string(component) + "]";
            };
            body += "    {\n";
            if (primitive.opcode == FPT_SDF_OP_SPHERE) {
                body += "      float candidate = (length(sharedPoint" + t +
                    ") - " + data_value(0u) + ") / sharedDivisor" + t + ";\n";
            } else if (primitive.opcode == FPT_SDF_OP_BOX) {
                body += "      float3 q = abs(sharedPoint" + t +
                    ") - abs(float3(" + data_value(0u) + ", " +
                    data_value(1u) + ", " + data_value(2u) + "));\n";
                body += "      float candidate = (min(max(q.x, max(q.y, q.z)), "
                        "0.0f) + length(max(q, float3(0.0f)))) / sharedDivisor" +
                    t + ";\n";
            } else {
                body += "      float candidate = (dot(sharedPoint" + t +
                    ", float3(" + data_value(0u) + ", " + data_value(1u) +
                    ", " + data_value(2u) + ")) + " + data_value(3u) +
                    ") / sharedDivisor" + t + ";\n";
            }
            if (index == 0u) {
                body += "      distance = candidate;\n";
            } else if (primitive._pad0 == 1u) {
                body += "      distance = max(distance, candidate);\n";
            } else if (primitive._pad0 == 2u) {
                body += "      distance = max(distance, -candidate);\n";
            } else {
                body += "      distance = min(distance, candidate);\n";
            }
            body += "    }\n";
        }
        body += "    return distance;";
        return install_bodies(body, std::string());
    }

    if (config.sdf_topology_specialization == 5u && canonical_count > 0u) {
        const uint32_t transform_count = std::min<uint32_t>(
            config.sdf_canonical_transform_count,
            FPT_SDF_FLAT_UNION_MAX_PRIMITIVES);
        if (transform_count == 0u) {
            failure = "affine-index lowering contains no transform records";
            return false;
        }
        std::string body = "    float distance = inf;\n";
        uint32_t index = 0u;
        while (index < canonical_count) {
            const FptIndexedPrimitive &first =
                config.sdf_indexed_primitives[index];
            if (first.transform_index >= transform_count) {
                failure = "affine-index lowering contains an invalid transform index " +
                    std::to_string(first.transform_index);
                return false;
            }
            uint32_t run_end = index + 1u;
            while (run_end < canonical_count &&
                   config.sdf_indexed_primitives[run_end].transform_index ==
                       first.transform_index) {
                ++run_end;
            }

            const std::string transform_id =
                std::to_string(first.transform_index);
            const std::string transform =
                "cfg.sdf_canonical_transforms[" + transform_id + "]";
            auto transform_value = [&](uint32_t component) {
                return transform + ".transform[" +
                    std::to_string(component) + "]";
            };
            const std::string row0 = "float3(" + transform_value(0u) + ", " +
                transform_value(1u) + ", " + transform_value(2u) + ")";
            const std::string row1 = "float3(" + transform_value(4u) + ", " +
                transform_value(5u) + ", " + transform_value(6u) + ")";
            const std::string row2 = "float3(" + transform_value(8u) + ", " +
                transform_value(9u) + ", " + transform_value(10u) + ")";
            body += "    {\n";
            body += "      float3 p = float3(dot(" + row0 +
                ", source) + " + transform_value(3u) + ", dot(" + row1 +
                ", source) + " + transform_value(7u) + ", dot(" + row2 +
                ", source) + " + transform_value(11u) + ");\n";
            body += "      float divisor = max(" + transform +
                ".distance_scale, 1.0e-6f);\n";

            for (uint32_t leaf_index = index; leaf_index < run_end;
                 ++leaf_index) {
                const FptIndexedPrimitive &primitive =
                    config.sdf_indexed_primitives[leaf_index];
                if (primitive.opcode != FPT_SDF_OP_SPHERE &&
                    primitive.opcode != FPT_SDF_OP_BOX &&
                    primitive.opcode != FPT_SDF_OP_PLANE) {
                    failure = "affine-index lowering contains unsupported primitive opcode " +
                        std::to_string(primitive.opcode);
                    return false;
                }
                if (leaf_index > 0u && primitive.combine_mode > 2u) {
                    failure = "affine-index lowering contains unsupported combine mode " +
                        std::to_string(primitive.combine_mode);
                    return false;
                }
                const std::string leaf = std::to_string(leaf_index);
                const std::string instance =
                    "cfg.sdf_indexed_primitives[" + leaf + "]";
                auto data_value = [&](uint32_t component) {
                    return instance + ".data[" +
                        std::to_string(component) + "]";
                };
                if (primitive.opcode == FPT_SDF_OP_SPHERE) {
                    body += "      float candidate" + leaf +
                        " = (length(p) - " + data_value(0u) +
                        ") / divisor;\n";
                } else if (primitive.opcode == FPT_SDF_OP_BOX) {
                    body += "      float3 q" + leaf + " = abs(p) - abs(float3(" +
                        data_value(0u) + ", " + data_value(1u) + ", " +
                        data_value(2u) + "));\n";
                    body += "      float candidate" + leaf +
                        " = (min(max(q" + leaf + ".x, max(q" + leaf +
                        ".y, q" + leaf + ".z)), 0.0f) + length(max(q" + leaf +
                        ", float3(0.0f)))) / divisor;\n";
                } else {
                    body += "      float candidate" + leaf +
                        " = (dot(p, float3(" + data_value(0u) + ", " +
                        data_value(1u) + ", " + data_value(2u) + ")) + " +
                        data_value(3u) + ") / divisor;\n";
                }
                const std::string candidate = "candidate" + leaf;
                if (leaf_index == 0u) {
                    body += "      distance = " + candidate + ";\n";
                } else if (primitive.combine_mode == 1u) {
                    body += "      distance = max(distance, " + candidate + ");\n";
                } else if (primitive.combine_mode == 2u) {
                    body += "      distance = max(distance, -" + candidate + ");\n";
                } else {
                    body += "      distance = min(distance, " + candidate + ");\n";
                }
            }
            body += "    }\n";
            index = run_end;
        }
        body += "    return distance;";
        return install_bodies(body, std::string());
    }

    std::string body;
    body += "    float3 p = source;\n";
    body += "    float distance_scale = 1.0f;\n";
    body += "    float distance = inf;\n";
    body += "    float4 data;\n";
    body += "    float candidate;\n";
    bool has_primitive = false;
    for (uint32_t index = 0u; index < instruction_count; ++index) {
        const FptSdfInstruction &instruction = config.sdf_program[index];
        const std::string i = std::to_string(index);
        body += "    data = float4(cfg.sdf_program[" + i + "].data[0], "
                "cfg.sdf_program[" + i + "].data[1], cfg.sdf_program[" + i +
                "].data[2], cfg.sdf_program[" + i + "].data[3]);\n";
        switch (instruction.opcode) {
            case FPT_SDF_OP_ABS:
                body += "    p = abs(p);\n";
                break;
            case FPT_SDF_OP_TRANSLATE:
                body += "    p -= data.xyz;\n";
                break;
            case FPT_SDF_OP_SCALE:
                body += "    { float scale = abs(data.x) > 1.0e-6f ? "
                        "data.x : 1.0f; p *= scale; distance_scale *= "
                        "abs(scale); }\n";
                break;
            case FPT_SDF_OP_ROTATE_X:
                body += "    p.yz = rot2(p.yz, data.x);\n";
                break;
            case FPT_SDF_OP_ROTATE_Y:
                body += "    p.xz = rot2(p.xz, data.x);\n";
                break;
            case FPT_SDF_OP_ROTATE_Z:
                body += "    p.xy = rot2(p.xy, data.x);\n";
                break;
            case FPT_SDF_OP_REPEAT:
                body += "    { float3 period = max(abs(data.xyz), "
                        "float3(1.0e-5f)); p -= period * floor(p / period + "
                        "0.5f); }\n";
                break;
            case FPT_SDF_OP_SORT_DESC:
                body += "    if (p.x < p.z) p.xz = p.zx;\n";
                body += "    if (p.y < p.z) p.yz = p.zy;\n";
                body += "    if (p.x < p.y) p.xy = p.yx;\n";
                break;
            case FPT_SDF_OP_SPHERE:
                body += "    candidate = (length(p) - data.x) / "
                        "max(distance_scale, 1.0e-6f);\n";
                break;
            case FPT_SDF_OP_BOX:
                body += "    { float3 q = abs(p) - abs(data.xyz); "
                        "candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) + "
                        "length(max(q, float3(0.0f)))) / "
                        "max(distance_scale, 1.0e-6f); }\n";
                break;
            case FPT_SDF_OP_PLANE:
                body += "    candidate = (dot(p, normalize(data.xyz)) + "
                        "data.w) / max(distance_scale, 1.0e-6f);\n";
                break;
            default:
                failure = "unsupported geometry opcode " +
                    std::to_string(instruction.opcode) + " at instruction " + i;
                return false;
        }
        if (instruction.opcode == FPT_SDF_OP_SPHERE ||
            instruction.opcode == FPT_SDF_OP_BOX ||
            instruction.opcode == FPT_SDF_OP_PLANE) {
            if (!has_primitive) {
                body += "    distance = candidate;\n";
                has_primitive = true;
            } else if (instruction.flags == 1u) {
                body += "    distance = max(distance, candidate);\n";
            } else if (instruction.flags == 2u) {
                body += "    distance = max(distance, -candidate);\n";
            } else if (instruction.flags == 0u) {
                body += "    distance = min(distance, candidate);\n";
            } else {
                failure = "unsupported combine mode " +
                    std::to_string(instruction.flags) + " at instruction " + i;
                return false;
            }
        }
    }
    if (!has_primitive) {
        failure = "topology specialization found no geometry primitive";
        return false;
    }
    body += "    return distance;";

    std::string surface_body;
    surface_body += "    float3 p = source;\n";
    surface_body += "    float3 jx = float3(1.0f, 0.0f, 0.0f);\n";
    surface_body += "    float3 jy = float3(0.0f, 1.0f, 0.0f);\n";
    surface_body += "    float3 jz = float3(0.0f, 0.0f, 1.0f);\n";
    surface_body += "    float distance_scale = 1.0f;\n";
    surface_body += "    ProgramSurface surface = {inf, float3(0.0f)};\n";
    surface_body += "    float4 data;\n";
    surface_body += "    float candidate;\n";
    surface_body += "    float3 local_gradient;\n";
    surface_body += "    float3 candidate_gradient;\n";
    bool surface_has_primitive = false;
    for (uint32_t index = 0u; index < instruction_count; ++index) {
        const FptSdfInstruction &instruction = config.sdf_program[index];
        const std::string i = std::to_string(index);
        surface_body += "    data = float4(cfg.sdf_program[" + i + "].data[0], "
            "cfg.sdf_program[" + i + "].data[1], cfg.sdf_program[" + i +
            "].data[2], cfg.sdf_program[" + i + "].data[3]);\n";
        switch (instruction.opcode) {
            case FPT_SDF_OP_ABS:
                surface_body += "    jx *= sign(p.x); jy *= sign(p.y); "
                    "jz *= sign(p.z); p = abs(p);\n";
                break;
            case FPT_SDF_OP_TRANSLATE:
                surface_body += "    p -= data.xyz;\n";
                break;
            case FPT_SDF_OP_SCALE:
                surface_body += "    { float s = abs(data.x) > 1.0e-6f ? "
                    "data.x : 1.0f; p *= s; jx *= s; jy *= s; jz *= s; "
                    "distance_scale *= abs(s); }\n";
                break;
            case FPT_SDF_OP_ROTATE_X:
                surface_body += "    { float s = sin(data.x), c = cos(data.x); "
                    "float py = p.y; float3 old_jy = jy; "
                    "p.y = c * py - s * p.z; p.z = s * py + c * p.z; "
                    "jy = c * old_jy - s * jz; jz = s * old_jy + c * jz; }\n";
                break;
            case FPT_SDF_OP_ROTATE_Y:
                surface_body += "    { float s = sin(data.x), c = cos(data.x); "
                    "float px = p.x; float3 old_jx = jx; "
                    "p.x = c * px - s * p.z; p.z = s * px + c * p.z; "
                    "jx = c * old_jx - s * jz; jz = s * old_jx + c * jz; }\n";
                break;
            case FPT_SDF_OP_ROTATE_Z:
                surface_body += "    { float s = sin(data.x), c = cos(data.x); "
                    "float px = p.x; float3 old_jx = jx; "
                    "p.x = c * px - s * p.y; p.y = s * px + c * p.y; "
                    "jx = c * old_jx - s * jy; jy = s * old_jx + c * jy; }\n";
                break;
            case FPT_SDF_OP_REPEAT:
                surface_body += "    { float3 period = max(abs(data.xyz), "
                    "float3(1.0e-5f)); p -= period * "
                    "floor(p / period + 0.5f); }\n";
                break;
            case FPT_SDF_OP_SORT_DESC:
                surface_body += "    if (p.x < p.z) { p.xz = p.zx; "
                    "float3 swap = jx; jx = jz; jz = swap; }\n";
                surface_body += "    if (p.y < p.z) { p.yz = p.zy; "
                    "float3 swap = jy; jy = jz; jz = swap; }\n";
                surface_body += "    if (p.x < p.y) { p.xy = p.yx; "
                    "float3 swap = jx; jx = jy; jy = swap; }\n";
                break;
            case FPT_SDF_OP_SPHERE:
                surface_body += "    { float scale = max(distance_scale, "
                    "1.0e-6f); float radius = length(p); "
                    "candidate = (radius - data.x) / scale; "
                    "local_gradient = radius > 1.0e-8f ? p / radius : "
                    "float3(0.0f, 1.0f, 0.0f); "
                    "candidate_gradient = (local_gradient.x * jx + "
                    "local_gradient.y * jy + local_gradient.z * jz) / scale;\n";
                break;
            case FPT_SDF_OP_BOX:
                surface_body += "    { float scale = max(distance_scale, "
                    "1.0e-6f); float3 q = abs(p) - abs(data.xyz); "
                    "float3 outside = max(q, float3(0.0f)); "
                    "float outside_length = length(outside); "
                    "candidate = (min(max(q.x, max(q.y, q.z)), 0.0f) + "
                    "outside_length) / scale; "
                    "if (outside_length > 1.0e-8f) local_gradient = "
                    "sign(p) * outside / outside_length; "
                    "else if (q.x >= q.y && q.x >= q.z) local_gradient = "
                    "float3(sign(p.x), 0.0f, 0.0f); "
                    "else if (q.y >= q.z) local_gradient = "
                    "float3(0.0f, sign(p.y), 0.0f); "
                    "else local_gradient = float3(0.0f, 0.0f, sign(p.z)); "
                    "candidate_gradient = (local_gradient.x * jx + "
                    "local_gradient.y * jy + local_gradient.z * jz) / scale;\n";
                break;
            case FPT_SDF_OP_PLANE:
                surface_body += "    { float scale = max(distance_scale, "
                    "1.0e-6f); local_gradient = normalize(data.xyz); "
                    "candidate = (dot(p, local_gradient) + data.w) / scale; "
                    "candidate_gradient = (local_gradient.x * jx + "
                    "local_gradient.y * jy + local_gradient.z * jz) / scale;\n";
                break;
            default:
                failure = "unsupported surface geometry opcode " +
                    std::to_string(instruction.opcode) + " at instruction " + i;
                return false;
        }
        const bool primitive = instruction.opcode == FPT_SDF_OP_SPHERE ||
            instruction.opcode == FPT_SDF_OP_BOX ||
            instruction.opcode == FPT_SDF_OP_PLANE;
        if (!primitive) continue;
        if (!surface_has_primitive) {
            surface_body += "      surface.distance = candidate; "
                "surface.gradient = candidate_gradient; }\n";
            surface_has_primitive = true;
        } else if (instruction.flags == 0u) {
            surface_body += "      if (candidate < surface.distance) { "
                "surface.distance = candidate; "
                "surface.gradient = candidate_gradient; } }\n";
        } else if (instruction.flags == 1u) {
            surface_body += "      if (candidate > surface.distance) { "
                "surface.distance = candidate; "
                "surface.gradient = candidate_gradient; } }\n";
        } else if (instruction.flags == 2u) {
            surface_body += "      if (-candidate > surface.distance) { "
                "surface.distance = -candidate; "
                "surface.gradient = -candidate_gradient; } }\n";
        } else {
            failure = "unsupported surface combine mode " +
                std::to_string(instruction.flags) + " at instruction " + i;
            return false;
        }
    }
    surface_body += "    return surface;";

    constexpr const char *marker =
        "    // FPT_TOPOLOGY_SPECIALIZED_BODY\n"
        "    return deProgramDistance(source, cfg);";
    specialized_source.assign(shader_source, shader_source_len);
    specialized_source.insert(
        0u, "#define FPT_TOPOLOGY_RUNTIME_SOURCE 1\n");
    if (config.sdf_stitched_surface != 0u) {
        specialized_source.insert(
            0u, "#define FPT_TOPOLOGY_GENERATED_SURFACE 1\n");
    }
    const size_t position = specialized_source.find(marker);
    if (position == std::string::npos) {
        failure = "topology specialization marker is missing from Metal source";
        return false;
    }
    specialized_source.replace(position, std::strlen(marker), body);
    if (config.sdf_stitched_surface != 0u) {
        constexpr const char *surface_marker =
            "    // FPT_TOPOLOGY_SPECIALIZED_SURFACE_BODY\n"
            "    return programSurfaceInterpreted(source, cfg);";
        const size_t surface_position = specialized_source.find(surface_marker);
        if (surface_position == std::string::npos) {
            failure = "topology-specialized surface marker is missing from Metal source";
            return false;
        }
        specialized_source.replace(
            surface_position, std::strlen(surface_marker), surface_body);
    }
    return true;
}

bool generate_dual_topology_specialized_source(
    const char *shader_source,
    size_t shader_source_len,
    const FptRenderConfig &source_config,
    std::string &specialized_source,
    std::string &failure) {
    FptRenderConfig generated = source_config;
    generated.sdf_stitched_surface = 1u;
    if (!generate_topology_specialized_source(
            shader_source, shader_source_len, generated,
            specialized_source, failure)) {
        return false;
    }
    const std::string single = "#define FPT_TOPOLOGY_GENERATED_SURFACE 1\n";
    const size_t macro = specialized_source.find(single);
    if (macro == std::string::npos) {
        failure = "dual topology source is missing the generated-surface macro";
        return false;
    }
    specialized_source.replace(
        macro, single.size(), "#define FPT_TOPOLOGY_DUAL_SURFACE 1\n");
    return true;
}

bool extract_metal_braced_block(
    const std::string &source,
    size_t declaration,
    std::string &block,
    std::string &failure) {
    const size_t open = source.find('{', declaration);
    if (open == std::string::npos) {
        failure = "generated Metal declaration has no body";
        return false;
    }
    uint32_t depth = 0u;
    for (size_t index = open; index < source.size(); ++index) {
        if (source[index] == '{') {
            ++depth;
        } else if (source[index] == '}' && --depth == 0u) {
            block = source.substr(open, index - open + 1u);
            return true;
        }
    }
    failure = "generated Metal declaration has an unterminated body";
    return false;
}

bool generate_tiny_linked_topology_source(
    const char *shader_source,
    size_t shader_source_len,
    const FptRenderConfig &source_config,
    std::string &tiny_source,
    std::string &failure) {
    // Reuse the production code generator, then retain only its two evaluator
    // bodies plus the ABI types needed by the precompiled stitch host. This
    // deliberately excludes all kernels, integrators, materials, and voxel
    // code from the runtime compilation unit.
    FptRenderConfig generated = source_config;
    const bool analytic_surface = generated.sdf_stitched_surface != 0u;
    std::string full_source;
    if (!generate_topology_specialized_source(
            shader_source, shader_source_len, generated,
            full_source, failure)) {
        return false;
    }

    const std::string source_text(shader_source, shader_source_len);
    const size_t config_declaration = source_text.find("struct FptRenderConfig");
    std::string config_block;
    if (config_declaration == std::string::npos ||
        !extract_metal_braced_block(
            source_text, config_declaration, config_block, failure)) {
        if (failure.empty()) failure = "FptRenderConfig is missing from Metal source";
        return false;
    }
    const size_t config_open = source_text.find('{', config_declaration);
    const size_t config_end = source_text.find(';',
        config_open + config_block.size());
    if (config_end == std::string::npos) {
        failure = "FptRenderConfig declaration is unterminated";
        return false;
    }

    const size_t distance_declaration = full_source.find(
        "static float deTopologySpecializedDistance(");
    std::string distance_body;
    if (distance_declaration == std::string::npos ||
        !extract_metal_braced_block(
            full_source, distance_declaration, distance_body, failure)) {
        if (failure.empty()) failure = "generated distance helper is missing";
        return false;
    }

    std::string surface_body;
    if (analytic_surface) {
        const size_t surface_declaration = full_source.find(
            "ProgramSurface deTopologySpecializedSurface(");
        if (surface_declaration == std::string::npos ||
            !extract_metal_braced_block(
                full_source, surface_declaration, surface_body, failure)) {
            if (failure.empty()) failure = "generated surface helper is missing";
            return false;
        }
    }

    tiny_source.assign(source_text, 0u, config_end + 1u);
    tiny_source += "\nstruct ProgramSurface { float distance; float3 gradient; };\n";
    tiny_source += "[[visible]] float deTopologyStitchedDistance("
        "float3 source, constant FptRenderConfig &cfg) ";
    tiny_source += distance_body;
    tiny_source += "\n[[visible]] ProgramSurface deTopologyStitchedSurface("
        "float3 source, constant FptRenderConfig &cfg) ";
    if (analytic_surface) {
        tiny_source += surface_body;
    } else {
        tiny_source += "{ ProgramSurface surface = {"
            "deTopologyStitchedDistance(source, cfg), float3(0.0f)}; "
            "return surface; }";
    }
    tiny_source += "\n";
    return true;
}

NSString *ns_string(const char *path) {
    return [NSString stringWithUTF8String:path ? path : ""];
}

double command_buffer_gpu_ms(id<MTLCommandBuffer> command_buffer) {
    if (command_buffer.GPUEndTime > command_buffer.GPUStartTime) {
        return (command_buffer.GPUEndTime - command_buffer.GPUStartTime) * 1000.0;
    }
    return 0.0;
}

MTLSize threadgroup_for_pipeline(id<MTLComputePipelineState> pipeline) {
    const NSUInteger execution_width = std::max<NSUInteger>(pipeline.threadExecutionWidth, 1u);
    const NSUInteger maximum = std::max<NSUInteger>(pipeline.maxTotalThreadsPerThreadgroup, execution_width);
    const NSUInteger target = std::min<NSUInteger>(maximum, 128u);
    const NSUInteger width = std::min<NSUInteger>(execution_width, target);
    const NSUInteger height = std::max<NSUInteger>(1u, target / width);
    return MTLSizeMake(width, height, 1u);
}

MTLSize groups_for_extent(uint32_t width, uint32_t height, MTLSize threads) {
    return MTLSizeMake((width + threads.width - 1u) / threads.width,
                       (height + threads.height - 1u) / threads.height,
                       1u);
}

bool render_preview_buffer(id<MTLCommandQueue> queue,
                           id<MTLComputePipelineState> pipeline,
                           id<MTLBuffer> output,
                           id<MTLBuffer> config,
                           uint32_t width,
                           uint32_t height,
                           double *elapsed_ms,
                           std::string &failure) {
    NSDate *start = [NSDate date];
    id<MTLCommandBuffer> command = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    if (!command || !encoder) {
        failure = "failed to create preview validation command encoder";
        return false;
    }
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:output offset:0 atIndex:0];
    [encoder setBuffer:config offset:0 atIndex:1];
    MTLSize threads = threadgroup_for_pipeline(pipeline);
    MTLSize groups = groups_for_extent(width, height, threads);
    [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads];
    [encoder endEncoding];
    [command commit];
    [command waitUntilCompleted];
    if (command.status == MTLCommandBufferStatusError) {
        failure = "preview validation render failed: " + std::string(
            command.error.localizedDescription.UTF8String ?: "unknown error");
        return false;
    }
    if (elapsed_ms) {
        const double gpu_ms = command_buffer_gpu_ms(command);
        *elapsed_ms = gpu_ms > 0.0 ? gpu_ms : -[start timeIntervalSinceNow] * 1000.0;
    }
    return true;
}

double median_time(std::vector<double> values) {
    std::sort(values.begin(), values.end());
    return values[values.size() / 2u];
}

FptRenderConfig generated_topology_config(
    const FptRenderConfig &source,
    bool analytic_surface) {
    FptRenderConfig generated = source;
    generated.sdf_typed_soa = {};
    generated.sdf_flat_union_count = 0u;
    generated.sdf_function_stitching = 0u;
    generated.sdf_topology_specialization = 1u;
    generated.sdf_runtime_source_bytecode = 0u;
    generated.sdf_stitched_surface = analytic_surface ? 1u : 0u;
    generated.sdf_stitch_validation = 0u;
    generated.sdf_stitch_distance_only = FPT_SDF_STITCH_STATE_FULL;
    generated.sdf_stitch_split_graph = 0u;
    generated.sdf_stitch_fusion = 0u;
    return generated;
}

NSData *procedural_topology_key(const FptRenderConfig &config,
                                NSString *function_name) {
    NSMutableData *key = [NSMutableData data];
    const char tag[] = "fpt-metal-preview-topology-v8";
    [key appendBytes:tag length:sizeof(tag)];
    const uint32_t header[] = {
        config.sdf_id,
        config.sdf_program_count,
        config.sdf_topology_specialization,
        config.sdf_function_stitching,
        config.sdf_stitched_surface,
        config.sdf_stitch_distance_only,
        config.sdf_stitch_split_graph,
        config.sdf_stitch_fusion,
        config.sdf_geometry_split,
    };
    [key appendBytes:header length:sizeof(header)];
    const uint32_t instruction_count = std::min<uint32_t>(
        config.sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
    for (uint32_t index = 0u; index < instruction_count; ++index) {
        const uint32_t topology[] = {
            config.sdf_program[index].opcode,
            config.sdf_program[index].flags,
        };
        [key appendBytes:topology length:sizeof(topology)];
    }
    NSData *function_data = [function_name dataUsingEncoding:NSUTF8StringEncoding];
    if (function_data) [key appendData:function_data];
    return key;
}

NSData *procedural_workload_key(const FptRenderConfig &config) {
    NSMutableData *key = [NSMutableData data];
    const char tag[] = "fpt-metal-preview-workload-v8";
    [key appendBytes:tag length:sizeof(tag)];
    // FptRenderConfig is zero-initialized POD. The complete snapshot is a
    // deliberately conservative workload identity: it includes material,
    // bounce, roulette, camera, lighting, and numeric geometry state.
    [key appendBytes:&config length:sizeof(config)];
    return key;
}

bool build_generated_topology_pipeline(
    id<MTLDevice> device,
    const std::string &shader_source,
    const FptRenderConfig &config,
    NSString *function_name,
    id<MTLComputePipelineState> __strong *pipeline,
    bool *cache_hit,
    std::string &failure) {
    std::string generated_source;
    if (!generate_topology_specialized_source(
            shader_source.data(), shader_source.size(), config,
            generated_source, failure)) {
        return false;
    }
    NSString *metal_source = [[NSString alloc]
        initWithBytes:generated_source.data()
               length:generated_source.size()
             encoding:NSUTF8StringEncoding];
    if (!metal_source) {
        failure = "failed to decode generated Metal source";
        return false;
    }
    NSError *library_error = nil;
    id<MTLLibrary> library = compile_runtime_source_library(
        device, metal_source, cache_hit, &library_error);
    if (!library) {
        failure = "generated Metal compilation failed: " + std::string(
            library_error.localizedDescription.UTF8String ?: "unknown error");
        return false;
    }
    id<MTLFunction> function = [library newFunctionWithName:function_name];
    NSError *pipeline_error = nil;
    *pipeline = function
        ? [device newComputePipelineStateWithFunction:function error:&pipeline_error]
        : nil;
    if (!*pipeline) {
        failure = "generated Metal pipeline failed: " + std::string(
            pipeline_error.localizedDescription.UTF8String ?: "kernel missing");
        return false;
    }
    return true;
}

struct FptAccumulationChunkCpp {
    uint32_t start_sample;
    uint32_t sample_count;
};

struct FptAccumulationTileCpp {
    uint32_t dispatch_origin[2];
};

struct RegionalProgramHeaderCpp {
    uint32_t instruction_offset;
    uint32_t instruction_count;
    uint32_t primitive_offset;
    uint32_t primitive_count;
};

struct RegionalProgramValidationCountsCpp {
    uint32_t sampled_distance_failures;
};

struct RegionalProgramProofCpp {
    uint32_t flags;
    uint32_t pruned_primitives;
};

struct RegionalProgramLocalStatsCpp {
    uint32_t distance_evaluations[3];
    uint32_t atlas_evaluations[3];
    uint32_t full_program_evaluations[3];
    uint32_t cell_entries[3];
    uint32_t same_cell_reuses[3];
    uint32_t same_program_reuses[3];
    uint32_t program_id_loads[3];
    uint32_t header_loads[3];
    uint32_t dynamic_instructions[3];
    uint32_t profiled_paths;
};

enum RegionalProgramProofFlagCpp : uint32_t {
    RegionalProofStrictDominanceCpp = 1u << 0u,
    RegionalProofRepeatSeamFallbackCpp = 1u << 1u,
    RegionalProofUnsupportedFallbackCpp = 1u << 2u,
    RegionalProofInvalidPrimitiveFallbackCpp = 1u << 3u,
    RegionalProofNoPrimitiveFallbackCpp = 1u << 4u,
    RegionalProofNoDominanceCpp = 1u << 5u,
};

static_assert(sizeof(RegionalProgramHeaderCpp) == 16u,
              "RegionalProgramHeader layout must match Metal");
static_assert(sizeof(RegionalProgramProofCpp) == 8u,
              "RegionalProgramProof layout must match Metal");
static_assert(sizeof(RegionalProgramLocalStatsCpp) == 112u,
              "RegionalProgramLocalStats layout must match Metal");

struct VoxelCellCpp {
    uint32_t packed_color;
    uint32_t packed_properties;
    float emission;
};

static_assert(sizeof(VoxelCellCpp) == 12u, "VoxelCell layout must match Metal");

struct VoxelStorageResult {
    __strong id<MTLBuffer> cells = nil;
    __strong id<MTLBuffer> page_table = nil;
    uint32_t active_bricks = 0u;
    uint64_t active_cells = 0u;
    uint64_t resident_bytes = 0u;
    uint32_t rejected_bricks = 0u;
    bool overflow = false;
};

struct VoxelBuildStateCpp {
    uint32_t next_page;
    uint32_t overflow;
    uint32_t active_cells;
    uint32_t rejected_bricks;
};

static_assert(sizeof(VoxelBuildStateCpp) == 16u, "VoxelBuildState layout must match Metal");

size_t voxel_page_count(uint32_t dimension) {
    return static_cast<size_t>(dimension) * dimension * dimension;
}

bool finalize_voxel_acceleration_layout(id<MTLDevice> device,
                                        const FptRenderConfig &config,
                                        VoxelStorageResult &result) {
    if (config.voxel_storage == FPT_VOXEL_STORAGE_TEMPLATE_BRICKS) {
        const uint32_t brick_grid = (config.voxel_resolution + 3u) / 4u;
        const size_t page_count = voxel_page_count(brick_grid);
        if (!result.page_table || result.page_table.length < page_count * sizeof(uint32_t) ||
            !result.cells) return false;
        const auto *pages = static_cast<const uint32_t *>(result.page_table.contents);
        const auto *cells = static_cast<const VoxelCellCpp *>(result.cells.contents);
        std::vector<uint32_t> remapped(pages, pages + page_count);
        std::unordered_map<uint64_t, uint32_t> ids;
        std::vector<uint64_t> masks;
        for (size_t index = 0u; index < page_count; ++index) {
            const uint32_t page = pages[index];
            if (page == 0u) continue;
            uint64_t mask = 0u;
            const size_t first = static_cast<size_t>(page - 1u) * 64u;
            for (uint32_t local = 0u; local < 64u; ++local) {
                if ((cells[first + local].packed_color & 0x80000000u) != 0u) {
                    mask |= uint64_t{1} << local;
                }
            }
            auto [found, inserted] = ids.emplace(mask, static_cast<uint32_t>(masks.size() + 1u));
            if (inserted) masks.push_back(mask);
            remapped[index] = found->second;
        }
        if (masks.empty()) masks.push_back(0u);
        result.cells = [device newBufferWithBytes:masks.data()
                                           length:masks.size() * sizeof(uint64_t)
                                          options:MTLResourceStorageModeShared];
        result.page_table = [device newBufferWithBytes:remapped.data()
                                                length:remapped.size() * sizeof(uint32_t)
                                               options:MTLResourceStorageModeShared];
        if (!result.cells || !result.page_table) return false;
    }
    result.resident_bytes = result.cells.length + result.page_table.length;
    return true;
}

VoxelStorageResult finalize_voxel_storage(id<MTLDevice> device,
                                          const FptRenderConfig &config,
                                          id<MTLBuffer> dense_cells) {
    VoxelStorageResult result;
    const uint32_t resolution = config.voxel_resolution;
    const uint32_t zero = 0u;
    if (config.voxel_storage == FPT_VOXEL_STORAGE_DENSE) {
        result.cells = dense_cells;
        result.page_table = [device newBufferWithBytes:&zero
                                                length:sizeof(zero)
                                               options:MTLResourceStorageModeShared];
        result.resident_bytes = dense_cells.length + result.page_table.length;
        const auto *dense = static_cast<const VoxelCellCpp *>(dense_cells.contents);
        const size_t cell_count = static_cast<size_t>(resolution) * resolution * resolution;
        for (size_t index = 0u; index < cell_count; ++index) {
            result.active_cells += (dense[index].packed_color & 0x80000000u) != 0u ? 1u : 0u;
        }
        return result;
    }

    constexpr uint32_t brick_size = 4u;
    constexpr uint32_t brick_voxels = brick_size * brick_size * brick_size;
    const uint32_t brick_grid = (resolution + brick_size - 1u) / brick_size;
    const size_t page_count = static_cast<size_t>(brick_grid) * brick_grid * brick_grid;
    std::vector<uint32_t> page_table(page_count, 0u);
    const auto *dense = static_cast<const VoxelCellCpp *>(dense_cells.contents);
    for (uint32_t bz = 0u; bz < brick_grid; ++bz) {
        for (uint32_t by = 0u; by < brick_grid; ++by) {
            for (uint32_t bx = 0u; bx < brick_grid; ++bx) {
                bool occupied = false;
                for (uint32_t lz = 0u; lz < brick_size && !occupied; ++lz) {
                    const uint32_t z = bz * brick_size + lz;
                    if (z >= resolution) continue;
                    for (uint32_t ly = 0u; ly < brick_size && !occupied; ++ly) {
                        const uint32_t y = by * brick_size + ly;
                        if (y >= resolution) continue;
                        for (uint32_t lx = 0u; lx < brick_size; ++lx) {
                            const uint32_t x = bx * brick_size + lx;
                            if (x >= resolution) continue;
                            const size_t index = x + static_cast<size_t>(y) * resolution +
                                                 static_cast<size_t>(z) * resolution * resolution;
                            if ((dense[index].packed_color & 0x80000000u) != 0u) {
                                occupied = true;
                                break;
                            }
                        }
                    }
                }
                if (occupied) {
                    const size_t page = bx + static_cast<size_t>(by) * brick_grid +
                                        static_cast<size_t>(bz) * brick_grid * brick_grid;
                    page_table[page] = ++result.active_bricks;
                }
            }
        }
    }

    const size_t compact_count = std::max<size_t>(1u,
        static_cast<size_t>(result.active_bricks) * brick_voxels);
    result.cells = [device newBufferWithLength:compact_count * sizeof(VoxelCellCpp)
                                       options:MTLResourceStorageModeShared];
    result.page_table = [device newBufferWithBytes:page_table.data()
                                                length:page_table.size() * sizeof(uint32_t)
                                               options:MTLResourceStorageModeShared];
    if (!result.cells || !result.page_table) return {};
    std::memset(result.cells.contents, 0, result.cells.length);
    auto *compact = static_cast<VoxelCellCpp *>(result.cells.contents);
    for (uint32_t bz = 0u; bz < brick_grid; ++bz) {
        for (uint32_t by = 0u; by < brick_grid; ++by) {
            for (uint32_t bx = 0u; bx < brick_grid; ++bx) {
                const size_t page = bx + static_cast<size_t>(by) * brick_grid +
                                    static_cast<size_t>(bz) * brick_grid * brick_grid;
                const uint32_t compact_page = page_table[page];
                if (compact_page == 0u) continue;
                for (uint32_t lz = 0u; lz < brick_size; ++lz) {
                    const uint32_t z = bz * brick_size + lz;
                    if (z >= resolution) continue;
                    for (uint32_t ly = 0u; ly < brick_size; ++ly) {
                        const uint32_t y = by * brick_size + ly;
                        if (y >= resolution) continue;
                        for (uint32_t lx = 0u; lx < brick_size; ++lx) {
                            const uint32_t x = bx * brick_size + lx;
                            if (x >= resolution) continue;
                            const size_t dense_index = x + static_cast<size_t>(y) * resolution +
                                                       static_cast<size_t>(z) * resolution * resolution;
                            const size_t local = lx + ly * brick_size + lz * brick_size * brick_size;
                            compact[(static_cast<size_t>(compact_page) - 1u) * brick_voxels + local] = dense[dense_index];
                            result.active_cells +=
                                (dense[dense_index].packed_color & 0x80000000u) != 0u ? 1u : 0u;
                        }
                    }
                }
            }
        }
    }
    result.resident_bytes = result.cells.length + result.page_table.length;
    if (!finalize_voxel_acceleration_layout(device, config, result)) return {};
    return result;
}

VoxelStorageResult build_direct_voxel_storage(id<MTLDevice> device,
                                              id<MTLCommandQueue> queue,
                                              id<MTLComputePipelineState> pipeline,
                                              id<MTLBuffer> config_buffer,
                                              const FptRenderConfig &config) {
    VoxelStorageResult result;
    constexpr uint64_t brick_bytes = 64u * sizeof(VoxelCellCpp);
    const uint32_t brick_grid = (config.voxel_resolution + 3u) / 4u;
    const uint64_t page_count = static_cast<uint64_t>(brick_grid) * brick_grid * brick_grid;
    const uint64_t page_bytes = page_count * sizeof(uint32_t);
    const uint64_t memory_budget = config.voxel_resolution >= 512u
        ? 480u * 1024u * 1024u
        : page_bytes + page_count * brick_bytes;
    const uint64_t available = memory_budget > page_bytes ? memory_budget - page_bytes : brick_bytes;
    const uint32_t page_capacity = static_cast<uint32_t>(std::max<uint64_t>(
        1u, std::min<uint64_t>(page_count, available / brick_bytes)));

    result.cells = [device newBufferWithLength:static_cast<size_t>(page_capacity) * brick_bytes
                                       options:MTLResourceStorageModeShared];
    result.page_table = [device newBufferWithLength:static_cast<size_t>(page_bytes)
                                            options:MTLResourceStorageModeShared];
    const VoxelBuildStateCpp zero_state = {};
    id<MTLBuffer> state_buffer = [device newBufferWithBytes:&zero_state
                                                     length:sizeof(zero_state)
                                                    options:MTLResourceStorageModeShared];
    if (!result.cells || !result.page_table || !state_buffer) return {};
    std::memset(result.page_table.contents, 0, result.page_table.length);

    id<MTLCommandBuffer> command = [queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
    [encoder setComputePipelineState:pipeline];
    [encoder setBuffer:result.cells offset:0 atIndex:0];
    [encoder setBuffer:result.page_table offset:0 atIndex:1];
    [encoder setBuffer:state_buffer offset:0 atIndex:2];
    [encoder setBuffer:config_buffer offset:0 atIndex:3];
    [encoder setBytes:&page_capacity length:sizeof(page_capacity) atIndex:4];
    [encoder dispatchThreadgroups:MTLSizeMake(brick_grid, brick_grid, brick_grid)
             threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
    [encoder endEncoding];
    [command commit];
    [command waitUntilCompleted];
    if (command.status == MTLCommandBufferStatusError) return {};

    const auto *state = static_cast<const VoxelBuildStateCpp *>(state_buffer.contents);
    result.overflow = state->overflow != 0u || state->next_page > page_capacity;
    result.active_bricks = std::min(state->next_page, page_capacity);
    result.active_cells = state->active_cells;
    result.rejected_bricks = state->rejected_bricks;
    result.resident_bytes = result.cells.length + result.page_table.length;
    if (!result.overflow &&
        !finalize_voxel_acceleration_layout(device, config, result)) return {};
    return result;
}

NSString *format_perf_hud(NSString *renderer,
                          NSString *gpu_breakdown,
                          NSString *cpu_breakdown,
                          NSString *bottleneck,
                          double gpu_ms,
                          double cpu_ms,
                          uint32_t frame,
                          uint32_t samples) {
    const double frame_ms = gpu_ms > 0.0 ? gpu_ms : cpu_ms;
    const double fps = frame_ms > 0.0 ? 1000.0 / frame_ms : 0.0;
    NSString *sample_text = samples > 1
        ? [NSString stringWithFormat:@" | sample %u/%u", std::min(frame + 1u, samples), samples]
        : @"";
    return [NSString stringWithFormat:@"%@%@\nGPU %.2f ms  %.1f FPS | CPU frame %.2f ms\nGPU: %@\n%@\n%@",
            renderer,
            sample_text,
            gpu_ms,
            fps,
            cpu_ms,
            gpu_breakdown,
            cpu_breakdown,
            bottleneck];
}

NSString *format_stage_ms(NSString *name, double stage_ms, double total_ms) {
    const double pct = total_ms > 0.0 ? (stage_ms / total_ms) * 100.0 : 0.0;
    return [NSString stringWithFormat:@"%@ %.2f ms %.0f%%", name, stage_ms, pct];
}

NSString *format_cpu_breakdown(double encode_ms, double wait_ms, double image_ms) {
    return [NSString stringWithFormat:@"CPU encode %.2f ms, wait %.2f ms, image %.2f ms",
            encode_ms,
            wait_ms,
            image_ms];
}

NSTextField *make_perf_hud(NSView *parent) {
    NSTextField *label = [NSTextField labelWithString:@""];
    label.translatesAutoresizingMaskIntoConstraints = NO;
    label.drawsBackground = YES;
    label.backgroundColor = [[NSColor blackColor] colorWithAlphaComponent:0.62];
    label.textColor = [NSColor whiteColor];
    label.font = [NSFont monospacedSystemFontOfSize:12.0 weight:NSFontWeightRegular];
    label.lineBreakMode = NSLineBreakByWordWrapping;
    label.maximumNumberOfLines = 7;
    label.editable = NO;
    label.selectable = NO;
    label.bezeled = NO;
    label.wantsLayer = YES;
    label.layer.cornerRadius = 4.0;
    label.layer.masksToBounds = YES;
    [parent addSubview:label];
    [NSLayoutConstraint activateConstraints:@[
        [label.leadingAnchor constraintEqualToAnchor:parent.leadingAnchor constant:10.0],
        [label.topAnchor constraintEqualToAnchor:parent.topAnchor constant:10.0],
        [label.widthAnchor constraintLessThanOrEqualToAnchor:parent.widthAnchor multiplier:0.86],
    ]];
    return label;
}

NSString *sdf_bottleneck_label(const struct FptRenderConfig &config) {
    if (config.preview != 0) return @"SDF ray march, normal/material shading";
    if (config.sdf_id == FPT_SDF_GLASS_BALL) return @"glass analytic/pathtrace branch and sample accumulation";
    if (config.sdf_id == FPT_SDF_README_GLASS) return @"glass pathtrace, room bounces, and normal estimation";
    if (config.sdf_id == FPT_SDF_CORNELL_BOX) return @"path bounces and normal estimation";
    return @"fractal SDF march, path bounces, normal estimation";
}

bool interactive_preview_dispatch_extent(const FptRenderConfig &config,
                                         uint32_t &width,
                                         uint32_t &height) {
    if (config.preview == 0u || config.sdf_id != FPT_SDF_MANDELBULBER) return false;
    const int32_t spatial_mode = int32_t(std::round(config.vset_values[132]));
    if (spatial_mode != 0) {
        const uint32_t stride = std::clamp<uint32_t>(
            spatial_mode < 0 ? uint32_t(-spatial_mode)
                             : uint32_t(spatial_mode) >> 16u,
            2u, 16u);
        width = (config.width + stride - 1u) / stride;
        height = (config.height + stride - 1u) / stride;
        return width > 0u && height > 0u;
    }
    const float state = config.vset_values[131];
    if (!(state >= 1.0f) || !std::isfinite(state)) return false;
    const uint32_t mask = std::min(uint32_t(state), 0xffffu);
    if (mask == 0u || (mask & (mask - 1u)) != 0u) return false;
    const uint32_t tile = uint32_t(__builtin_ctz(mask));
    const uint32_t column = tile & 3u;
    const uint32_t row = tile >> 2u;
    const uint32_t x0 = (column * config.width + 3u) / 4u;
    const uint32_t x1 = ((column + 1u) * config.width + 3u) / 4u;
    const uint32_t y0 = (row * config.height + 3u) / 4u;
    const uint32_t y1 = ((row + 1u) * config.height + 3u) / 4u;
    width = x1 - x0;
    height = y1 - y0;
    return width > 0u && height > 0u;
}

NSString *sdf_scene_name(uint32_t sdf_id) {
    switch (sdf_id) {
        case FPT_SDF_CORNELL_BOX: return @"Cornell_Box";
        case FPT_SDF_GLASS_BALL: return @"Glass_Ball";
        case FPT_SDF_BALL_FRACTAL: return @"Ball_Fractal";
        case FPT_SDF_CAGE_FRACTAL: return @"Cage_Fractal";
        case FPT_SDF_IFS_FRACTAL: return @"IFS_Fractal";
        case FPT_SDF_MANDELBOX_FRACTAL: return @"Mandelbox_Fractal";
        case FPT_SDF_MENGER_SPONGE: return @"Menger_Sponge";
        case FPT_SDF_TOWER_FRACTAL: return @"Tower_Fractal";
        case FPT_SDF_TREE_FRACTAL: return @"Tree_Fractal";
        case FPT_SDF_README_CORNELL: return @"README_Cornell";
        case FPT_SDF_README_GLASS: return @"README_Glass";
        case FPT_SDF_MANDELBULBER: return @"Mandelbulber";
        case FPT_SDF_PROGRAM: return @"Procedural_Program";
        default: return @"Unknown";
    }
}

struct FptSdfProfileConfigCpp {
    uint32_t frame_index;
    uint32_t stride;
    uint32_t pad0;
    uint32_t pad1;
};

struct FptSdfProfileCountsCpp {
    uint32_t primary_steps;
    uint32_t secondary_steps;
    uint32_t shadow_steps;
    uint32_t normal_evals;
    uint32_t bounces;
    uint32_t pixels;
    uint32_t distance_evals;
    uint32_t march_orbit_iterations;
    uint32_t refinement_steps;
    uint32_t normal_field_evals;
    uint32_t material_evals;
    uint32_t max_ray_steps;
    uint32_t max_pixel_steps;
    uint32_t distance_evals_by_phase[4];
    uint32_t orbit_iterations_by_phase[4];
    uint32_t formula_slot_iterations[9];
    uint32_t refinement_distance_evals;
};

struct BoundGridLocalStatsCpp {
    uint32_t macro_cells[3];
    uint32_t certified_skips[3];
    uint32_t candidate_intervals[3];
    uint32_t candidate_misses[3];
    uint32_t candidate_hits[3];
    uint32_t unknown_intervals[3];
    uint32_t field_evaluations[3];
    uint32_t directional_steps[3];
    uint32_t cell_exit_clamps[3];
    uint32_t unknown_derivative_intervals[3];
    uint32_t profiled_paths;
};

struct BoundGridValidationCountsCpp {
    uint32_t certified_cells;
    uint32_t unknown_cells;
    uint32_t sampled_bound_failures;
    uint32_t sampled_false_skips;
    uint32_t certified_derivative_cells;
    uint32_t unknown_derivative_cells;
    uint32_t sampled_derivative_failures;
};

static_assert(sizeof(BoundGridLocalStatsCpp) == 124u,
              "Metal/C++ bound-grid profile layout mismatch");

NSString *format_sdf_work_breakdown(const FptSdfProfileCountsCpp &counts,
                                    double sample_gpu_ms,
                                    const FptRenderConfig &config) {
    (void)config;
    const double primary_units = static_cast<double>(counts.primary_steps);
    const double secondary_units = static_cast<double>(counts.secondary_steps);
    const double shadow_units = static_cast<double>(counts.shadow_steps);
    const double normal_units = counts.normal_field_evals > 0u
        ? static_cast<double>(counts.normal_field_evals)
        : static_cast<double>(counts.normal_evals);
    const double bounce_units = static_cast<double>(counts.bounces);
    const double total_units = primary_units + secondary_units + shadow_units +
                               normal_units + bounce_units;
    if (sample_gpu_ms <= 0.0 || total_units <= 0.0 || counts.pixels == 0u) {
        return @"SDF work est: profiling...";
    }
    const double primary_ms = sample_gpu_ms * primary_units / total_units;
    const double secondary_ms = sample_gpu_ms * secondary_units / total_units;
    const double shadow_ms = sample_gpu_ms * shadow_units / total_units;
    const double normal_ms = sample_gpu_ms * normal_units / total_units;
    const double bounce_ms = sample_gpu_ms * bounce_units / total_units;
    return [NSString stringWithFormat:@"SDF est: primary %.2f ms, secondary %.2f ms, shadow %.2f ms, normal %.2f ms, bounce %.2f ms",
            primary_ms,
            secondary_ms,
            shadow_ms,
            normal_ms,
            bounce_ms];
}

bool write_png(const char *path, uint32_t width, uint32_t height, const std::vector<uint8_t> &rgba, char *error, size_t error_len) {
    CGColorSpaceRef color_space = CGColorSpaceCreateDeviceRGB();
    if (!color_space) {
        set_error(error, error_len, "failed to create RGB color space");
        return false;
    }
    CGDataProviderRef provider = CGDataProviderCreateWithData(nullptr, rgba.data(), rgba.size(), nullptr);
    if (!provider) {
        CGColorSpaceRelease(color_space);
        set_error(error, error_len, "failed to create image data provider");
        return false;
    }
    CGImageRef image = CGImageCreate(width,
                                     height,
                                     8,
                                     32,
                                     static_cast<size_t>(width) * 4,
                                     color_space,
                                     kCGImageAlphaLast | kCGBitmapByteOrder32Big,
                                     provider,
                                     nullptr,
                                     false,
                                     kCGRenderingIntentDefault);
    CGDataProviderRelease(provider);
    CGColorSpaceRelease(color_space);
    if (!image) {
        set_error(error, error_len, "failed to create CGImage");
        return false;
    }

    NSURL *url = [NSURL fileURLWithPath:ns_string(path)];
    CGImageDestinationRef dest = CGImageDestinationCreateWithURL((__bridge CFURLRef)url, CFSTR("public.png"), 1, nullptr);
    if (!dest) {
        CGImageRelease(image);
        set_error(error, error_len, "failed to create PNG destination for %s", path);
        return false;
    }
    CGImageDestinationAddImage(dest, image, nullptr);
    const bool ok = CGImageDestinationFinalize(dest);
    CFRelease(dest);
    CGImageRelease(image);
    if (!ok) {
        set_error(error, error_len, "failed to write PNG %s", path);
        return false;
    }
    return true;
}

struct Image {
    uint32_t width = 0;
    uint32_t height = 0;
    std::vector<uint8_t> rgba;
};

bool read_image(const char *path, Image &out, char *error, size_t error_len) {
    NSURL *url = [NSURL fileURLWithPath:ns_string(path)];
    CGImageSourceRef source = CGImageSourceCreateWithURL((__bridge CFURLRef)url, nullptr);
    if (!source) {
        set_error(error, error_len, "failed to open image %s", path);
        return false;
    }
    CGImageRef image = CGImageSourceCreateImageAtIndex(source, 0, nullptr);
    CFRelease(source);
    if (!image) {
        set_error(error, error_len, "failed to decode image %s", path);
        return false;
    }
    out.width = static_cast<uint32_t>(CGImageGetWidth(image));
    out.height = static_cast<uint32_t>(CGImageGetHeight(image));
    out.rgba.assign(static_cast<size_t>(out.width) * out.height * 4, 0);

    CGColorSpaceRef color_space = CGColorSpaceCreateDeviceRGB();
    CGContextRef ctx = CGBitmapContextCreate(out.rgba.data(),
                                             out.width,
                                             out.height,
                                             8,
                                             static_cast<size_t>(out.width) * 4,
                                             color_space,
                                             kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big);
    CGColorSpaceRelease(color_space);
    if (!ctx) {
        CGImageRelease(image);
        set_error(error, error_len, "failed to create image decode context");
        return false;
    }
    CGContextDrawImage(ctx, CGRectMake(0, 0, out.width, out.height), image);
    CGContextRelease(ctx);
    CGImageRelease(image);
    return true;
}

uint16_t float_to_half(float value) {
    uint32_t bits = 0;
    std::memcpy(&bits, &value, sizeof(bits));
    const uint32_t sign = (bits >> 16u) & 0x8000u;
    int exponent = int((bits >> 23u) & 0xffu) - 127 + 15;
    uint32_t mantissa = bits & 0x7fffffu;
    if (exponent <= 0) {
        if (exponent < -10) return static_cast<uint16_t>(sign);
        mantissa = (mantissa | 0x800000u) >> uint32_t(1 - exponent);
        return static_cast<uint16_t>(sign | ((mantissa + 0x1000u) >> 13u));
    }
    if (exponent >= 31) return static_cast<uint16_t>(sign | 0x7c00u);
    return static_cast<uint16_t>(sign | (uint32_t(exponent) << 10u) | ((mantissa + 0x1000u) >> 13u));
}

struct HdrImage {
    uint32_t width = 0;
    uint32_t height = 0;
    std::vector<float> rgb;
};

bool read_radiance_hdr(const char *path, HdrImage &out, char *error, size_t error_len) {
    std::ifstream stream(path, std::ios::binary);
    if (!stream) {
        set_error(error, error_len, "failed to open HDRI %s", path);
        return false;
    }
    std::string line;
    bool format_ok = false;
    while (std::getline(stream, line)) {
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (line.rfind("FORMAT=32-bit_rle_rgbe", 0) == 0) format_ok = true;
        if (line.empty()) break;
    }
    if (!std::getline(stream, line)) {
        set_error(error, error_len, "missing HDRI resolution in %s", path);
        return false;
    }
    char y_sign = 0, x_sign = 0;
    int width = 0, height = 0;
    if (std::sscanf(line.c_str(), " %cY %d %cX %d", &y_sign, &height, &x_sign, &width) != 4 || width <= 0 || height <= 0) {
        set_error(error, error_len, "unsupported HDRI resolution line '%s'", line.c_str());
        return false;
    }
    if (!format_ok || width < 8 || width > 32767) {
        set_error(error, error_len, "unsupported non-RLE Radiance HDRI %s", path);
        return false;
    }
    out.width = static_cast<uint32_t>(width);
    out.height = static_cast<uint32_t>(height);
    out.rgb.assign(static_cast<size_t>(width) * height * 3u, 0.0f);
    std::vector<uint8_t> scanline(static_cast<size_t>(width) * 4u);
    for (int file_y = 0; file_y < height; ++file_y) {
        uint8_t header[4] = {};
        stream.read(reinterpret_cast<char *>(header), 4);
        if (!stream || header[0] != 2u || header[1] != 2u || ((uint32_t(header[2]) << 8u) | header[3]) != static_cast<uint32_t>(width)) {
            set_error(error, error_len, "invalid HDRI scanline %d in %s", file_y, path);
            return false;
        }
        for (int channel = 0; channel < 4; ++channel) {
            int x = 0;
            while (x < width) {
                uint8_t code = 0;
                stream.read(reinterpret_cast<char *>(&code), 1);
                if (!stream || code == 0u) {
                    set_error(error, error_len, "truncated HDRI scanline %d in %s", file_y, path);
                    return false;
                }
                if (code > 128u) {
                    const int count = int(code - 128u);
                    uint8_t value = 0;
                    stream.read(reinterpret_cast<char *>(&value), 1);
                    if (!stream || x + count > width) return false;
                    for (int i = 0; i < count; ++i) scanline[static_cast<size_t>(x++ * 4 + channel)] = value;
                } else {
                    const int count = int(code);
                    if (x + count > width) return false;
                    for (int i = 0; i < count; ++i) {
                        uint8_t value = 0;
                        stream.read(reinterpret_cast<char *>(&value), 1);
                        scanline[static_cast<size_t>(x++ * 4 + channel)] = value;
                    }
                }
            }
        }
        const int output_y = y_sign == '-' ? file_y : height - 1 - file_y;
        for (int file_x = 0; file_x < width; ++file_x) {
            const int output_x = x_sign == '+' ? file_x : width - 1 - file_x;
            const uint8_t exponent = scanline[static_cast<size_t>(file_x) * 4u + 3u];
            const float scale = exponent == 0u ? 0.0f : std::ldexp(1.0f, int(exponent) - (128 + 8));
            const size_t dst = (static_cast<size_t>(output_y) * width + output_x) * 3u;
            out.rgb[dst + 0u] = (float(scanline[static_cast<size_t>(file_x) * 4u + 0u]) + 0.5f) * scale;
            out.rgb[dst + 1u] = (float(scanline[static_cast<size_t>(file_x) * 4u + 1u]) + 0.5f) * scale;
            out.rgb[dst + 2u] = (float(scanline[static_cast<size_t>(file_x) * 4u + 2u]) + 0.5f) * scale;
        }
    }
    return true;
}

bool hydrate_hdri_config(FptRenderConfig &config, char *error, size_t error_len) {
    config.hdri_width = 0u;
    config.hdri_height = 0u;
    std::memset(config.hdri_lut, 0, sizeof(config.hdri_lut));
    if (config.hdri_enabled == 0u || config.world[0] != 2.0f) return true;
    const char *path = reinterpret_cast<const char *>(config.hdri_path);
    if (!path[0]) {
        set_error(error, error_len, "HDRI world mode requires world.hdri");
        return false;
    }
    HdrImage image;
    if (!read_radiance_hdr(path, image, error, error_len)) return false;
    config.hdri_width = FPT_HDRI_LUT_WIDTH;
    config.hdri_height = FPT_HDRI_LUT_HEIGHT;
    for (uint32_t y = 0; y < FPT_HDRI_LUT_HEIGHT; ++y) {
        const uint32_t sy0 = (uint64_t(y) * image.height) / FPT_HDRI_LUT_HEIGHT;
        const uint32_t sy1 = std::max(sy0 + 1u, uint32_t((uint64_t(y + 1u) * image.height) / FPT_HDRI_LUT_HEIGHT));
        for (uint32_t x = 0; x < FPT_HDRI_LUT_WIDTH; ++x) {
            const uint32_t sx0 = (uint64_t(x) * image.width) / FPT_HDRI_LUT_WIDTH;
            const uint32_t sx1 = std::max(sx0 + 1u, uint32_t((uint64_t(x + 1u) * image.width) / FPT_HDRI_LUT_WIDTH));
            float sum[3] = {};
            uint32_t samples = 0u;
            for (uint32_t sy = sy0; sy < std::min(sy1, image.height); ++sy) {
                for (uint32_t sx = sx0; sx < std::min(sx1, image.width); ++sx) {
                    const size_t source = (static_cast<size_t>(sy) * image.width + sx) * 3u;
                    sum[0] += image.rgb[source + 0u];
                    sum[1] += image.rgb[source + 1u];
                    sum[2] += image.rgb[source + 2u];
                    samples += 1u;
                }
            }
            const size_t destination = (static_cast<size_t>(y) * FPT_HDRI_LUT_WIDTH + x) * 3u;
            for (uint32_t channel = 0; channel < 3u; ++channel) {
                config.hdri_lut[destination + channel] = float_to_half(sum[channel] / float(std::max(samples, 1u)));
            }
        }
    }
    return true;
}

size_t unique_colour_sample(const std::vector<uint8_t> &rgba) {
    std::vector<uint32_t> colours;
    colours.reserve(256);
    for (size_t i = 0; i + 3 < rgba.size(); i += 4) {
        const uint32_t c = (uint32_t(rgba[i]) << 24u) | (uint32_t(rgba[i + 1]) << 16u) | (uint32_t(rgba[i + 2]) << 8u) | uint32_t(rgba[i + 3]);
        if (std::find(colours.begin(), colours.end(), c) == colours.end()) {
            colours.push_back(c);
            if (colours.size() >= 2048) break;
        }
    }
    return colours.size();
}

std::string replace_extension(const char *path, const char *suffix) {
    std::string out(path ? path : "");
    const size_t slash = out.find_last_of('/');
    const size_t dot = out.find_last_of('.');
    if (dot != std::string::npos && (slash == std::string::npos || dot > slash)) {
        out.resize(dot);
    }
    out += suffix;
    return out;
}

std::vector<uint8_t> comparison_sheet(const Image &baseline, const Image &candidate, uint32_t &out_w, uint32_t &out_h) {
    const uint32_t gap = 12;
    const uint32_t panel_w = 360u;
    const uint32_t panel_h = std::max<uint32_t>(1u, static_cast<uint32_t>(std::round(double(baseline.height) * double(panel_w) / double(baseline.width))));
    out_w = panel_w * 3u + gap * 2u;
    out_h = panel_h;
    std::vector<uint8_t> sheet(static_cast<size_t>(out_w) * out_h * 4u, 32u);
    for (uint32_t y = 0; y < out_h; ++y) {
        const uint32_t sy = std::min<uint32_t>(baseline.height - 1u, static_cast<uint32_t>((uint64_t(y) * baseline.height) / panel_h));
        for (uint32_t x = 0; x < panel_w; ++x) {
            const uint32_t sx = std::min<uint32_t>(baseline.width - 1u, static_cast<uint32_t>((uint64_t(x) * baseline.width) / panel_w));
            const size_t src = (static_cast<size_t>(sy) * baseline.width + sx) * 4u;
            const size_t dst_a = (static_cast<size_t>(y) * out_w + x) * 4u;
            const size_t dst_b = (static_cast<size_t>(y) * out_w + panel_w + gap + x) * 4u;
            const size_t dst_d = (static_cast<size_t>(y) * out_w + panel_w * 2u + gap * 2u + x) * 4u;
            for (size_t ch = 0; ch < 4; ++ch) {
                sheet[dst_a + ch] = baseline.rgba[src + ch];
                sheet[dst_b + ch] = candidate.rgba[src + ch];
            }
            const int dr = std::abs(int(baseline.rgba[src + 0]) - int(candidate.rgba[src + 0]));
            const int dg = std::abs(int(baseline.rgba[src + 1]) - int(candidate.rgba[src + 1]));
            const int db = std::abs(int(baseline.rgba[src + 2]) - int(candidate.rgba[src + 2]));
            const uint8_t heat = static_cast<uint8_t>(std::min(255, (dr + dg + db) / 3 * 4));
            sheet[dst_d + 0] = heat;
            sheet[dst_d + 1] = heat / 2;
            sheet[dst_d + 2] = 255u - heat;
            sheet[dst_d + 3] = 255u;
        }
    }
    return sheet;
}

struct Gate {
    const char *name;
    double mae_threshold;
    double strict_mae_threshold;
    double strict_ssim_threshold;
};

Gate gate_for_path(const char *path) {
    const std::string p(path ? path : "");
    const bool preview = p.find("preview-parity") != std::string::npos;
    if (preview && p.find("Ball_Fractal") != std::string::npos) return {"preview_ball_fractal", 50.0, 35.0, 0.50};
    if (preview && p.find("Glass_Ball") != std::string::npos) return {"preview_glass", 60.0, 42.0, 0.45};
    if (preview) return {"preview", 90.0, 55.0, 0.45};
    if (p.find("Cornell_Box") != std::string::npos) return {"cornell", 50.0, 30.0, 0.55};
    if (p.find("Glass_Ball") != std::string::npos) return {"glass", 50.0, 30.0, 0.30};
    if (p.find("Ball_Fractal") != std::string::npos) return {"ball_fractal", 45.0, 28.0, 0.55};
    if (p.find("Cage_Fractal") != std::string::npos) return {"cage_fractal", 70.0, 45.0, 0.45};
    if (p.find("IFS_Fractal") != std::string::npos) return {"ifs_fractal", 75.0, 48.0, 0.45};
    if (p.find("Mandelbox_Fractal") != std::string::npos) return {"mandelbox_fractal", 80.0, 52.0, 0.42};
    if (p.find("Menger_Sponge") != std::string::npos) return {"menger_sponge", 60.0, 40.0, 0.48};
    if (p.find("Tower_Fractal") != std::string::npos) return {"tower_fractal", 75.0, 48.0, 0.45};
    if (p.find("Tree_Fractal") != std::string::npos) return {"tree_fractal", 60.0, 38.0, 0.48};
    return {"generic", 90.0, 55.0, 0.45};
}

double luma_at(const Image &image, size_t pixel) {
    const uint8_t *p = &image.rgba[pixel * 4u];
    return 0.2126 * double(p[0]) + 0.7152 * double(p[1]) + 0.0722 * double(p[2]);
}

double luminance_ssim(const Image &baseline, const Image &candidate) {
    const uint32_t block = 8;
    const double c1 = 6.5025;
    const double c2 = 58.5225;
    double weighted_sum = 0.0;
    double weight_total = 0.0;
    for (uint32_t by = 0; by < baseline.height; by += block) {
        for (uint32_t bx = 0; bx < baseline.width; bx += block) {
            const uint32_t y_end = std::min<uint32_t>(baseline.height, by + block);
            const uint32_t x_end = std::min<uint32_t>(baseline.width, bx + block);
            const double n = double((y_end - by) * (x_end - bx));
            double mean_a = 0.0;
            double mean_b = 0.0;
            for (uint32_t y = by; y < y_end; ++y) {
                for (uint32_t x = bx; x < x_end; ++x) {
                    const size_t p = static_cast<size_t>(y) * baseline.width + x;
                    mean_a += luma_at(baseline, p);
                    mean_b += luma_at(candidate, p);
                }
            }
            mean_a /= n;
            mean_b /= n;

            double var_a = 0.0;
            double var_b = 0.0;
            double cov = 0.0;
            for (uint32_t y = by; y < y_end; ++y) {
                for (uint32_t x = bx; x < x_end; ++x) {
                    const size_t p = static_cast<size_t>(y) * baseline.width + x;
                    const double da = luma_at(baseline, p) - mean_a;
                    const double db = luma_at(candidate, p) - mean_b;
                    var_a += da * da;
                    var_b += db * db;
                    cov += da * db;
                }
            }
            const double denom = std::max(1.0, n - 1.0);
            var_a /= denom;
            var_b /= denom;
            cov /= denom;
            const double numerator = (2.0 * mean_a * mean_b + c1) * (2.0 * cov + c2);
            const double denominator = (mean_a * mean_a + mean_b * mean_b + c1) * (var_a + var_b + c2);
            weighted_sum += (denominator > 0.0 ? numerator / denominator : 1.0) * n;
            weight_total += n;
        }
    }
    return weight_total > 0.0 ? std::clamp(weighted_sum / weight_total, -1.0, 1.0) : 0.0;
}

double low_frequency_luminance_ssim(const Image &baseline, const Image &candidate) {
    const uint32_t target_w = 64;
    const uint32_t target_h = 64;
    std::vector<double> a(static_cast<size_t>(target_w) * target_h, 0.0);
    std::vector<double> b(static_cast<size_t>(target_w) * target_h, 0.0);
    for (uint32_t y = 0; y < target_h; ++y) {
        uint32_t y0 = static_cast<uint32_t>((uint64_t(y) * baseline.height) / target_h);
        uint32_t y1 = static_cast<uint32_t>((uint64_t(y + 1u) * baseline.height) / target_h);
        y1 = std::max(y1, y0 + 1u);
        for (uint32_t x = 0; x < target_w; ++x) {
            uint32_t x0 = static_cast<uint32_t>((uint64_t(x) * baseline.width) / target_w);
            uint32_t x1 = static_cast<uint32_t>((uint64_t(x + 1u) * baseline.width) / target_w);
            x1 = std::max(x1, x0 + 1u);
            double sum_a = 0.0;
            double sum_b = 0.0;
            double n = 0.0;
            for (uint32_t sy = y0; sy < y1; ++sy) {
                for (uint32_t sx = x0; sx < x1; ++sx) {
                    const size_t p = static_cast<size_t>(sy) * baseline.width + sx;
                    sum_a += luma_at(baseline, p);
                    sum_b += luma_at(candidate, p);
                    n += 1.0;
                }
            }
            const size_t q = static_cast<size_t>(y) * target_w + x;
            a[q] = sum_a / n;
            b[q] = sum_b / n;
        }
    }

    const double c1 = 6.5025;
    const double c2 = 58.5225;
    const double n = double(a.size());
    double mean_a = 0.0;
    double mean_b = 0.0;
    for (size_t i = 0; i < a.size(); ++i) {
        mean_a += a[i];
        mean_b += b[i];
    }
    mean_a /= n;
    mean_b /= n;
    double var_a = 0.0;
    double var_b = 0.0;
    double cov = 0.0;
    for (size_t i = 0; i < a.size(); ++i) {
        const double da = a[i] - mean_a;
        const double db = b[i] - mean_b;
        var_a += da * da;
        var_b += db * db;
        cov += da * db;
    }
    const double denom = std::max(1.0, n - 1.0);
    var_a /= denom;
    var_b /= denom;
    cov /= denom;
    const double numerator = (2.0 * mean_a * mean_b + c1) * (2.0 * cov + c2);
    const double denominator = (mean_a * mean_a + mean_b * mean_b + c1) * (var_a + var_b + c2);
    return denominator > 0.0 ? std::clamp(numerator / denominator, -1.0, 1.0) : 1.0;
}

} // namespace

@interface FPTPreviewController : NSObject <NSWindowDelegate> {
@public
    std::vector<FptRenderConfig> _sceneConfigs;
    std::vector<std::string> _stitchArchivePaths;
    std::string _shaderSource;
    uint32_t _sceneIndex;
    uint64_t _jitGeneration;
    CVDisplayLinkRef _displayLink;
    std::atomic_bool _displayTickPending;
}
@property(nonatomic) struct FptRenderConfig config;
@property(nonatomic, strong) id<MTLDevice> device;
@property(nonatomic, strong) id<MTLCommandQueue> queue;
@property(nonatomic, strong) id<MTLLibrary> fallbackLibrary;
@property(nonatomic, strong) id<MTLLibrary> stitchHostLibrary;
@property(nonatomic, strong) id<MTLComputePipelineState> fallbackPipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> pipeline;
@property(nonatomic, strong) NSCache<NSData *, id<MTLComputePipelineState>> *sceneJitPipelineCache;
@property(nonatomic, strong) NSCache<NSData *, NSString *> *sceneJitLabelCache;
@property(nonatomic, strong) id<MTLComputePipelineState> presentPipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> profilePipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> voxelBuildPipeline;
@property(nonatomic, strong) id<MTLComputePipelineState> voxelFocusPipeline;
@property(nonatomic, strong) id<MTLBuffer> outBuffer;
@property(nonatomic, strong) id<MTLBuffer> accumBuffer;
@property(nonatomic, strong) id<MTLBuffer> profileBuffer;
@property(nonatomic, strong) id<MTLBuffer> voxelBuffer;
@property(nonatomic, strong) id<MTLBuffer> voxelPageTableBuffer;
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) NSImageView *imageView;
@property(nonatomic, strong) NSTextField *hudLabel;
@property(nonatomic) uint32_t frameIndex;
@property(nonatomic) uint32_t profileFrameCounter;
@property(nonatomic) BOOL needsRender;
@property(nonatomic) BOOL accumulationComplete;
@property(nonatomic, copy) NSString *sdfWorkBreakdown;
@property(nonatomic, copy) NSString *jitStatus;
@property(nonatomic) double jitBuildMs;
@property(nonatomic, strong) dispatch_queue_t jitQueue;
@property(nonatomic) double voxelBuildMs;
@property(nonatomic) uint32_t voxelActiveBricks;
@property(nonatomic) uint64_t voxelResidentBytes;
@property(nonatomic) BOOL moveForward;
@property(nonatomic) BOOL moveBackward;
@property(nonatomic) BOOL moveLeft;
@property(nonatomic) BOOL moveRight;
@property(nonatomic) BOOL moveDown;
@property(nonatomic) BOOL moveUp;
@property(nonatomic) BOOL fastMovement;
@property(nonatomic, strong) NSDate *lastMotionTime;
@property(nonatomic) BOOL refinementInvalidated;
@property(nonatomic) uint32_t refinementPassIndex;
@property(nonatomic) uint32_t interactiveSpatialStride;
@property(nonatomic) float interactiveIterationScale;
@property(nonatomic) double interactiveTargetGpuMs;
@property(nonatomic) double lastInteractivePreviewGpuMs;
@property(nonatomic) double lastRefinementGpuMs;
@property(nonatomic) double refinementAccumulatedGpuMs;
- (instancetype)initWithConfig:(const struct FptRenderConfig *)config
                  sceneConfigs:(const struct FptRenderConfig *)sceneConfigs
                    sceneCount:(uint32_t)sceneCount
                    sceneIndex:(uint32_t)sceneIndex
                      metallib:(NSString *)metallib
               stitchMetallib:(NSString *)stitchMetallib
            stitchArchivePaths:(const char *const *)stitchArchivePaths
                  shaderSource:(const char *)shaderSource
            shaderSourceLength:(size_t)shaderSourceLength
                         error:(NSError **)error;
- (void)run;
- (BOOL)setMovementKey:(unichar)key down:(BOOL)down;
- (void)handleSpecialKey:(NSEvent *)event;
- (void)mouseLookWithDeltaX:(CGFloat)deltaX deltaY:(CGFloat)deltaY;
- (void)switchSceneByOffset:(int)offset;
- (void)displayLinkTick;
- (void)renderFrame;
- (BOOL)rebuildVoxelField;
- (void)requestProceduralPipeline;
- (void)updateSdfProfileWithConfigBuffer:(id<MTLBuffer>)cfgBuffer
                           sampleGpuTime:(double)sampleGpuMs
                                    frame:(uint32_t)frame
                                   groups:(MTLSize)groups
                          threadsPerGroup:(MTLSize)threadsPerGroup;
@end

static CVReturn fpt_sdf_display_link_callback(CVDisplayLinkRef displayLink,
                                               const CVTimeStamp *now,
                                               const CVTimeStamp *outputTime,
                                               CVOptionFlags flagsIn,
                                               CVOptionFlags *flagsOut,
                                               void *context) {
    (void)displayLink;
    (void)now;
    (void)outputTime;
    (void)flagsIn;
    (void)flagsOut;
    FPTPreviewController *controller = (__bridge FPTPreviewController *)context;
    bool expected = false;
    if (!controller->_displayTickPending.compare_exchange_strong(expected, true)) return kCVReturnSuccess;
    dispatch_async(dispatch_get_main_queue(), ^{
        [controller displayLinkTick];
    });
    return kCVReturnSuccess;
}

@interface FPTPreviewView : NSImageView
@property(nonatomic, weak) FPTPreviewController *controller;
@end

@implementation FPTPreviewView
- (BOOL)acceptsFirstResponder {
    return YES;
}

- (BOOL)acceptsFirstMouse:(NSEvent *)event {
    (void)event;
    return YES;
}

- (void)keyDown:(NSEvent *)event {
    NSString *chars = [event charactersIgnoringModifiers];
    if ([chars length] == 1 && [self.controller setMovementKey:[chars characterAtIndex:0] down:YES]) {
        return;
    }
    [self.controller handleSpecialKey:event];
}

- (void)keyUp:(NSEvent *)event {
    NSString *chars = [event charactersIgnoringModifiers];
    if ([chars length] == 1) {
        [self.controller setMovementKey:[chars characterAtIndex:0] down:NO];
    }
}

- (void)mouseDown:(NSEvent *)event {
    (void)event;
    [[self window] makeFirstResponder:self];
}

- (void)rightMouseDown:(NSEvent *)event {
    (void)event;
    [[self window] makeFirstResponder:self];
}

- (void)otherMouseDown:(NSEvent *)event {
    (void)event;
    [[self window] makeFirstResponder:self];
}

- (void)mouseDragged:(NSEvent *)event {
    [self.controller mouseLookWithDeltaX:event.deltaX deltaY:event.deltaY];
}

- (void)rightMouseDragged:(NSEvent *)event {
    [self.controller mouseLookWithDeltaX:event.deltaX deltaY:event.deltaY];
}

- (void)otherMouseDragged:(NSEvent *)event {
    [self.controller mouseLookWithDeltaX:event.deltaX deltaY:event.deltaY];
}
@end

@implementation FPTPreviewController
- (instancetype)initWithConfig:(const struct FptRenderConfig *)config
                  sceneConfigs:(const struct FptRenderConfig *)sceneConfigs
                    sceneCount:(uint32_t)sceneCount
                    sceneIndex:(uint32_t)sceneIndex
                      metallib:(NSString *)metallib
               stitchMetallib:(NSString *)stitchMetallib
            stitchArchivePaths:(const char *const *)stitchArchivePaths
                  shaderSource:(const char *)shaderSource
            shaderSourceLength:(size_t)shaderSourceLength
                         error:(NSError **)error {
    self = [super init];
    if (!self) return nil;
    _displayLink = nullptr;
    _displayTickPending.store(false);
    _jitGeneration = 0u;
    _jitQueue = dispatch_queue_create("com.fpt-metal.procedural-jit",
                                      DISPATCH_QUEUE_SERIAL);
    _needsRender = YES;
    _accumulationComplete = NO;
    _refinementInvalidated = YES;
    _refinementPassIndex = 0u;
    _interactiveSpatialStride = 2u;
    _interactiveIterationScale = 0.5f;
    _interactiveTargetGpuMs = 16.67;
    if (const char *target = std::getenv("FPT_MANDEL_INTERACTIVE_TARGET_MS")) {
        char *end = nullptr;
        const double parsed = std::strtod(target, &end);
        if (end != target && std::isfinite(parsed) && parsed >= 4.0 && parsed <= 100.0) {
            _interactiveTargetGpuMs = parsed;
        }
    }
    _lastInteractivePreviewGpuMs = 0.0;
    _lastRefinementGpuMs = 0.0;
    _refinementAccumulatedGpuMs = 0.0;
    _config = *config;
    if (shaderSource && shaderSourceLength > 0u) {
        _shaderSource.assign(shaderSource, shaderSourceLength);
    }
    if (sceneConfigs && sceneCount > 0u) {
        _sceneConfigs.assign(sceneConfigs, sceneConfigs + sceneCount);
        _stitchArchivePaths.reserve(sceneCount);
        for (uint32_t index = 0u; index < sceneCount; ++index) {
            const char *path = stitchArchivePaths ? stitchArchivePaths[index] : nullptr;
            _stitchArchivePaths.emplace_back(path ? path : "");
        }
        char hdriError[512] = {};
        for (FptRenderConfig &sceneConfig : _sceneConfigs) {
            if (!hydrate_hdri_config(sceneConfig, hdriError, sizeof(hdriError))) {
                if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:9 userInfo:@{NSLocalizedDescriptionKey: ns_string(hdriError)}];
                return nil;
            }
        }
        _sceneIndex = std::min(sceneIndex, sceneCount - 1u);
        _config = _sceneConfigs[_sceneIndex];
    } else {
        char hdriError[512] = {};
        if (!hydrate_hdri_config(_config, hdriError, sizeof(hdriError))) {
            if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:9 userInfo:@{NSLocalizedDescriptionKey: ns_string(hdriError)}];
            return nil;
        }
        _sceneConfigs.push_back(_config);
        const char *path = stitchArchivePaths ? stitchArchivePaths[0] : nullptr;
        _stitchArchivePaths.emplace_back(path ? path : "");
        _sceneIndex = 0u;
    }
    _device = MTLCreateSystemDefaultDevice();
    if (!_device) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:1 userInfo:@{NSLocalizedDescriptionKey: @"Metal is unavailable"}];
        return nil;
    }
    id<MTLLibrary> library = [_device newLibraryWithURL:[NSURL fileURLWithPath:metallib] error:error];
    if (!library) return nil;
    _fallbackLibrary = library;
    const bool useVoxels = _config.renderer_backend == FPT_RENDERER_VOXEL;
    NSString *functionName = useVoxels
        ? (_config.preview ? @"voxel_preview_linear_kernel" : @"voxel_accumulate_kernel")
        : (_config.preview ? @"preview_linear_kernel" : @"accumulate_kernel");
    id<MTLFunction> function = [library newFunctionWithName:functionName];
    if (!function) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:2 userInfo:@{NSLocalizedDescriptionKey: @"preview compute kernel not found"}];
        return nil;
    }
    _fallbackPipeline = [_device newComputePipelineStateWithFunction:function error:error];
    if (!_fallbackPipeline) return nil;
    _pipeline = _fallbackPipeline;
    _sceneJitPipelineCache = [NSCache new];
    _sceneJitPipelineCache.countLimit = 32u;
    _sceneJitLabelCache = [NSCache new];
    _sceneJitLabelCache.countLimit = 32u;
    if (_config.sdf_function_stitching == 1u ||
        _config.sdf_function_stitching == 2u) {
        _stitchHostLibrary = [_device
            newLibraryWithURL:[NSURL fileURLWithPath:stitchMetallib]
                        error:error];
        if (!_stitchHostLibrary) return nil;
    }
    id<MTLFunction> presentFunction = [library newFunctionWithName:@"present_kernel"];
    if (!presentFunction) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:4 userInfo:@{NSLocalizedDescriptionKey: @"present_kernel not found"}];
        return nil;
    }
    _presentPipeline = [_device newComputePipelineStateWithFunction:presentFunction error:error];
    if (!_presentPipeline) return nil;
    if (useVoxels) {
        id<MTLFunction> voxelBuildFunction = [library newFunctionWithName:@"voxel_build_kernel"];
        id<MTLFunction> voxelFocusFunction = [library newFunctionWithName:@"estimate_voxel_focus_distance_kernel"];
        if (!voxelBuildFunction || !voxelFocusFunction) {
            if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:10 userInfo:@{NSLocalizedDescriptionKey: @"voxel build kernels not found"}];
            return nil;
        }
        _voxelBuildPipeline = [_device newComputePipelineStateWithFunction:voxelBuildFunction error:error];
        if (!_voxelBuildPipeline) return nil;
        _voxelFocusPipeline = [_device newComputePipelineStateWithFunction:voxelFocusFunction error:error];
        if (!_voxelFocusPipeline) return nil;
    }
    id<MTLFunction> profileFunction = !useVoxels && _config.sdf_profile != 0u ? [library newFunctionWithName:@"sdf_profile_kernel"] : nil;
    if (!useVoxels && _config.sdf_profile != 0u && !profileFunction) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:7 userInfo:@{NSLocalizedDescriptionKey: @"sdf_profile_kernel not found"}];
        return nil;
    }
    if (!useVoxels && _config.sdf_profile != 0u && profileFunction) {
        _profilePipeline = [_device newComputePipelineStateWithFunction:profileFunction error:error];
        if (!_profilePipeline) return nil;
    }
    _queue = [_device newCommandQueue];
    if (!_queue) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:3 userInfo:@{NSLocalizedDescriptionKey: @"failed to create command queue"}];
        return nil;
    }
    const size_t pixel_count = static_cast<size_t>(_config.width) * _config.height;
    _outBuffer = [_device newBufferWithLength:pixel_count * 4 options:MTLResourceStorageModeShared];
    if (!_outBuffer) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:5 userInfo:@{NSLocalizedDescriptionKey: @"failed to allocate preview output buffer"}];
        return nil;
    }
    _accumBuffer = [_device newBufferWithLength:pixel_count * sizeof(float) * 4 options:MTLResourceStorageModePrivate];
    if (!_accumBuffer) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:6 userInfo:@{NSLocalizedDescriptionKey: @"failed to allocate preview accumulation buffer"}];
        return nil;
    }
    if (_profilePipeline) {
        _profileBuffer = [_device newBufferWithLength:sizeof(FptSdfProfileCountsCpp) options:MTLResourceStorageModeShared];
        if (!_profileBuffer) {
            if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:8 userInfo:@{NSLocalizedDescriptionKey: @"failed to allocate SDF profile buffer"}];
            return nil;
        }
    }
    if (useVoxels && ![self rebuildVoxelField]) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:11 userInfo:@{NSLocalizedDescriptionKey: @"failed to build interactive voxel field"}];
        return nil;
    }
    _sdfWorkBreakdown = useVoxels
        ? @"Voxel field: persistent"
        : (_config.sdf_profile != 0u ? @"SDF est: profiling..." : @"SDF profile: disabled (--sdf-profile)");
    _jitStatus = _config.sdf_function_stitching != 0u
        ? @"Procedural JIT: queued; optimized bytecode active"
        : @"Procedural JIT: disabled";
    [self requestProceduralPipeline];
    return self;
}

- (void)requestProceduralPipeline {
    _jitGeneration += 1u;
    const uint64_t generation = _jitGeneration;
    self.pipeline = self.fallbackPipeline;
    self.jitBuildMs = 0.0;

    const FptRenderConfig requested = self.config;
    const bool automatic = requested.sdf_function_stitching == 3u;
    const bool generated = requested.sdf_topology_specialization != 0u;
    const bool stitched = requested.sdf_function_stitching == 1u ||
        requested.sdf_function_stitching == 2u;
    if (requested.renderer_backend != FPT_RENDERER_SDF ||
        (!automatic && !generated && !stitched)) {
        self.jitStatus = @"Procedural JIT: disabled; optimized bytecode active";
        return;
    }
    if (requested.sdf_id != FPT_SDF_PROGRAM ||
        requested.sdf_program_count == 0u) {
        self.jitStatus = @"Procedural JIT: unsupported scene; optimized bytecode active";
        return;
    }
    if (!self.jitQueue || ((automatic || generated) && _shaderSource.empty()) ||
        (stitched && !self.stitchHostLibrary)) {
        self.jitStatus = @"Procedural JIT: unavailable; optimized bytecode active";
        return;
    }
    NSString *functionName = requested.preview != 0u
        ? @"preview_linear_kernel" : @"accumulate_kernel";
    NSData *requestKey = automatic
        ? procedural_workload_key(requested)
        : procedural_topology_key(requested, functionName);
    id<MTLComputePipelineState> cachedPipeline =
        [self.sceneJitPipelineCache objectForKey:requestKey];
    if (cachedPipeline) {
        NSString *cachedLabel = [self.sceneJitLabelCache objectForKey:requestKey]
            ?: @"specialized";
        self.pipeline = cachedPipeline;
        self.jitStatus = [NSString stringWithFormat:
            @"Procedural JIT: context cache hit; %@ pipeline active",
            cachedLabel];
        return;
    }

    const std::string archivePath = _sceneIndex < _stitchArchivePaths.size()
        ? _stitchArchivePaths[_sceneIndex] : std::string();
    id<MTLDevice> device = self.device;
    id<MTLLibrary> stitchHostLibrary = self.stitchHostLibrary;
    id<MTLComputePipelineState> fallbackPipeline = self.fallbackPipeline;
    id<MTLCommandQueue> commandQueue = self.queue;
    const std::string shaderSource = _shaderSource;
    dispatch_queue_t jitQueue = self.jitQueue;
    __weak FPTPreviewController *weakSelf = self;
    self.jitStatus = @"Procedural JIT: compiling; optimized bytecode active";

    dispatch_async(jitQueue, ^{
        @autoreleasepool {
            NSDate *start = [NSDate date];
            std::string failure;
            NSError *pipelineError = nil;
            id<MTLComputePipelineState> pipeline = nil;
            NSString *selectedBackend = nil;
            bool cacheHit = false;
            bool populateArchive = false;

            if (automatic || generated) {
                FptRenderConfig distanceConfig = generated_topology_config(
                    requested, false);
                FptRenderConfig surfaceConfig = generated_topology_config(
                    requested, true);
                id<MTLComputePipelineState> distancePipeline = nil;
                id<MTLComputePipelineState> surfacePipeline = nil;
                bool distanceCacheHit = false;
                bool surfaceCacheHit = false;
                if (generated) {
                    const bool analyticSurface =
                        requested.sdf_stitched_surface != 0u;
                    const bool built = build_generated_topology_pipeline(
                        device, shaderSource,
                        analyticSurface ? surfaceConfig : distanceConfig,
                        functionName, &pipeline, &cacheHit, failure);
                    if (built) {
                        selectedBackend = analyticSurface
                            ? @"generated surface" : @"generated distance";
                    }
                } else {
                    const bool distanceBuilt = build_generated_topology_pipeline(
                        device, shaderSource, distanceConfig, functionName,
                        &distancePipeline, &distanceCacheHit, failure);
                    const bool surfaceBuilt = distanceBuilt &&
                        build_generated_topology_pipeline(
                            device, shaderSource, surfaceConfig, functionName,
                            &surfacePipeline, &surfaceCacheHit, failure);
                    cacheHit = distanceCacheHit && surfaceCacheHit;
                    if (distanceBuilt && surfaceBuilt && requested.preview != 0u) {
                    FptRenderConfig probeConfig = requested;
                    probeConfig.preview = 1u;
                    probeConfig.width = std::min<uint32_t>(requested.width, 1920u);
                    probeConfig.height = std::max<uint32_t>(1u,
                        static_cast<uint32_t>(
                            static_cast<uint64_t>(requested.height) *
                            probeConfig.width /
                            std::max<uint32_t>(requested.width, 1u)));
                    distanceConfig.width = probeConfig.width;
                    distanceConfig.height = probeConfig.height;
                    distanceConfig.preview = 1u;
                    surfaceConfig.width = probeConfig.width;
                    surfaceConfig.height = probeConfig.height;
                    surfaceConfig.preview = 1u;
                    const size_t outputBytes = static_cast<size_t>(
                        probeConfig.width) * probeConfig.height * 4u * sizeof(float);
                    id<MTLBuffer> directOutput = [device
                        newBufferWithLength:outputBytes
                                     options:MTLResourceStorageModeShared];
                    id<MTLBuffer> distanceOutput = [device
                        newBufferWithLength:outputBytes
                                     options:MTLResourceStorageModeShared];
                    id<MTLBuffer> surfaceOutput = [device
                        newBufferWithLength:outputBytes
                                     options:MTLResourceStorageModeShared];
                    id<MTLBuffer> directConfig = [device
                        newBufferWithBytes:&probeConfig
                                    length:sizeof(probeConfig)
                                   options:MTLResourceStorageModeShared];
                    id<MTLBuffer> distanceConfigBuffer = [device
                        newBufferWithBytes:&distanceConfig
                                    length:sizeof(distanceConfig)
                                   options:MTLResourceStorageModeShared];
                    id<MTLBuffer> surfaceConfigBuffer = [device
                        newBufferWithBytes:&surfaceConfig
                                    length:sizeof(surfaceConfig)
                                   options:MTLResourceStorageModeShared];
                    if (!directOutput || !distanceOutput || !surfaceOutput ||
                        !directConfig || !distanceConfigBuffer ||
                        !surfaceConfigBuffer) {
                        failure = "failed to allocate automatic JIT probe buffers";
                    } else {
                        std::vector<double> directTimes;
                        std::vector<double> distanceTimes;
                        std::vector<double> surfaceTimes;
                        bool probePassed = true;
                        auto renderBackend = [&](uint32_t backend, bool record) {
                            double elapsed = 0.0;
                            id<MTLComputePipelineState> backendPipeline = backend == 0u
                                ? fallbackPipeline
                                : (backend == 1u ? distancePipeline : surfacePipeline);
                            id<MTLBuffer> backendOutput = backend == 0u
                                ? directOutput
                                : (backend == 1u ? distanceOutput : surfaceOutput);
                            id<MTLBuffer> backendConfig = backend == 0u
                                ? directConfig
                                : (backend == 1u ? distanceConfigBuffer : surfaceConfigBuffer);
                            probePassed = render_preview_buffer(
                                commandQueue, backendPipeline, backendOutput,
                                backendConfig, probeConfig.width, probeConfig.height,
                                &elapsed, failure);
                            if (probePassed && record) {
                                (backend == 0u ? directTimes
                                    : (backend == 1u ? distanceTimes : surfaceTimes))
                                    .push_back(elapsed);
                            }
                        };
                        for (uint32_t backend = 0u;
                             backend < 3u && probePassed; ++backend) {
                            renderBackend(backend, false);
                        }
                        const uint32_t latinOrder[3][3] = {
                            {0u, 1u, 2u}, {2u, 0u, 1u}, {1u, 2u, 0u}};
                        for (uint32_t run = 0u; run < 3u && probePassed; ++run) {
                            for (uint32_t slot = 0u; slot < 3u && probePassed; ++slot) {
                                renderBackend(latinOrder[run % 3u][slot], true);
                            }
                        }
                        const size_t components = outputBytes / sizeof(float);
                        struct PreviewParity {
                            double mean;
                            float maximum;
                            double outlier_fraction;
                        };
                        const auto parityError = [components](id<MTLBuffer> lhs,
                                                              id<MTLBuffer> rhs) {
                            const float *a = static_cast<const float *>(lhs.contents);
                            const float *b = static_cast<const float *>(rhs.contents);
                            double mean = 0.0;
                            float maximum = 0.0f;
                            size_t outliers = 0u;
                            for (size_t index = 0u; index < components; ++index) {
                                const float difference = std::abs(a[index] - b[index]);
                                if (!std::isfinite(difference)) {
                                    return PreviewParity{INFINITY, INFINITY, 1.0};
                                }
                                mean += difference;
                                maximum = std::max(maximum, difference);
                                outliers += difference > 1.0e-3f ? 1u : 0u;
                            }
                            return PreviewParity{
                                mean / std::max<size_t>(components, 1u), maximum,
                                static_cast<double>(outliers) /
                                    std::max<size_t>(components, 1u)};
                        };
                        if (probePassed) {
                            double directMs = median_time(directTimes);
                            double distanceMs = median_time(distanceTimes);
                            double surfaceMs = median_time(surfaceTimes);
                            const auto nearGate = [](double ratio, double gate) {
                                return std::abs(ratio / gate - 1.0) <= 0.05;
                            };
                            const bool adaptive =
                                nearGate(directMs / distanceMs, 1.0 / 0.85) ||
                                nearGate(directMs / surfaceMs, 1.0 / 0.85) ||
                                nearGate(std::min(directMs, distanceMs) / surfaceMs,
                                         1.10);
                            if (adaptive) {
                                for (uint32_t run = 3u;
                                     run < 7u && probePassed; ++run) {
                                    for (uint32_t slot = 0u;
                                         slot < 3u && probePassed; ++slot) {
                                        renderBackend(latinOrder[run % 3u][slot], true);
                                    }
                                }
                                directMs = median_time(directTimes);
                                distanceMs = median_time(distanceTimes);
                                surfaceMs = median_time(surfaceTimes);
                            }
                            const auto distanceParity = parityError(
                                directOutput, distanceOutput);
                            const auto surfaceParity = parityError(
                                directOutput, surfaceOutput);
                            const bool distanceQualified =
                                distanceParity.mean <= 1.0e-6 &&
                                distanceParity.outlier_fraction <= 1.0e-5 &&
                                directMs / distanceMs >= 1.0 / 0.85;
                            const bool surfaceQualified =
                                surfaceParity.mean <= 1.0e-6 &&
                                surfaceParity.outlier_fraction <= 1.0e-5 &&
                                directMs / surfaceMs >= 1.0 / 0.85;
                            if (surfaceQualified &&
                                (!distanceQualified ||
                                 distanceMs / surfaceMs >= 1.10)) {
                                pipeline = surfacePipeline;
                                selectedBackend = @"generated surface";
                            } else if (distanceQualified) {
                                pipeline = distancePipeline;
                                selectedBackend = @"generated distance";
                            } else {
                                pipeline = fallbackPipeline;
                                selectedBackend = @"direct";
                            }
                        }
                    }
                    } else if (requested.preview == 0u) {
                        failure = "automatic JIT probing requires viewport preview mode";
                    }
                }
            } else {
                id<MTLLibrary> stitchedLibrary = nil;
                id<MTLFunction> distanceFunction = nil;
                id<MTLFunction> surfaceFunction = nil;
                id<MTLBinaryArchive> binaryArchive = nil;
                bool built = build_topology_stitched_library(
                    device, stitchHostLibrary, archivePath.c_str(), requested,
                    &stitchedLibrary, &distanceFunction, &surfaceFunction,
                    &binaryArchive, &populateArchive, failure);
                if (built) {
                    id<MTLFunction> function = [stitchHostLibrary
                        newFunctionWithName:functionName];
                    NSArray<id<MTLFunction>> *privateFunctions = @[
                        distanceFunction, surfaceFunction];
                    pipeline = function ? new_compute_pipeline(
                        device, function, privateFunctions, binaryArchive,
                        populateArchive, &pipelineError) : nil;
                    selectedBackend = @"stitched";
                    if (!pipeline && !pipelineError) {
                        failure = "stitched preview kernel is missing";
                    }
                }
                if (pipeline && populateArchive && binaryArchive &&
                    !archivePath.empty()) {
                    NSURL *archiveURL = [NSURL fileURLWithPath:
                        [NSString stringWithUTF8String:archivePath.c_str()]];
                    if (![binaryArchive serializeToURL:archiveURL
                                                 error:&pipelineError]) {
                        pipeline = nil;
                    }
                }
            }
            const double buildMs = -[start timeIntervalSinceNow] * 1000.0;
            NSString *failureMessage = pipelineError
                ? pipelineError.localizedDescription
                : (failure.empty() ? @"unknown error" :
                    [NSString stringWithUTF8String:failure.c_str()]);
            dispatch_async(dispatch_get_main_queue(), ^{
                FPTPreviewController *controller = weakSelf;
                if (!controller || controller->_jitGeneration != generation) return;
                controller.jitBuildMs = buildMs;
                if (!pipeline) {
                    controller.pipeline = controller.fallbackPipeline;
                    controller.jitStatus = [NSString stringWithFormat:
                        @"Procedural JIT: failed (%@); optimized bytecode active",
                        failureMessage];
                    controller.needsRender = YES;
                    return;
                }
                controller.pipeline = pipeline;
                [controller.sceneJitPipelineCache setObject:pipeline
                                                     forKey:requestKey];
                [controller.sceneJitLabelCache
                    setObject:(selectedBackend ?: @"specialized")
                       forKey:requestKey];
                controller.frameIndex = 0u;
                controller.needsRender = YES;
                controller.accumulationComplete = NO;
                controller.jitStatus = [NSString stringWithFormat:
                    @"Procedural JIT: %@ in %.2f ms; %@ pipeline active",
                    (cacheHit || (stitched && !populateArchive))
                        ? @"cache hit" : @"compiled",
                    buildMs, selectedBackend ?: @"specialized"];
            });
        }
    });
}

- (BOOL)rebuildVoxelField {
    if (self.config.renderer_backend != FPT_RENDERER_VOXEL) {
        self.voxelBuffer = nil;
        self.voxelPageTableBuffer = nil;
        self.voxelBuildMs = 0.0;
        self.voxelActiveBricks = 0u;
        self.voxelResidentBytes = 0u;
        return YES;
    }
    if (!self.voxelBuildPipeline || !self.voxelFocusPipeline || !self.queue) return NO;
    const size_t resolution = std::clamp<size_t>(self.config.voxel_resolution, 32u, 512u);
    const size_t voxelBytes = resolution * resolution * resolution * 12u;
    id<MTLBuffer> denseVoxelBuffer = [self.device newBufferWithLength:voxelBytes
                                                              options:MTLResourceStorageModeShared];
    if (!denseVoxelBuffer) return NO;
    id<MTLBuffer> cfgBuffer = [self.device newBufferWithBytes:&_config
                                                       length:sizeof(FptRenderConfig)
                                                      options:MTLResourceStorageModeShared];
    if (!cfgBuffer) return NO;
    NSDate *buildStart = [NSDate date];
    id<MTLCommandBuffer> buildCommand = [self.queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [buildCommand computeCommandEncoder];
    [encoder setComputePipelineState:self.voxelBuildPipeline];
    [encoder setBuffer:denseVoxelBuffer offset:0 atIndex:0];
    [encoder setBuffer:cfgBuffer offset:0 atIndex:1];
    MTLSize grid = MTLSizeMake(resolution, resolution, resolution);
    [encoder dispatchThreads:grid threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
    [encoder endEncoding];
    [buildCommand commit];
    [buildCommand waitUntilCompleted];
    if (buildCommand.status == MTLCommandBufferStatusError) return NO;
    VoxelStorageResult storage = finalize_voxel_storage(self.device, self.config, denseVoxelBuffer);
    if (!storage.cells || !storage.page_table) return NO;
    self.voxelBuffer = storage.cells;
    self.voxelPageTableBuffer = storage.page_table;
    self.voxelActiveBricks = storage.active_bricks;
    self.voxelResidentBytes = storage.resident_bytes;

    if (self.config.focus_distance <= 0.0f) {
        id<MTLBuffer> focusBuffer = [self.device newBufferWithLength:sizeof(float) options:MTLResourceStorageModeShared];
        if (!focusBuffer) return NO;
        id<MTLCommandBuffer> focusCommand = [self.queue commandBuffer];
        encoder = [focusCommand computeCommandEncoder];
        [encoder setComputePipelineState:self.voxelFocusPipeline];
        [encoder setBuffer:focusBuffer offset:0 atIndex:0];
        [encoder setBuffer:cfgBuffer offset:0 atIndex:1];
        [encoder setBuffer:self.voxelBuffer offset:0 atIndex:2];
        [encoder setBuffer:self.voxelPageTableBuffer offset:0 atIndex:3];
        [encoder dispatchThreads:MTLSizeMake(1u, 1u, 1u) threadsPerThreadgroup:MTLSizeMake(1u, 1u, 1u)];
        [encoder endEncoding];
        [focusCommand commit];
        [focusCommand waitUntilCompleted];
        if (focusCommand.status == MTLCommandBufferStatusError) return NO;
        struct FptRenderConfig next = self.config;
        next.focus_distance = *static_cast<const float *>(focusBuffer.contents);
        self.config = next;
    }
    self.voxelBuildMs = -[buildStart timeIntervalSinceNow] * 1000.0;
    return YES;
}

- (void)run {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    NSRect frame = NSMakeRect(80, 80, self.config.width, self.config.height);
    self.window = [[NSWindow alloc] initWithContentRect:frame
                                              styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable)
                                                backing:NSBackingStoreBuffered
                                                  defer:NO];
    self.window.title = [NSString stringWithFormat:@"%@ - %@",
                         self.config.preview ? @"FPT Metal Preview" : @"FPT Metal Pathtrace Preview",
                         sdf_scene_name(self.config.sdf_id)];
    self.window.delegate = self;
    FPTPreviewView *previewView = [[FPTPreviewView alloc] initWithFrame:self.window.contentView.bounds];
    previewView.controller = self;
    self.imageView = previewView;
    self.imageView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    self.imageView.imageScaling = NSImageScaleAxesIndependently;
    [self.window.contentView addSubview:self.imageView];
    self.hudLabel = make_perf_hud(self.window.contentView);
    self.lastMotionTime = [NSDate date];
    if (CVDisplayLinkCreateWithActiveCGDisplays(&_displayLink) == kCVReturnSuccess && _displayLink) {
        CVDisplayLinkSetOutputCallback(_displayLink, fpt_sdf_display_link_callback, (__bridge void *)self);
        CVDisplayLinkStart(_displayLink);
    }
    [self.window makeKeyAndOrderFront:nil];
    [self.window makeFirstResponder:self.imageView];
    [NSApp activateIgnoringOtherApps:YES];
    [self renderFrame];
    [NSApp run];
}

- (void)windowWillClose:(NSNotification *)notification {
    (void)notification;
    if (_displayLink) {
        CVDisplayLinkStop(_displayLink);
        CVDisplayLinkRelease(_displayLink);
        _displayLink = nullptr;
    }
    [NSApp stop:nil];
}

- (void)displayLinkTick {
    [self renderFrame];
    _displayTickPending.store(false);
}

- (void)switchSceneByOffset:(int)offset {
    if (_sceneConfigs.empty() || offset == 0) return;
    const int count = static_cast<int>(_sceneConfigs.size());
    int next_index = static_cast<int>(_sceneIndex) + offset;
    next_index = ((next_index % count) + count) % count;
    _sceneIndex = static_cast<uint32_t>(next_index);
    struct FptRenderConfig next = _sceneConfigs[_sceneIndex];
    next.width = self.config.width;
    next.height = self.config.height;
    next.samples = self.config.samples;
    next.preview = self.config.preview;
    next.sdf_profile = self.config.sdf_profile;
    self.config = next;
    self.frameIndex = 0;
    self.needsRender = YES;
    self.accumulationComplete = NO;
    self.profileFrameCounter = 0;
    self.refinementInvalidated = YES;
    self.refinementPassIndex = 0u;
    self.interactiveSpatialStride = 2u;
    self.lastInteractivePreviewGpuMs = 0.0;
    self.lastRefinementGpuMs = 0.0;
    self.refinementAccumulatedGpuMs = 0.0;
    if (self.config.renderer_backend == FPT_RENDERER_VOXEL) {
        if (![self rebuildVoxelField]) return;
        self.sdfWorkBreakdown = @"Voxel field: persistent";
    } else {
        self.sdfWorkBreakdown = self.config.sdf_profile != 0u ? @"SDF est: profiling..." : @"SDF profile: disabled (--sdf-profile)";
    }
    [self requestProceduralPipeline];
    self.window.title = [NSString stringWithFormat:@"%@ - %@",
                         self.config.preview ? @"FPT Metal Preview" : @"FPT Metal Pathtrace Preview",
                         sdf_scene_name(self.config.sdf_id)];
}

- (BOOL)setMovementKey:(unichar)key down:(BOOL)down {
    const char c = key < 128 ? static_cast<char>(std::tolower(static_cast<unsigned char>(key))) : '\0';
    switch (c) {
        case 'w': self.moveForward = down; break;
        case 's': self.moveBackward = down; break;
        case 'a': self.moveLeft = down; break;
        case 'd': self.moveRight = down; break;
        case 'q': self.moveDown = down; break;
        case 'e':
        case ' ': self.moveUp = down; break;
        case 'n': if (down) [self switchSceneByOffset:1]; break;
        case 'p': if (down) [self switchSceneByOffset:-1]; break;
        case ']': if (down) [self switchSceneByOffset:1]; break;
        case '[': if (down) [self switchSceneByOffset:-1]; break;
        default: return NO;
    }
    self.needsRender = YES;
    if (down) self.refinementInvalidated = YES;
    return YES;
}

- (void)handleSpecialKey:(NSEvent *)event {
    struct FptRenderConfig next = self.config;
    const float turn = (event.modifierFlags & NSEventModifierFlagShift) ? 0.055f : 0.025f;
    switch (event.keyCode) {
        case 123: next.camera_yaw_pitch[0] -= turn; break;
        case 124: next.camera_yaw_pitch[0] += turn; break;
        case 125: next.camera_yaw_pitch[1] -= turn; break;
        case 126: next.camera_yaw_pitch[1] += turn; break;
        case 53: [self.window close]; return;
        default: return;
    }
    next.camera_yaw_pitch[1] = std::clamp(next.camera_yaw_pitch[1], -1.52f, 1.52f);
    self.config = next;
    self.frameIndex = 0;
    self.needsRender = YES;
    self.accumulationComplete = NO;
    self.refinementInvalidated = YES;
}

- (void)mouseLookWithDeltaX:(CGFloat)deltaX deltaY:(CGFloat)deltaY {
    struct FptRenderConfig next = self.config;
    constexpr float sensitivity = 0.0035f;
    next.camera_yaw_pitch[0] += static_cast<float>(deltaX) * sensitivity;
    next.camera_yaw_pitch[1] -= static_cast<float>(deltaY) * sensitivity;
    next.camera_yaw_pitch[1] = std::clamp(next.camera_yaw_pitch[1], -1.52f, 1.52f);
    self.config = next;
    self.frameIndex = 0;
    self.needsRender = YES;
    self.accumulationComplete = NO;
    self.refinementInvalidated = YES;
}

- (void)updateInteractiveMotion {
    NSDate *now = [NSDate date];
    const double dt = std::clamp([now timeIntervalSinceDate:(self.lastMotionTime ?: now)], 0.0, 0.05);
    self.lastMotionTime = now;
    if (dt <= 0.0) return;

    self.fastMovement = ([NSEvent modifierFlags] & NSEventModifierFlagShift) != 0;
    const float yaw = self.config.camera_yaw_pitch[0];
    const float pitch = self.config.camera_yaw_pitch[1];
    const float cos_pitch = std::cos(pitch);
    const float forward[3] = {
        std::sin(yaw) * cos_pitch,
        std::sin(pitch),
        std::cos(yaw) * cos_pitch,
    };
    const float right[3] = {
        std::cos(yaw),
        0.0f,
        -std::sin(yaw),
    };

    float move[3] = {};
    if (self.moveForward) {
        move[0] += forward[0]; move[1] += forward[1]; move[2] += forward[2];
    }
    if (self.moveBackward) {
        move[0] -= forward[0]; move[1] -= forward[1]; move[2] -= forward[2];
    }
    if (self.moveRight) {
        move[0] += right[0]; move[2] += right[2];
    }
    if (self.moveLeft) {
        move[0] -= right[0]; move[2] -= right[2];
    }
    if (self.moveUp) move[1] += 1.0f;
    if (self.moveDown) move[1] -= 1.0f;

    const float len = std::sqrt(move[0] * move[0] + move[1] * move[1] + move[2] * move[2]);
    if (len <= 0.0f) return;
    const float base_speed = 4.0f;
    const float speed = base_speed * (self.fastMovement ? 4.0f : 1.0f) * static_cast<float>(dt);
    struct FptRenderConfig next = self.config;
    next.camera_position[0] += (move[0] / len) * speed;
    next.camera_position[1] += (move[1] / len) * speed;
    next.camera_position[2] += (move[2] / len) * speed;
    self.config = next;
    self.frameIndex = 0;
    self.needsRender = YES;
    self.accumulationComplete = NO;
    self.refinementInvalidated = YES;
}

- (void)updateSdfProfileWithConfigBuffer:(id<MTLBuffer>)cfgBuffer
                           sampleGpuTime:(double)sampleGpuMs
                                    frame:(uint32_t)frame
                                   groups:(MTLSize)groups
                          threadsPerGroup:(MTLSize)threadsPerGroup {
    if (self.config.preview || self.config.sdf_profile == 0u || !self.profilePipeline || !self.profileBuffer || !cfgBuffer || sampleGpuMs <= 0.0) return;
    self.profileFrameCounter += 1u;
    if ((self.profileFrameCounter % 30u) != 1u) return;
    std::memset(self.profileBuffer.contents, 0, sizeof(FptSdfProfileCountsCpp));
    FptSdfProfileConfigCpp profile = {frame, 4u, 0u, 0u};
    id<MTLCommandBuffer> commandBuffer = [self.queue commandBuffer];
    id<MTLComputeCommandEncoder> encoder = [commandBuffer computeCommandEncoder];
    [encoder setComputePipelineState:self.profilePipeline];
    [encoder setBuffer:self.profileBuffer offset:0 atIndex:0];
    [encoder setBuffer:cfgBuffer offset:0 atIndex:1];
    [encoder setBytes:&profile length:sizeof(profile) atIndex:2];
    [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threadsPerGroup];
    [encoder endEncoding];
    [commandBuffer commit];
    [commandBuffer waitUntilCompleted];
    if (commandBuffer.status == MTLCommandBufferStatusError) return;
    FptSdfProfileCountsCpp counts = {};
    std::memcpy(&counts, self.profileBuffer.contents, sizeof(counts));
    self.sdfWorkBreakdown = format_sdf_work_breakdown(counts, sampleGpuMs, self.config);
}

- (void)renderFrame {
    if (!self.window.visible) return;
    const BOOL moving = self.moveForward || self.moveBackward || self.moveLeft || self.moveRight || self.moveDown || self.moveUp;
    const BOOL refinement_enabled = self.config.preview != 0u &&
        self.config.sdf_id == FPT_SDF_MANDELBULBER &&
        std::getenv("FPT_MANDEL_INTERACTIVE_REFINEMENT") != nullptr;
    const uint32_t refinement_pass_count =
        self.interactiveSpatialStride * self.interactiveSpatialStride;
    const BOOL refinement_pending = refinement_enabled &&
        (self.refinementInvalidated ||
         self.refinementPassIndex < refinement_pass_count);
    if (self.config.preview && !self.needsRender && !moving && !refinement_pending) return;
    if (!self.config.preview && self.accumulationComplete && !self.needsRender && !moving) return;
    self.needsRender = NO;
    NSDate *frameStart = [NSDate date];
    [self updateInteractiveMotion];
    FptRenderConfig render_config = self.config;
    BOOL rendered_reduced_preview = NO;
    BOOL rendered_refinement_pass = NO;
    if (refinement_enabled) {
        // Moving frames trace one sample per spatial block and replicate it
        // across that block. Stationary frames visit each lattice offset once
        // at exact quality; together the passes replace every preview value.
        if (moving || self.refinementInvalidated) {
            render_config.vset_values[131] = -self.interactiveIterationScale;
            render_config.vset_values[132] = -float(self.interactiveSpatialStride);
            rendered_reduced_preview = YES;
            self.refinementPassIndex = 0u;
            self.refinementAccumulatedGpuMs = 0.0;
            self.refinementInvalidated = NO;
            self.needsRender = YES;
        } else if (self.refinementPassIndex < refinement_pass_count) {
            const uint32_t spatial_code =
                (self.interactiveSpatialStride << 16u) |
                (self.refinementPassIndex + 1u);
            render_config.vset_values[131] = 0.0f;
            render_config.vset_values[132] = float(spatial_code);
            rendered_refinement_pass = YES;
            self.refinementPassIndex += 1u;
            self.needsRender = self.refinementPassIndex < refinement_pass_count;
        }
    }
    const size_t pixel_count = static_cast<size_t>(self.config.width) * self.config.height;
    id<MTLBuffer> cfg_buffer = [self.device newBufferWithBytes:&render_config length:sizeof(FptRenderConfig) options:MTLResourceStorageModeShared];
    if (!self.outBuffer || !cfg_buffer) return;
    MTLSize threads_per_group = threadgroup_for_pipeline(self.pipeline);
    uint32_t dispatch_width = self.config.width;
    uint32_t dispatch_height = self.config.height;
    interactive_preview_dispatch_extent(render_config, dispatch_width, dispatch_height);
    MTLSize groups = groups_for_extent(dispatch_width, dispatch_height, threads_per_group);
    double render_gpu_ms = 0.0;
    double present_gpu_ms = 0.0;
    double encode_ms = 0.0;
    double wait_ms = 0.0;
    NSDate *encodeStart = [NSDate date];
    if (self.config.preview) {
        id<MTLCommandBuffer> command_buffer = [self.queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command_buffer computeCommandEncoder];
        [encoder setComputePipelineState:self.pipeline];
        [encoder setBuffer:self.accumBuffer offset:0 atIndex:0];
        [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
        if (self.config.renderer_backend == FPT_RENDERER_VOXEL) [encoder setBuffer:self.voxelBuffer offset:0 atIndex:3];
        if (self.config.renderer_backend == FPT_RENDERER_VOXEL) [encoder setBuffer:self.voxelPageTableBuffer offset:0 atIndex:4];
        [encoder dispatchThreadgroups:groups
                 threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        encode_ms = -[encodeStart timeIntervalSinceNow] * 1000.0;
        NSDate *waitStart = [NSDate date];
        [command_buffer commit];
        [command_buffer waitUntilCompleted];
        wait_ms += -[waitStart timeIntervalSinceNow] * 1000.0;
        if (command_buffer.status == MTLCommandBufferStatusError) return;
        render_gpu_ms = command_buffer_gpu_ms(command_buffer);

        encodeStart = [NSDate date];
        id<MTLCommandBuffer> present_command_buffer = [self.queue commandBuffer];
        encoder = [present_command_buffer computeCommandEncoder];
        FptRenderConfig present_config = render_config;
        id<MTLBuffer> present_config_buffer = cfg_buffer;
        MTLSize present_groups = groups;
        if (refinement_enabled &&
            (rendered_reduced_preview || rendered_refinement_pass)) {
            // Spatial preview/refinement dispatches cover a compact sample
            // grid, but presentation always reads the complete persistent
            // buffer. This also refreshes post-process neighbourhoods as exact
            // lattice samples replace replicated preview values.
            present_config.vset_values[131] = 0.0f;
            present_config.vset_values[132] = 0.0f;
            present_config_buffer = [self.device
                newBufferWithBytes:&present_config
                            length:sizeof(present_config)
                           options:MTLResourceStorageModeShared];
            if (!present_config_buffer) return;
            present_groups = groups_for_extent(
                self.config.width, self.config.height, threads_per_group);
        }
        [encoder setComputePipelineState:self.presentPipeline];
        [encoder setBuffer:self.outBuffer offset:0 atIndex:0];
        [encoder setBuffer:self.accumBuffer offset:0 atIndex:1];
        [encoder setBuffer:present_config_buffer offset:0 atIndex:2];
        [encoder dispatchThreadgroups:present_groups
                 threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        encode_ms += -[encodeStart timeIntervalSinceNow] * 1000.0;
        waitStart = [NSDate date];
        [present_command_buffer commit];
        [present_command_buffer waitUntilCompleted];
        wait_ms += -[waitStart timeIntervalSinceNow] * 1000.0;
        if (present_command_buffer.status == MTLCommandBufferStatusError) return;
        present_gpu_ms = command_buffer_gpu_ms(present_command_buffer);
    } else {
        uint32_t frame = self.frameIndex;
        id<MTLCommandBuffer> sample_command_buffer = [self.queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [sample_command_buffer computeCommandEncoder];
        [encoder setComputePipelineState:self.pipeline];
        [encoder setBuffer:self.accumBuffer offset:0 atIndex:0];
        [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
        [encoder setBytes:&frame length:sizeof(frame) atIndex:2];
        if (self.config.renderer_backend == FPT_RENDERER_VOXEL) [encoder setBuffer:self.voxelBuffer offset:0 atIndex:3];
        if (self.config.renderer_backend == FPT_RENDERER_VOXEL) [encoder setBuffer:self.voxelPageTableBuffer offset:0 atIndex:4];
        [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        encode_ms += -[encodeStart timeIntervalSinceNow] * 1000.0;
        NSDate *waitStart = [NSDate date];
        [sample_command_buffer commit];
        [sample_command_buffer waitUntilCompleted];
        wait_ms += -[waitStart timeIntervalSinceNow] * 1000.0;
        if (sample_command_buffer.status == MTLCommandBufferStatusError) return;
        render_gpu_ms = command_buffer_gpu_ms(sample_command_buffer);

        encodeStart = [NSDate date];
        id<MTLCommandBuffer> present_command_buffer = [self.queue commandBuffer];
        encoder = [present_command_buffer computeCommandEncoder];
        [encoder setComputePipelineState:self.presentPipeline];
        [encoder setBuffer:self.outBuffer offset:0 atIndex:0];
        [encoder setBuffer:self.accumBuffer offset:0 atIndex:1];
        [encoder setBuffer:cfg_buffer offset:0 atIndex:2];
        [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        encode_ms += -[encodeStart timeIntervalSinceNow] * 1000.0;
        waitStart = [NSDate date];
        [present_command_buffer commit];
        [present_command_buffer waitUntilCompleted];
        wait_ms += -[waitStart timeIntervalSinceNow] * 1000.0;
        if (present_command_buffer.status == MTLCommandBufferStatusError) return;
        present_gpu_ms = command_buffer_gpu_ms(present_command_buffer);
    }
    const double gpu_ms = render_gpu_ms + present_gpu_ms;
    if (refinement_enabled && rendered_reduced_preview) {
        self.lastInteractivePreviewGpuMs = render_gpu_ms;
        if (render_gpu_ms > self.interactiveTargetGpuMs * 1.15) {
            if (self.interactiveSpatialStride < 16u) {
                self.interactiveSpatialStride *= 2u;
            } else if (self.interactiveIterationScale > 0.25f) {
                self.interactiveIterationScale = std::max(
                    0.25f, self.interactiveIterationScale - 0.25f);
            }
        } else if (render_gpu_ms < self.interactiveTargetGpuMs * 0.65) {
            if (self.interactiveSpatialStride > 2u) {
                self.interactiveSpatialStride /= 2u;
            } else if (self.interactiveIterationScale < 1.0f) {
                self.interactiveIterationScale = std::min(
                    1.0f, self.interactiveIterationScale + 0.25f);
            }
        }
    } else if (refinement_enabled && rendered_refinement_pass) {
        self.lastRefinementGpuMs = render_gpu_ms;
        self.refinementAccumulatedGpuMs += render_gpu_ms;
        if (render_gpu_ms > self.interactiveTargetGpuMs * 1.15 &&
            self.interactiveSpatialStride < 16u) {
            self.interactiveSpatialStride *= 2u;
            self.refinementPassIndex = 0u;
            self.needsRender = YES;
        } else if (render_gpu_ms < self.interactiveTargetGpuMs * 0.40 &&
                   self.interactiveSpatialStride > 2u) {
            self.interactiveSpatialStride /= 2u;
            self.refinementPassIndex = 0u;
            self.needsRender = YES;
        }
    }
    const uint32_t rendered_frame = self.frameIndex;
    if (!self.config.preview) {
        if (self.frameIndex + 1u < std::max<uint32_t>(self.config.samples, 1u)) {
            self.frameIndex += 1u;
        } else {
            self.accumulationComplete = YES;
        }
    }
    if (!self.config.preview) {
        self.window.title = [NSString stringWithFormat:@"FPT Metal Pathtrace Preview - %@ - sample %u/%u",
                             sdf_scene_name(self.config.sdf_id),
                             self.frameIndex + 1u,
                             std::max<uint32_t>(self.config.samples, 1u)];
    }

    NSDate *imageStart = [NSDate date];
    NSData *data = [NSData dataWithBytes:self.outBuffer.contents length:pixel_count * 4];
    CGDataProviderRef provider = CGDataProviderCreateWithCFData((__bridge CFDataRef)data);
    CGColorSpaceRef color_space = CGColorSpaceCreateDeviceRGB();
    CGImageRef image = CGImageCreate(self.config.width,
                                     self.config.height,
                                     8,
                                     32,
                                     static_cast<size_t>(self.config.width) * 4,
                                     color_space,
                                     kCGImageAlphaLast | kCGBitmapByteOrder32Big,
                                     provider,
                                     nullptr,
                                     false,
                                     kCGRenderingIntentDefault);
    if (image) {
        self.imageView.image = [[NSImage alloc] initWithCGImage:image size:NSMakeSize(self.config.width, self.config.height)];
        CGImageRelease(image);
    }
    CGColorSpaceRelease(color_space);
    CGDataProviderRelease(provider);
    const double image_ms = -[imageStart timeIntervalSinceNow] * 1000.0;
    const double cpu_ms = -[frameStart timeIntervalSinceNow] * 1000.0;
    NSString *renderer = [NSString stringWithFormat:@"%@ %@ | %@",
                          self.config.renderer_backend == FPT_RENDERER_VOXEL ? @"Voxel" : @"SDF",
                          self.config.preview ? @"viewport" : @"pathtrace",
                          sdf_scene_name(self.config.sdf_id)];
    NSString *breakdown = self.config.preview
        ? format_stage_ms(@"render", render_gpu_ms, gpu_ms)
        : [NSString stringWithFormat:@"%@, %@",
           format_stage_ms(@"sample", render_gpu_ms, gpu_ms),
           format_stage_ms(@"present", present_gpu_ms, gpu_ms)];
    NSString *detail = self.config.renderer_backend == FPT_RENDERER_VOXEL
        ? [NSString stringWithFormat:@"Voxel field: %u^3 | %.1f MB | %u active bricks | build %.2f ms | reused",
           self.config.voxel_resolution,
           double(self.voxelResidentBytes) / (1024.0 * 1024.0),
           self.voxelActiveBricks,
           self.voxelBuildMs]
        : (self.config.preview
        ? [NSString stringWithFormat:@"Bottleneck: %@", sdf_bottleneck_label(self.config)]
        : (self.config.sdf_profile != 0u
            ? [NSString stringWithFormat:@"%@\nBottleneck: %@", self.sdfWorkBreakdown ?: @"SDF est: profiling...", sdf_bottleneck_label(self.config)]
            : [NSString stringWithFormat:@"Bottleneck: %@", sdf_bottleneck_label(self.config)]));
    if (self.config.renderer_backend != FPT_RENDERER_VOXEL && self.jitStatus) {
        detail = [NSString stringWithFormat:@"%@\n%@", self.jitStatus, detail];
    }
    if (refinement_enabled) {
        const uint32_t hud_refinement_pass_count =
            self.interactiveSpatialStride * self.interactiveSpatialStride;
        NSString *interactive_status = nil;
        if (rendered_reduced_preview) {
            interactive_status = [NSString stringWithFormat:
                @"Interactive: preview %.0f%% | stride %ux%u | target %.1f ms | render %.2f ms",
                double(-render_config.vset_values[131]) * 100.0,
                uint32_t(-render_config.vset_values[132]),
                uint32_t(-render_config.vset_values[132]),
                self.interactiveTargetGpuMs,
                render_gpu_ms];
        } else if (self.refinementPassIndex < hud_refinement_pass_count) {
            interactive_status = [NSString stringWithFormat:
                @"Interactive: exact %u/%u interlace passes | settle GPU %.2f ms",
                self.refinementPassIndex,
                hud_refinement_pass_count,
                self.refinementAccumulatedGpuMs];
        } else {
            interactive_status = [NSString stringWithFormat:
                @"Interactive: exact %u/%u interlace passes | settle GPU %.2f ms",
                hud_refinement_pass_count,
                hud_refinement_pass_count,
                self.refinementAccumulatedGpuMs];
        }
        detail = [NSString stringWithFormat:@"%@\n%@", interactive_status, detail];
    }
    self.hudLabel.stringValue = format_perf_hud(renderer,
                                                breakdown,
                                                format_cpu_breakdown(encode_ms, wait_ms, image_ms),
                                                detail,
                                                gpu_ms,
                                                cpu_ms,
                                                rendered_frame,
                                                self.config.preview ? 1u : std::max<uint32_t>(self.config.samples, 1u));
    if (self.config.renderer_backend != FPT_RENDERER_VOXEL) {
        [self updateSdfProfileWithConfigBuffer:cfg_buffer
                                 sampleGpuTime:render_gpu_ms
                                          frame:rendered_frame
                                         groups:groups
                                threadsPerGroup:threads_per_group];
    }
}
@end

extern "C" int fpt_metal_device_name(char *name, size_t name_len) {
    @autoreleasepool {
        if (!name || name_len == 0u) return 1;
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device) {
            name[0] = '\0';
            return 1;
        }
        std::snprintf(name, name_len, "%s", device.name.UTF8String ?: "Unknown Metal GPU");
        return 0;
    }
}

extern "C" int fpt_test_voxel_dda(const char *metallib_path,
                                   char *error,
                                   size_t error_len) {
    @autoreleasepool {
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device) {
            set_error(error, error_len, "Metal is unavailable on this machine");
            return 1;
        }
        NSError *ns_error = nil;
        id<MTLLibrary> library = [device newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)]
                                                     error:&ns_error];
        if (!library) {
            set_error(error, error_len, "failed to load DDA test metallib: %s",
                      ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLFunction> function = [library newFunctionWithName:@"voxel_dda_contract_test"];
        if (!function) {
            set_error(error, error_len, "voxel_dda_contract_test not found in metallib");
            return 1;
        }
        id<MTLComputePipelineState> pipeline = [device newComputePipelineStateWithFunction:function
                                                                                       error:&ns_error];
        if (!pipeline) {
            set_error(error, error_len, "failed to create DDA test pipeline: %s",
                      ns_error.localizedDescription.UTF8String);
            return 1;
        }
        const uint32_t initial_mask = UINT32_MAX;
        id<MTLBuffer> result = [device newBufferWithBytes:&initial_mask
                                                   length:sizeof(initial_mask)
                                                  options:MTLResourceStorageModeShared];
        FptRenderConfig program_configs[12] = {};
        for (FptRenderConfig &config : program_configs) {
            config.sdf_id = FPT_SDF_PROGRAM;
            config.voxel_surface_band = 0.5f;
        }
        auto append = [](FptRenderConfig &config, uint32_t opcode, uint32_t flags,
                         float x, float y, float z, float w) {
            FptSdfInstruction &instruction = config.sdf_program[config.sdf_program_count++];
            instruction.opcode = opcode;
            instruction.flags = flags;
            instruction.data[0] = x;
            instruction.data[1] = y;
            instruction.data[2] = z;
            instruction.data[3] = w;
        };
        append(program_configs[0], FPT_SDF_OP_SPHERE, 0u, 1.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[1], FPT_SDF_OP_BOX, 0u, 0.8f, 0.6f, 0.4f, 0.0f);
        append(program_configs[2], FPT_SDF_OP_PLANE, 0u, 1.0f, 2.0f, -0.5f, 0.2f);
        append(program_configs[3], FPT_SDF_OP_REPEAT, 0u, 4.0f, 4.0f, 4.0f, 0.0f);
        append(program_configs[3], FPT_SDF_OP_SPHERE, 0u, 1.0f, 0.0f, 0.0f, 0.0f);

        append(program_configs[4], FPT_SDF_OP_ABS, 0u, 0.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_ROTATE_X, 0u, 0.31f, 0.0f, 0.0f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_ROTATE_Y, 0u, -0.47f, 0.0f, 0.0f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_ROTATE_Z, 0u, 0.19f, 0.0f, 0.0f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_SCALE, 0u, 1.25f, 0.0f, 0.0f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_TRANSLATE, 0u, 0.2f, -0.1f, 0.3f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_BOX, 0u, 1.0f, 0.8f, 0.6f, 0.0f);
        append(program_configs[4], FPT_SDF_OP_SPHERE, 2u, 0.35f, 0.0f, 0.0f, 0.0f);

        append(program_configs[5], FPT_SDF_OP_SPHERE, 0u, 1.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[5], FPT_SDF_OP_TRANSLATE, 0u, 0.5f, 0.0f, 0.0f, 0.0f);
        append(program_configs[5], FPT_SDF_OP_BOX, 1u, 0.7f, 0.7f, 0.7f, 0.0f);

        append(program_configs[6], FPT_SDF_OP_SPHERE, 0u, 0.7f, 0.0f, 0.0f, 0.0f);
        append(program_configs[6], FPT_SDF_OP_TRANSLATE, 0u, 0.9f, 0.0f, 0.0f, 0.0f);
        append(program_configs[6], FPT_SDF_OP_SPHERE, 0u, 0.7f, 0.0f, 0.0f, 0.0f);

        append(program_configs[7], FPT_SDF_OP_SORT_DESC, 0u, 0.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[7], FPT_SDF_OP_SPHERE, 0u, 1.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[8], FPT_SDF_OP_SORT_DESC, 0u, 0.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[8], FPT_SDF_OP_BOX, 0u, 0.9f, 0.55f, 0.3f, 0.0f);
        program_configs[9].sdf_id = FPT_SDF_CAGE_FRACTAL;
        program_configs[9].bound_grid_cage_bounds = 1u;
        program_configs[9].set_values[2] = 0.25f;
        program_configs[9].set_values[3] = 0.5f;
        program_configs[9].set_values[4] = 0.2f;
        program_configs[9].set_values[5] = 4.0f;
        append(program_configs[10], FPT_SDF_OP_ROTATE_X, 0u, 0.31f, 0.0f, 0.0f, 0.0f);
        append(program_configs[10], FPT_SDF_OP_ROTATE_Y, 0u, -0.47f, 0.0f, 0.0f, 0.0f);
        append(program_configs[10], FPT_SDF_OP_ROTATE_Z, 0u, 0.19f, 0.0f, 0.0f, 0.0f);
        append(program_configs[10], FPT_SDF_OP_SCALE, 0u, -1.25f, 0.0f, 0.0f, 0.0f);
        append(program_configs[10], FPT_SDF_OP_TRANSLATE, 0u, 0.2f, -0.1f, 0.3f, 0.0f);
        append(program_configs[10], FPT_SDF_OP_SPHERE, 0u, 0.9f, 0.0f, 0.0f, 0.0f);
        append(program_configs[10], FPT_SDF_OP_BOX, 2u, 0.25f, 0.35f, 0.2f, 0.0f);
        append(program_configs[11], FPT_SDF_OP_ABS, 0u, 0.0f, 0.0f, 0.0f, 0.0f);
        append(program_configs[11], FPT_SDF_OP_TRANSLATE, 0u, 0.65f, 0.35f, 0.2f, 0.0f);
        append(program_configs[11], FPT_SDF_OP_SPHERE, 0u, 0.42f, 0.0f, 0.0f, 0.0f);
        id<MTLBuffer> program_config_buffer =
            [device newBufferWithBytes:program_configs
                                length:sizeof(program_configs)
                               options:MTLResourceStorageModeShared];
        id<MTLCommandQueue> queue = [device newCommandQueue];
        if (!result || !program_config_buffer || !queue) {
            set_error(error, error_len, "failed to allocate DDA test resources");
            return 1;
        }
        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:result offset:0 atIndex:0];
        [encoder setBuffer:program_config_buffer offset:0 atIndex:1];
        [encoder dispatchThreads:MTLSizeMake(1u, 1u, 1u)
            threadsPerThreadgroup:MTLSizeMake(1u, 1u, 1u)];
        [encoder endEncoding];
        [command commit];
        [command waitUntilCompleted];
        if (command.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "DDA contract command failed: %s",
                      command.error.localizedDescription.UTF8String);
            return 1;
        }
        const uint32_t failure_mask = *static_cast<const uint32_t *>(result.contents);
        if (failure_mask != 0u) {
            set_error(error, error_len, "DDA contract failure mask: 0x%08x", failure_mask);
            return 1;
        }

        FptRenderConfig parity_config = program_configs[0];
        parity_config.voxel_resolution = 32u;
        parity_config.voxel_storage = FPT_VOXEL_STORAGE_SPARSE_BRICKS;
        parity_config.voxel_coverage_mode = FPT_VOXEL_COVERAGE_INTERVAL;
        parity_config.voxel_brick_rejection = 1u;
        parity_config.voxel_bounds_min[0] = -2.0f;
        parity_config.voxel_bounds_min[1] = -2.0f;
        parity_config.voxel_bounds_min[2] = -2.0f;
        parity_config.voxel_bounds_max[0] = 2.0f;
        parity_config.voxel_bounds_max[1] = 2.0f;
        parity_config.voxel_bounds_max[2] = 2.0f;
        id<MTLBuffer> parity_config_buffer =
            [device newBufferWithBytes:&parity_config
                                length:sizeof(parity_config)
                               options:MTLResourceStorageModeShared];
        const size_t parity_cell_count = 32u * 32u * 32u;
        id<MTLBuffer> dense_cells =
            [device newBufferWithLength:parity_cell_count * sizeof(VoxelCellCpp)
                                options:MTLResourceStorageModeShared];
        id<MTLFunction> staging_function = [library newFunctionWithName:@"voxel_build_kernel"];
        id<MTLFunction> direct_function =
            [library newFunctionWithName:@"voxel_build_sparse_direct_kernel"];
        id<MTLComputePipelineState> staging_pipeline = staging_function
            ? [device newComputePipelineStateWithFunction:staging_function error:&ns_error]
            : nil;
        id<MTLComputePipelineState> direct_pipeline = direct_function
            ? [device newComputePipelineStateWithFunction:direct_function error:&ns_error]
            : nil;
        if (!parity_config_buffer || !dense_cells || !staging_pipeline || !direct_pipeline) {
            set_error(error, error_len, "failed to allocate direct-build parity resources");
            return 1;
        }
        id<MTLCommandBuffer> parity_command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> parity_encoder = [parity_command computeCommandEncoder];
        [parity_encoder setComputePipelineState:staging_pipeline];
        [parity_encoder setBuffer:dense_cells offset:0 atIndex:0];
        [parity_encoder setBuffer:parity_config_buffer offset:0 atIndex:1];
        [parity_encoder dispatchThreads:MTLSizeMake(32u, 32u, 32u)
                threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
        [parity_encoder endEncoding];
        [parity_command commit];
        [parity_command waitUntilCompleted];
        if (parity_command.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "staging parity build failed: %s",
                      parity_command.error.localizedDescription.UTF8String);
            return 1;
        }
        VoxelStorageResult staging_storage =
            finalize_voxel_storage(device, parity_config, dense_cells);
        VoxelStorageResult direct_storage =
            build_direct_voxel_storage(device, queue, direct_pipeline,
                                       parity_config_buffer, parity_config);
        if (!staging_storage.cells || !direct_storage.cells || direct_storage.overflow ||
            staging_storage.active_bricks != direct_storage.active_bricks ||
            staging_storage.active_cells != direct_storage.active_cells) {
            set_error(error, error_len, "direct-build occupancy totals differ from staging");
            return 1;
        }
        const auto *staging_pages =
            static_cast<const uint32_t *>(staging_storage.page_table.contents);
        const auto *direct_pages =
            static_cast<const uint32_t *>(direct_storage.page_table.contents);
        const auto *staging_cells =
            static_cast<const VoxelCellCpp *>(staging_storage.cells.contents);
        const auto *direct_cells =
            static_cast<const VoxelCellCpp *>(direct_storage.cells.contents);
        const VoxelCellCpp empty_cell = {};
        for (uint32_t z = 0u; z < 32u; ++z) {
            for (uint32_t y = 0u; y < 32u; ++y) {
                for (uint32_t x = 0u; x < 32u; ++x) {
                    const uint32_t page_index = x / 4u + (y / 4u) * 8u + (z / 4u) * 64u;
                    const uint32_t local = x % 4u + (y % 4u) * 4u + (z % 4u) * 16u;
                    const uint32_t staging_page = staging_pages[page_index];
                    const uint32_t direct_page = direct_pages[page_index];
                    const VoxelCellCpp *staging_cell = staging_page == 0u
                        ? &empty_cell
                        : &staging_cells[(staging_page - 1u) * 64u + local];
                    const VoxelCellCpp *direct_cell = direct_page == 0u
                        ? &empty_cell
                        : &direct_cells[(direct_page - 1u) * 64u + local];
                    if (std::memcmp(staging_cell, direct_cell, sizeof(VoxelCellCpp)) != 0) {
                        set_error(error, error_len,
                                  "direct-build cell mismatch at (%u,%u,%u)", x, y, z);
                        return 1;
                    }
                }
            }
        }
        return 0;
    }
}

extern "C" int fpt_test_async_stitch_context(
    const char *metallib_path,
    const char *stitch_metallib_path,
    const char *stitch_archive_path,
    const struct FptRenderConfig *config,
    struct FptAsyncJitStats *stats,
    char *error,
    size_t error_len) {
    @autoreleasepool {
        if (!config || !stats) {
            set_error(error, error_len, "missing async JIT validation input");
            return 1;
        }
        std::memset(stats, 0, sizeof(*stats));
        if (config->width == 0u || config->height == 0u ||
            config->sdf_id != FPT_SDF_PROGRAM ||
            config->sdf_program_count == 0u ||
            config->sdf_function_stitching == 0u) {
            set_error(error, error_len,
                      "async JIT validation requires a non-empty stitched typed program");
            return 1;
        }

        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device) {
            set_error(error, error_len, "Metal is unavailable on this machine");
            return 1;
        }
        NSError *ns_error = nil;
        id<MTLLibrary> fallback_library = [device
            newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)]
                        error:&ns_error];
        if (!fallback_library) {
            set_error(error, error_len, "failed to load fallback metallib: %s",
                      ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLLibrary> stitch_host_library = [device
            newLibraryWithURL:[NSURL fileURLWithPath:ns_string(stitch_metallib_path)]
                        error:&ns_error];
        if (!stitch_host_library) {
            set_error(error, error_len, "failed to load stitch-host metallib: %s",
                      ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLFunction> fallback_function = [fallback_library
            newFunctionWithName:@"preview_linear_kernel"];
        id<MTLComputePipelineState> fallback_pipeline = fallback_function
            ? [device newComputePipelineStateWithFunction:fallback_function
                                                    error:&ns_error]
            : nil;
        if (!fallback_pipeline) {
            set_error(error, error_len, "failed to create fallback preview pipeline: %s",
                      ns_error.localizedDescription.UTF8String ?: "kernel missing");
            return 1;
        }
        id<MTLCommandQueue> command_queue = [device newCommandQueue];
        id<MTLBuffer> config_buffer = [device
            newBufferWithBytes:config
                        length:sizeof(*config)
                       options:MTLResourceStorageModeShared];
        const size_t component_count = static_cast<size_t>(config->width) *
            config->height * 4u;
        const size_t output_bytes = component_count * sizeof(float);
        id<MTLBuffer> fallback_output = [device
            newBufferWithLength:output_bytes options:MTLResourceStorageModeShared];
        id<MTLBuffer> stitched_output = [device
            newBufferWithLength:output_bytes options:MTLResourceStorageModeShared];
        if (!command_queue || !config_buffer || !fallback_output || !stitched_output) {
            set_error(error, error_len, "failed to allocate async JIT validation resources");
            return 1;
        }

        __block id<MTLComputePipelineState> stitched_pipeline = nil;
        __block NSString *jit_failure = nil;
        __block double jit_build_ms = 0.0;
        __block uint32_t cache_status = 0u;
        auto jit_done = std::make_shared<std::atomic_bool>(false);
        dispatch_queue_t jit_queue = dispatch_queue_create(
            "com.fpt-metal.procedural-jit-test", DISPATCH_QUEUE_SERIAL);
        dispatch_group_t jit_group = dispatch_group_create();
        dispatch_semaphore_t jit_started = dispatch_semaphore_create(0);
        const FptRenderConfig requested = *config;
        const std::string archive_path = stitch_archive_path
            ? stitch_archive_path : "";

        dispatch_group_async(jit_group, jit_queue, ^{
            @autoreleasepool {
                NSDate *start = [NSDate date];
                dispatch_semaphore_signal(jit_started);
                id<MTLLibrary> stitched_library = nil;
                id<MTLFunction> distance_function = nil;
                id<MTLFunction> surface_function = nil;
                id<MTLBinaryArchive> binary_archive = nil;
                bool populate_archive = false;
                std::string failure;
                bool built = build_topology_stitched_library(
                    device, stitch_host_library, archive_path.c_str(), requested,
                    &stitched_library, &distance_function, &surface_function,
                    &binary_archive, &populate_archive, failure);
                NSError *pipeline_error = nil;
                if (built) {
                    id<MTLFunction> function = [stitch_host_library
                        newFunctionWithName:@"preview_linear_kernel"];
                    NSArray<id<MTLFunction>> *private_functions = @[
                        distance_function, surface_function];
                    stitched_pipeline = function ? new_compute_pipeline(
                        device, function, private_functions, binary_archive,
                        populate_archive, &pipeline_error) : nil;
                    if (!stitched_pipeline && !pipeline_error) {
                        failure = "stitched preview kernel is missing";
                    }
                }
                if (stitched_pipeline && populate_archive && binary_archive &&
                    !archive_path.empty()) {
                    NSURL *archive_url = [NSURL fileURLWithPath:
                        [NSString stringWithUTF8String:archive_path.c_str()]];
                    if (![binary_archive serializeToURL:archive_url
                                                  error:&pipeline_error]) {
                        stitched_pipeline = nil;
                    }
                }
                jit_build_ms = -[start timeIntervalSinceNow] * 1000.0;
                cache_status = populate_archive ? 1u : 2u;
                if (!stitched_pipeline) {
                    jit_failure = pipeline_error
                        ? pipeline_error.localizedDescription
                        : (failure.empty() ? @"unknown error" :
                            [NSString stringWithUTF8String:failure.c_str()]);
                }
                jit_done->store(true, std::memory_order_release);
            }
        });

        dispatch_semaphore_wait(jit_started, DISPATCH_TIME_FOREVER);
        std::string render_failure;
        if (!render_preview_buffer(command_queue, fallback_pipeline,
                                   fallback_output, config_buffer,
                                   config->width, config->height,
                                   &stats->fallback_render_ms, render_failure)) {
            dispatch_group_wait(jit_group, DISPATCH_TIME_FOREVER);
            set_error(error, error_len, "%s", render_failure.c_str());
            return 1;
        }
        stats->fallback_completed_before_jit =
            jit_done->load(std::memory_order_acquire) ? 0u : 1u;
        dispatch_group_wait(jit_group, DISPATCH_TIME_FOREVER);
        stats->jit_build_ms = jit_build_ms;
        stats->cache_status = cache_status;
        if (!stitched_pipeline) {
            set_error(error, error_len, "async stitched pipeline failed: %s",
                      jit_failure.UTF8String ?: "unknown error");
            return 1;
        }
        if (!render_preview_buffer(command_queue, stitched_pipeline,
                                   stitched_output, config_buffer,
                                   config->width, config->height,
                                   &stats->stitched_render_ms, render_failure)) {
            set_error(error, error_len, "%s", render_failure.c_str());
            return 1;
        }

        const auto *fallback_pixels = static_cast<const float *>(
            fallback_output.contents);
        const auto *stitched_pixels = static_cast<const float *>(
            stitched_output.contents);
        float max_error = 0.0f;
        for (size_t index = 0u; index < component_count; ++index) {
            const float lhs = fallback_pixels[index];
            const float rhs = stitched_pixels[index];
            if (lhs == rhs) continue;
            if (!std::isfinite(lhs) || !std::isfinite(rhs)) {
                max_error = INFINITY;
                break;
            }
            max_error = std::max(max_error, std::abs(lhs - rhs));
        }
        stats->max_absolute_error = max_error;
        if (max_error > 1.0e-4f) {
            set_error(error, error_len,
                      "fallback/stitched preview mismatch: max absolute error %.9g",
                      max_error);
            return 1;
        }
        return 0;
    }
}

extern "C" int fpt_test_typed_soa(
    const char *metallib_path,
    const struct FptRenderConfig *config,
    struct FptStitchValidationStats *stats,
    char *error,
    size_t error_len) {
    @autoreleasepool {
        if (!config || !stats) {
            set_error(error, error_len, "missing typed-SoA validation input");
            return 1;
        }
        std::memset(stats, 0, sizeof(*stats));
        const uint32_t primitive_count = config->sdf_typed_soa.sphere_count +
            config->sdf_typed_soa.box_count + config->sdf_typed_soa.plane_count;
        if (config->sdf_id != FPT_SDF_PROGRAM || primitive_count == 0u) {
            set_error(error, error_len,
                      "typed-SoA validation requires a lowered typed program");
            return 1;
        }

        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        NSError *ns_error = nil;
        id<MTLLibrary> library = device ? [device
            newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)]
                        error:&ns_error] : nil;
        id<MTLFunction> function = library
            ? [library newFunctionWithName:@"typed_soa_validation_kernel"] : nil;
        id<MTLComputePipelineState> pipeline = function
            ? [device newComputePipelineStateWithFunction:function error:&ns_error]
            : nil;
        id<MTLCommandQueue> queue = device ? [device newCommandQueue] : nil;
        struct ExactValidationCountsCpp {
            uint32_t distance_failures;
            uint32_t gradient_failures;
            uint32_t max_distance_error_bits;
            uint32_t max_gradient_error_bits;
        } initial_counts = {};
        id<MTLBuffer> counts_buffer = device ? [device
            newBufferWithBytes:&initial_counts
                        length:sizeof(initial_counts)
                       options:MTLResourceStorageModeShared] : nil;
        id<MTLBuffer> config_buffer = device ? [device
            newBufferWithBytes:config
                        length:sizeof(*config)
                       options:MTLResourceStorageModeShared] : nil;
        if (!pipeline || !queue || !counts_buffer || !config_buffer) {
            set_error(error, error_len, "failed to create typed-SoA validation resources: %s",
                      ns_error.localizedDescription.UTF8String ?: "Metal unavailable");
            return 1;
        }

        constexpr NSUInteger sample_count = 1u << 20u;
        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:counts_buffer offset:0 atIndex:0];
        [encoder setBuffer:config_buffer offset:0 atIndex:1];
        NSUInteger group_width = std::min<NSUInteger>(
            pipeline.maxTotalThreadsPerThreadgroup, 256u);
        [encoder dispatchThreads:MTLSizeMake(sample_count, 1u, 1u)
           threadsPerThreadgroup:MTLSizeMake(group_width, 1u, 1u)];
        [encoder endEncoding];
        [command commit];
        [command waitUntilCompleted];
        if (command.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "typed-SoA validation dispatch failed: %s",
                      command.error.localizedDescription.UTF8String);
            return 1;
        }

        const auto *counts = static_cast<const ExactValidationCountsCpp *>(
            counts_buffer.contents);
        stats->sample_count = sample_count;
        stats->distance_failures = counts->distance_failures;
        stats->gradient_failures = counts->gradient_failures;
        std::memcpy(&stats->max_distance_error,
                    &counts->max_distance_error_bits, sizeof(float));
        std::memcpy(&stats->max_gradient_error,
                    &counts->max_gradient_error_bits, sizeof(float));
        if (counts->distance_failures != 0u || counts->gradient_failures != 0u) {
            set_error(error, error_len,
                      "typed-SoA validation failed: %u distance, %u gradient",
                      counts->distance_failures, counts->gradient_failures);
            return 1;
        }
        return 0;
    }
}

extern "C" int fpt_mandelbulber_sample_field(
    const char *metallib_path,
    const char *shader_source,
    size_t shader_source_len,
    const struct FptRenderConfig *config,
    const float *points_xyzw,
    size_t point_count,
    struct FptMandelbulberFieldSample *samples,
    char *error,
    size_t error_len) {
    @autoreleasepool {
        if (!metallib_path || !config || !points_xyzw || !samples ||
            point_count == 0u) {
            set_error(error, error_len,
                      "missing Mandelbulber field-sampling input");
            return 1;
        }
        if (config->sdf_id != FPT_SDF_MANDELBULBER) {
            set_error(error, error_len,
                      "field sampling requires a Mandelbulber scene");
            return 1;
        }

        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        NSError *ns_error = nil;
        id<MTLLibrary> library = nil;
        if (device && shader_source && shader_source_len > 0u) {
            NSString *source = [[NSString alloc]
                initWithBytes:shader_source
                       length:shader_source_len
                     encoding:NSUTF8StringEncoding];
            bool cache_hit = false;
            library = source ? compile_runtime_source_library(
                device, source, &cache_hit, &ns_error) : nil;
        } else if (device) {
            library = [device
                newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)]
                            error:&ns_error];
        }
        id<MTLFunction> function = library
            ? [library newFunctionWithName:@"mandelbulber_field_sample_kernel"]
            : nil;
        id<MTLComputePipelineState> pipeline = function
            ? [device newComputePipelineStateWithFunction:function error:&ns_error]
            : nil;
        id<MTLCommandQueue> queue = device ? [device newCommandQueue] : nil;
        const size_t points_bytes = point_count * sizeof(float) * 4u;
        const size_t samples_bytes =
            point_count * sizeof(struct FptMandelbulberFieldSample);
        id<MTLBuffer> points_buffer = device ? [device
            newBufferWithBytes:points_xyzw
                        length:points_bytes
                       options:MTLResourceStorageModeShared] : nil;
        id<MTLBuffer> samples_buffer = device ? [device
            newBufferWithLength:samples_bytes
                       options:MTLResourceStorageModeShared] : nil;
        id<MTLBuffer> config_buffer = device ? [device
            newBufferWithBytes:config
                        length:sizeof(*config)
                       options:MTLResourceStorageModeShared] : nil;
        if (!pipeline || !queue || !points_buffer || !samples_buffer ||
            !config_buffer) {
            set_error(error, error_len,
                      "failed to create Mandelbulber field-sampling resources: %s",
                      ns_error.localizedDescription.UTF8String ?:
                          "Metal unavailable");
            return 1;
        }

        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:points_buffer offset:0 atIndex:0];
        [encoder setBuffer:samples_buffer offset:0 atIndex:1];
        [encoder setBuffer:config_buffer offset:0 atIndex:2];
        NSUInteger group_width = std::min<NSUInteger>(
            pipeline.maxTotalThreadsPerThreadgroup, 256u);
        [encoder dispatchThreads:MTLSizeMake(point_count, 1u, 1u)
           threadsPerThreadgroup:MTLSizeMake(group_width, 1u, 1u)];
        [encoder endEncoding];
        [command commit];
        [command waitUntilCompleted];
        if (command.status == MTLCommandBufferStatusError) {
            set_error(error, error_len,
                      "Mandelbulber field-sampling dispatch failed: %s",
                      command.error.localizedDescription.UTF8String);
            return 1;
        }
        std::memcpy(samples, samples_buffer.contents, samples_bytes);
        return 0;
    }
}

extern "C" int fpt_metal_voxel_build(
    const char *metallib_path,
    const struct FptRenderConfig *config,
    void *cells,
    size_t cells_len,
    uint32_t surface_payload_mode,
    uint32_t *packed_surface,
    size_t packed_surface_len,
    double *build_ms,
    char *error,
    size_t error_len) {
    @autoreleasepool {
        if (!metallib_path || !config || !cells) {
            set_error(error, error_len, "missing Metal voxel-build input");
            return 1;
        }
        const uint32_t resolution = config->voxel_resolution;
        if (resolution == 0u || resolution > 512u) {
            set_error(error, error_len, "voxel resolution must be 1..512");
            return 1;
        }
        const size_t cell_count = static_cast<size_t>(resolution) * resolution * resolution;
        if (cell_count > SIZE_MAX / sizeof(VoxelCellCpp) ||
            cells_len != cell_count * sizeof(VoxelCellCpp)) {
            set_error(error, error_len,
                      "voxel output is %zu bytes; expected %zu bytes",
                      cells_len, cell_count * sizeof(VoxelCellCpp));
            return 1;
        }
        const size_t surface_word_stride =
            surface_payload_mode == FPT_VOXEL_SURFACE_COMPLEX_PATCH ? 5u
            : (surface_payload_mode == FPT_VOXEL_SURFACE_BOUNDED_PATCH ? 3u : 1u);
        const size_t expected_surface_words =
            surface_payload_mode == FPT_VOXEL_SURFACE_NONE ? 0u : cell_count * surface_word_stride;
        if (surface_payload_mode > FPT_VOXEL_SURFACE_COMPLEX_PATCH ||
            (surface_payload_mode == FPT_VOXEL_SURFACE_NONE) != (packed_surface == nullptr) ||
            (packed_surface && packed_surface_len != expected_surface_words)) {
            set_error(error, error_len,
                      "surface output mode %u has %zu records; expected %zu",
                      surface_payload_mode, packed_surface_len,
                      expected_surface_words);
            return 1;
        }

        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        NSError *ns_error = nil;
        id<MTLLibrary> library = device ? [device
            newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)]
                        error:&ns_error] : nil;
        NSString *function_name = surface_payload_mode == FPT_VOXEL_SURFACE_COMPLEX_PATCH
            ? @"voxel_build_surface_complex_patch_kernel"
            : (surface_payload_mode == FPT_VOXEL_SURFACE_BOUNDED_PATCH
            ? @"voxel_build_surface_patch_kernel"
            : (surface_payload_mode == FPT_VOXEL_SURFACE_PLANE
            ? @"voxel_build_surface_plane_kernel"
            : (surface_payload_mode == FPT_VOXEL_SURFACE_NORMAL
                ? @"voxel_build_surface_kernel"
                : @"voxel_build_kernel")));
        id<MTLFunction> function = library
            ? [library newFunctionWithName:function_name] : nil;
        id<MTLComputePipelineState> pipeline = function
            ? [device newComputePipelineStateWithFunction:function error:&ns_error] : nil;
        id<MTLCommandQueue> queue = device ? [device newCommandQueue] : nil;
        id<MTLBuffer> cells_buffer = device ? [device
            newBufferWithLength:cells_len options:MTLResourceStorageModeShared] : nil;
        id<MTLBuffer> config_buffer = device ? [device
            newBufferWithBytes:config length:sizeof(*config)
                       options:MTLResourceStorageModeShared] : nil;
        id<MTLBuffer> surface_buffer = packed_surface && device ? [device
            newBufferWithLength:expected_surface_words * sizeof(uint32_t)
                        options:MTLResourceStorageModeShared] : nil;
        if (!pipeline || !queue || !cells_buffer || !config_buffer ||
            (packed_surface && !surface_buffer)) {
            set_error(error, error_len, "failed to create Metal voxel-build resources: %s",
                      ns_error.localizedDescription.UTF8String ?: "Metal unavailable");
            return 1;
        }

        NSDate *started = [NSDate date];
        id<MTLCommandBuffer> command = [queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command computeCommandEncoder];
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:cells_buffer offset:0 atIndex:0];
        [encoder setBuffer:config_buffer offset:0 atIndex:1];
        if (surface_buffer) [encoder setBuffer:surface_buffer offset:0 atIndex:2];
        [encoder dispatchThreads:MTLSizeMake(resolution, resolution, resolution)
           threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
        [encoder endEncoding];
        [command commit];
        [command waitUntilCompleted];
        if (command.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "Metal voxel build failed: %s",
                      command.error.localizedDescription.UTF8String);
            return 1;
        }
        if (build_ms) *build_ms = -[started timeIntervalSinceNow] * 1000.0;
        std::memcpy(cells, cells_buffer.contents, cells_len);
        if (packed_surface) {
            std::memcpy(packed_surface, surface_buffer.contents,
                        expected_surface_words * sizeof(uint32_t));
        }
        return 0;
    }
}

extern "C" int fpt_metal_render(const char *metallib_path,
                                 const char *stitch_metallib_path,
                                 const char *stitch_archive_path,
                                 const char *output_path,
                                 const char *shader_source,
                                 size_t shader_source_len,
                                 const struct FptRenderConfig *config,
                                 double *build_ms,
                                 double *elapsed_ms,
                                 uint64_t *voxel_memory_bytes,
                                 uint32_t *voxel_active_bricks,
                                 uint64_t *voxel_active_cells,
                                 uint32_t *voxel_rejected_bricks,
                                 struct FptBoundGridStats *bound_grid_stats,
                                 uint32_t *stitch_cache_status,
                                 struct FptStitchValidationStats *stitch_validation_stats,
                                 struct FptStitchPipelineStats *stitch_pipeline_stats,
                                 struct FptSdfProfileStats *sdf_profile_stats,
                                 float *linear_output,
                                 size_t linear_output_len,
                                 char *error,
                                size_t error_len) {
    @autoreleasepool {
        if (!config) {
            set_error(error, error_len, "missing render config");
            return 1;
        }
        if (config->width == 0 || config->height == 0) {
            set_error(error, error_len, "invalid render size %ux%u", config->width, config->height);
            return 1;
        }

        FptRenderConfig hydrated_config = *config;
        if (!hydrate_hdri_config(hydrated_config, error, error_len)) return 1;
        config = &hydrated_config;

        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device) {
            set_error(error, error_len, "Metal is unavailable on this machine");
            return 1;
        }
        NSError *ns_error = nil;
        id<MTLLibrary> library = [device newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)] error:&ns_error];
        if (!library) {
            set_error(error, error_len, "failed to load metallib %s: %s", metallib_path, ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLLibrary> render_library = library;
        id<MTLLibrary> stitched_library = nil;
        id<MTLFunction> stitched_distance_function = nil;
        id<MTLFunction> stitched_surface_function = nil;
        NSArray<id<MTLFunction>> *stitched_private_functions = @[];
        id<MTLBinaryArchive> stitch_binary_archive = nil;
        bool populate_stitch_binary_archive = false;
        if (stitch_cache_status) *stitch_cache_status = 0u;
        if (stitch_validation_stats) {
            *stitch_validation_stats = {};
        }
        if (stitch_pipeline_stats) {
            *stitch_pipeline_stats = {};
        }
        if (sdf_profile_stats) {
            *sdf_profile_stats = {};
        }
        NSDate *runtime_source_build_start = nil;
        double runtime_source_build_elapsed_ms = 0.0;
        const bool tiny_linked_helper =
            config->sdf_topology_specialization != 0u &&
            config->sdf_runtime_source_bytecode == 3u;
        if (config->sdf_function_stitching != 0u && !tiny_linked_helper) {
            if (config->renderer_backend != FPT_RENDERER_SDF) {
                set_error(error, error_len,
                          "function stitching requires the direct SDF renderer");
                return 1;
            }
            id<MTLLibrary> stitch_host_library = [device
                newLibraryWithURL:[NSURL fileURLWithPath:
                    ns_string(stitch_metallib_path)] error:&ns_error];
            if (!stitch_host_library) {
                set_error(error, error_len,
                          "failed to load stitch-host metallib %s: %s",
                          stitch_metallib_path,
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
            runtime_source_build_start = [NSDate date];
            std::string stitching_failure;
            if (!build_topology_stitched_library(
                    device, stitch_host_library, stitch_archive_path, *config,
                    &stitched_library, &stitched_distance_function,
                    &stitched_surface_function, &stitch_binary_archive,
                    &populate_stitch_binary_archive, stitching_failure)) {
                set_error(error, error_len, "%s", stitching_failure.c_str());
                return 1;
            }
            if (stitch_cache_status) {
                *stitch_cache_status = populate_stitch_binary_archive ? 1u : 2u;
            }
            stitched_private_functions = @[
                stitched_distance_function, stitched_surface_function];
            render_library = stitch_host_library;
        }
        if (tiny_linked_helper) {
            if (config->renderer_backend != FPT_RENDERER_SDF) {
                set_error(error, error_len,
                          "tiny linked helpers require the direct SDF renderer");
                return 1;
            }
            id<MTLLibrary> stitch_host_library = [device
                newLibraryWithURL:[NSURL fileURLWithPath:
                    ns_string(stitch_metallib_path)] error:&ns_error];
            if (!stitch_host_library) {
                set_error(error, error_len,
                          "failed to load tiny-link host metallib %s: %s",
                          stitch_metallib_path,
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
            std::string tiny_source;
            std::string generation_failure;
            if (!generate_tiny_linked_topology_source(
                    shader_source, shader_source_len, *config,
                    tiny_source, generation_failure)) {
                set_error(error, error_len, "%s", generation_failure.c_str());
                return 1;
            }
            salt_runtime_source_for_benchmark(tiny_source);
            if (stitch_pipeline_stats) {
                stitch_pipeline_stats->runtime_source_bytes = tiny_source.size();
            }
            NSString *metal_source = [[NSString alloc]
                initWithBytes:tiny_source.data()
                       length:tiny_source.size()
                     encoding:NSUTF8StringEncoding];
            if (!metal_source) {
                set_error(error, error_len,
                          "failed to decode tiny linked Metal source");
                return 1;
            }
            runtime_source_build_start = [NSDate date];
            bool runtime_source_cache_hit = false;
            id<MTLLibrary> helper_library = compile_runtime_source_library(
                device, metal_source, &runtime_source_cache_hit, &ns_error);
            if (stitch_pipeline_stats) {
                stitch_pipeline_stats->runtime_library_compile_ms =
                    -[runtime_source_build_start timeIntervalSinceNow] * 1000.0;
            }
            if (!helper_library) {
                set_error(error, error_len,
                          "tiny linked Metal compilation failed: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
            stitched_distance_function = [helper_library
                newFunctionWithName:@"deTopologyStitchedDistance"];
            stitched_surface_function = [helper_library
                newFunctionWithName:@"deTopologyStitchedSurface"];
            if (!stitched_distance_function || !stitched_surface_function) {
                set_error(error, error_len,
                          "tiny linked distance or surface helper is missing");
                return 1;
            }
            stitched_private_functions = @[
                stitched_distance_function, stitched_surface_function];
            // The precompiled host dispatches its unresolved evaluator calls
            // through the same stable runtime switch as function stitching.
            hydrated_config.sdf_function_stitching = 1u;
            config = &hydrated_config;
            render_library = stitch_host_library;
        }
        const bool use_runtime_source =
            !tiny_linked_helper &&
            (config->sdf_topology_specialization != 0u ||
             config->sdf_runtime_source_bytecode != 0u);
        if (use_runtime_source) {
            if (config->renderer_backend != FPT_RENDERER_SDF) {
                set_error(error, error_len,
                          "runtime-source compilation requires the direct SDF renderer");
                return 1;
            }
            std::string runtime_source;
            if (config->sdf_topology_specialization != 0u) {
                std::string generation_failure;
                const bool generated = config->sdf_runtime_source_bytecode == 2u
                    ? generate_dual_topology_specialized_source(
                        shader_source, shader_source_len, *config,
                        runtime_source, generation_failure)
                    : generate_topology_specialized_source(
                        shader_source, shader_source_len, *config,
                        runtime_source, generation_failure);
                if (!generated) {
                    set_error(error, error_len, "%s", generation_failure.c_str());
                    return 1;
                }
            } else {
                runtime_source.assign(shader_source, shader_source_len);
            }
            salt_runtime_source_for_benchmark(runtime_source);
            if (stitch_pipeline_stats) {
                stitch_pipeline_stats->runtime_source_bytes = runtime_source.size();
            }
            NSString *metal_source = [[NSString alloc]
                initWithBytes:runtime_source.data()
                       length:runtime_source.size()
                     encoding:NSUTF8StringEncoding];
            if (!metal_source) {
                set_error(error, error_len,
                          "failed to decode runtime Metal source");
                return 1;
            }
            runtime_source_build_start = [NSDate date];
            bool runtime_source_cache_hit = false;
            render_library = compile_runtime_source_library(
                device, metal_source, &runtime_source_cache_hit, &ns_error);
            if (stitch_pipeline_stats) {
                stitch_pipeline_stats->runtime_library_compile_ms =
                    -[runtime_source_build_start timeIntervalSinceNow] * 1000.0;
            }
            if (!render_library) {
                set_error(error, error_len,
                          "runtime Metal compilation failed: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        const bool use_voxels = config->renderer_backend == FPT_RENDERER_VOXEL;
        const bool use_bound_grid = config->renderer_backend == FPT_RENDERER_BOUND_GRID;
        const bool use_regional = config->renderer_backend == FPT_RENDERER_REGIONAL;
        if (voxel_memory_bytes) *voxel_memory_bytes = 0u;
        if (voxel_active_bricks) *voxel_active_bricks = 0u;
        if (voxel_active_cells) *voxel_active_cells = 0u;
        if (voxel_rejected_bricks) *voxel_rejected_bricks = 0u;
        if (bound_grid_stats) std::memset(bound_grid_stats, 0, sizeof(*bound_grid_stats));
        const uint32_t requested_samples = std::clamp<uint32_t>(config->samples, 1u, 512u);
        const bool auto_batch_accumulation = requested_samples <= 16u && config->sdf_id != FPT_SDF_CAGE_FRACTAL;
        const bool use_batch_accumulation = !config->preview &&
                                            (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_BATCH ||
                                             (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_AUTO && auto_batch_accumulation));
        const bool use_chunked_accumulation = !config->preview &&
                                              (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_CHUNKED ||
                                               (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_AUTO && !auto_batch_accumulation));
        const bool use_tiled_accumulation = !config->preview &&
                                            use_batch_accumulation &&
                                            config->renderer_backend == FPT_RENDERER_SDF &&
                                            config->sdf_id == FPT_SDF_MANDELBULBER &&
                                            std::getenv("FPT_MANDEL_TILED_DISPATCH") != nullptr;
        NSString *main_function_name = nil;
        if (use_voxels) {
            main_function_name = config->preview ? @"voxel_preview_linear_kernel" :
                (use_batch_accumulation ? @"voxel_accumulate_all_kernel" :
                 (use_chunked_accumulation ? @"voxel_accumulate_chunk_kernel" : @"voxel_accumulate_kernel"));
        } else if (use_regional && !config->preview) {
            main_function_name = use_batch_accumulation
                ? @"regional_accumulate_all_kernel"
                : (use_chunked_accumulation
                    ? @"regional_accumulate_chunk_kernel"
                    : @"regional_accumulate_kernel");
        } else if (use_bound_grid && !config->preview) {
            main_function_name = use_batch_accumulation ? @"bound_grid_accumulate_all_kernel" :
                (use_chunked_accumulation ? @"bound_grid_accumulate_chunk_kernel" :
                 @"bound_grid_accumulate_kernel");
        } else {
            main_function_name = config->preview ? @"preview_linear_kernel" :
                (use_tiled_accumulation ? @"accumulate_all_tile_kernel" :
                 (use_batch_accumulation ? @"accumulate_all_kernel" :
                 (use_chunked_accumulation ? @"accumulate_chunk_kernel" : @"accumulate_kernel")));
        }
        const bool dual_generated_library =
            config->sdf_topology_specialization != 0u &&
            config->sdf_runtime_source_bytecode == 2u;
        id<MTLFunction> main_function = new_topology_runtime_function(
            render_library, main_function_name, dual_generated_library,
            config->sdf_stitched_surface != 0u, &ns_error);
        if (!main_function) {
            set_error(error, error_len, "%s not found in metallib", main_function_name.UTF8String);
            return 1;
        }
        if (!stitch_binary_archive && stitch_archive_path &&
            stitch_archive_path[0] != '\0') {
            if (!runtime_source_build_start) runtime_source_build_start = [NSDate date];
            if (!prepare_compute_binary_archive(
                    device, stitch_archive_path, &stitch_binary_archive,
                    &populate_stitch_binary_archive, &ns_error)) {
                set_error(error, error_len,
                          "failed to prepare compute pipeline archive: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
            if (stitch_cache_status && stitch_binary_archive) {
                *stitch_cache_status = populate_stitch_binary_archive ? 1u : 2u;
            }
        }
        NSDate *runtime_pipeline_start = runtime_source_build_start
            ? [NSDate date] : nil;
        id<MTLComputePipelineState> main_pipeline = new_compute_pipeline(
            device, main_function, stitched_private_functions,
            stitch_binary_archive, populate_stitch_binary_archive, &ns_error);
        if (runtime_pipeline_start && stitch_pipeline_stats) {
            stitch_pipeline_stats->runtime_pipeline_link_ms =
                -[runtime_pipeline_start timeIntervalSinceNow] * 1000.0;
        }
        if (!main_pipeline) {
            set_error(error, error_len, "failed to create compute pipeline: %s", ns_error.localizedDescription.UTF8String);
            return 1;
        }
        if (stitch_pipeline_stats) {
            stitch_pipeline_stats->thread_execution_width =
                static_cast<uint32_t>(main_pipeline.threadExecutionWidth);
            stitch_pipeline_stats->max_total_threads_per_threadgroup =
                static_cast<uint32_t>(
                    main_pipeline.maxTotalThreadsPerThreadgroup);
            stitch_pipeline_stats->static_threadgroup_memory_bytes =
                static_cast<uint64_t>(
                    main_pipeline.staticThreadgroupMemoryLength);
        }
        id<MTLComputePipelineState> sdf_profile_pipeline = nil;
        if (!use_voxels && !use_bound_grid && !use_regional &&
            config->sdf_profile != 0u) {
            id<MTLFunction> profile_function = new_topology_runtime_function(
                render_library, @"sdf_profile_kernel", dual_generated_library,
                config->sdf_stitched_surface != 0u, &ns_error);
            sdf_profile_pipeline = profile_function
                ? new_compute_pipeline(device, profile_function,
                                       stitched_private_functions,
                                       stitch_binary_archive,
                                       populate_stitch_binary_archive, &ns_error)
                : nil;
            if (!sdf_profile_pipeline) {
                set_error(error, error_len,
                          "failed to create SDF profile pipeline: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        if (stitch_pipeline_stats && config->sdf_function_stitching != 0u) {
            stitch_pipeline_stats->instruction_count = std::min<uint32_t>(
                config->sdf_program_count, FPT_SDF_PROGRAM_MAX_OPS);
            auto is_translated_sphere_union = [&](uint32_t index) {
                return index + 1u < stitch_pipeline_stats->instruction_count &&
                    config->sdf_program[index].opcode == FPT_SDF_OP_TRANSLATE &&
                    config->sdf_program[index + 1u].opcode == FPT_SDF_OP_SPHERE &&
                    config->sdf_program[index + 1u].flags == 0u;
            };
            bool fused_one_pair = false;
            for (uint32_t index = 0u;
                 index < stitch_pipeline_stats->instruction_count;) {
                ++stitch_pipeline_stats->graph_node_count;
                if (config->sdf_stitch_fusion == 3u &&
                    is_translated_sphere_union(index) &&
                    is_translated_sphere_union(index + 2u)) {
                    index += 4u;
                } else if (is_translated_sphere_union(index) &&
                           (config->sdf_stitch_fusion == 2u ||
                            (config->sdf_stitch_fusion == 1u &&
                             !fused_one_pair))) {
                    fused_one_pair = true;
                    index += 2u;
                } else {
                    ++index;
                }
            }
            uint32_t previous_primitive_opcode = 0u;
            for (uint32_t index = 0u;
                 index < stitch_pipeline_stats->instruction_count; ++index) {
                const FptSdfInstruction &instruction = config->sdf_program[index];
                const bool primitive = instruction.opcode == FPT_SDF_OP_SPHERE ||
                    instruction.opcode == FPT_SDF_OP_BOX ||
                    instruction.opcode == FPT_SDF_OP_PLANE;
                if (!primitive) {
                    ++stitch_pipeline_stats->transform_instruction_count;
                    continue;
                }
                ++stitch_pipeline_stats->primitive_instruction_count;
                if (instruction.opcode != previous_primitive_opcode) {
                    ++stitch_pipeline_stats->primitive_type_runs;
                    previous_primitive_opcode = instruction.opcode;
                }
                switch (instruction.flags) {
                    case 0u: ++stitch_pipeline_stats->union_count; break;
                    case 1u: ++stitch_pipeline_stats->intersection_count; break;
                    case 2u: ++stitch_pipeline_stats->subtraction_count; break;
                    default: break;
                }
            }
            stitch_pipeline_stats->thread_execution_width =
                static_cast<uint32_t>(main_pipeline.threadExecutionWidth);
            stitch_pipeline_stats->max_total_threads_per_threadgroup =
                static_cast<uint32_t>(main_pipeline.maxTotalThreadsPerThreadgroup);
            stitch_pipeline_stats->static_threadgroup_memory_bytes =
                static_cast<uint64_t>(main_pipeline.staticThreadgroupMemoryLength);
        }
        id<MTLFunction> present_function = [library newFunctionWithName:@"present_kernel"];
        if (!present_function) {
            set_error(error, error_len, "present_kernel not found in metallib");
            return 1;
        }
        id<MTLComputePipelineState> present_pipeline = new_compute_pipeline(
            device, present_function, @[], stitch_binary_archive,
            populate_stitch_binary_archive, &ns_error);
        if (!present_pipeline) {
            set_error(error, error_len, "failed to create present pipeline: %s", ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLComputePipelineState> voxel_build_pipeline = nil;
        id<MTLComputePipelineState> direct_voxel_build_pipeline = nil;
        id<MTLComputePipelineState> bound_grid_build_pipeline = nil;
        id<MTLComputePipelineState> regional_program_build_pipeline = nil;
        id<MTLComputePipelineState> regional_program_validate_pipeline = nil;
        id<MTLComputePipelineState> regional_program_profile_pipeline = nil;
        id<MTLComputePipelineState> bound_grid_profile_pipeline = nil;
        id<MTLComputePipelineState> bound_grid_validate_pipeline = nil;
        if (use_voxels) {
            id<MTLFunction> voxel_build_function = [library newFunctionWithName:@"voxel_build_kernel"];
            if (!voxel_build_function) {
                set_error(error, error_len, "voxel_build_kernel not found in metallib");
                return 1;
            }
            voxel_build_pipeline = [device newComputePipelineStateWithFunction:voxel_build_function error:&ns_error];
            if (!voxel_build_pipeline) {
                set_error(error, error_len, "failed to create voxel build pipeline: %s", ns_error.localizedDescription.UTF8String);
                return 1;
            }
            if (config->voxel_build_mode == FPT_VOXEL_BUILD_DIRECT) {
                if (config->voxel_storage == FPT_VOXEL_STORAGE_DENSE) {
                    set_error(error, error_len, "direct voxel build requires a sparse voxel storage mode");
                    return 1;
                }
                id<MTLFunction> direct_function =
                    [library newFunctionWithName:@"voxel_build_sparse_direct_kernel"];
                direct_voxel_build_pipeline = direct_function
                    ? [device newComputePipelineStateWithFunction:direct_function error:&ns_error]
                    : nil;
                if (!direct_voxel_build_pipeline) {
                    set_error(error, error_len, "failed to create direct voxel build pipeline: %s",
                              ns_error.localizedDescription.UTF8String);
                    return 1;
                }
            }
        }
        if (use_bound_grid) {
            if (config->bound_grid_resolution != 32u &&
                config->bound_grid_resolution != 64u &&
                config->bound_grid_resolution != 128u &&
                config->bound_grid_resolution != 256u) {
                set_error(error, error_len,
                          "bound-grid resolution must be 32, 64, 128, or 256");
                return 1;
            }
            NSString *build_function_name = config->bound_grid_directional != 0u
                ? (config->bound_grid_fp16 != 0u
                    ? @"directional_grid_build_fp16_kernel"
                    : @"directional_grid_build_kernel")
                : @"bound_grid_build_kernel";
            id<MTLFunction> build_function =
                [library newFunctionWithName:build_function_name];
            bound_grid_build_pipeline = build_function
                ? [device newComputePipelineStateWithFunction:build_function error:&ns_error]
                : nil;
            if (!bound_grid_build_pipeline) {
                set_error(error, error_len, "failed to create bound-grid build pipeline: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
            if (config->bound_grid_profile != 0u) {
                id<MTLFunction> profile_function =
                    [library newFunctionWithName:@"bound_grid_profile_kernel"];
                id<MTLFunction> validate_function =
                    [library newFunctionWithName:@"bound_grid_validate_kernel"];
                bound_grid_profile_pipeline = profile_function
                    ? [device newComputePipelineStateWithFunction:profile_function error:&ns_error]
                    : nil;
                bound_grid_validate_pipeline = validate_function
                    ? [device newComputePipelineStateWithFunction:validate_function error:&ns_error]
                    : nil;
                if (!bound_grid_profile_pipeline || !bound_grid_validate_pipeline) {
                    set_error(error, error_len,
                              "failed to create bound-grid profiling pipelines: %s",
                              ns_error.localizedDescription.UTF8String);
                    return 1;
                }
            }
        }
        if (use_regional) {
            if (config->sdf_id != FPT_SDF_PROGRAM || config->sdf_program_count == 0u) {
                set_error(error, error_len,
                          "regional renderer requires a typed sdf_program");
                return 1;
            }
            if (config->regional_program_resolution != 16u &&
                config->regional_program_resolution != 32u) {
                set_error(error, error_len,
                          "regional program resolution must be 16 or 32");
                return 1;
            }
            id<MTLFunction> regional_build_function =
                [library newFunctionWithName:@"regional_program_build_kernel"];
            regional_program_build_pipeline = regional_build_function
                ? [device newComputePipelineStateWithFunction:regional_build_function
                                                        error:&ns_error]
                : nil;
            if (!regional_program_build_pipeline) {
                set_error(error, error_len,
                          "failed to create regional program build pipeline: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
            if (config->bound_grid_profile != 0u) {
                id<MTLFunction> regional_validate_function =
                    [library newFunctionWithName:@"regional_program_validate_kernel"];
                id<MTLFunction> regional_profile_function =
                    [library newFunctionWithName:@"regional_program_profile_kernel"];
                regional_program_validate_pipeline = regional_validate_function
                    ? [device newComputePipelineStateWithFunction:
                                  regional_validate_function error:&ns_error]
                    : nil;
                regional_program_profile_pipeline = regional_profile_function
                    ? [device newComputePipelineStateWithFunction:
                                  regional_profile_function error:&ns_error]
                    : nil;
                if (!regional_program_validate_pipeline ||
                    !regional_program_profile_pipeline) {
                    set_error(error, error_len,
                              "failed to create regional profiling pipelines: %s",
                              ns_error.localizedDescription.UTF8String);
                    return 1;
                }
            }
        }
        id<MTLComputePipelineState> focus_pipeline = nil;
        if (config->focus_distance <= 0.0f) {
            NSString *focus_function_name = use_voxels
                ? @"estimate_voxel_focus_distance_kernel"
                : (use_regional
                    ? @"estimate_regional_focus_distance_kernel"
                    : (use_bound_grid
                    ? @"estimate_bound_grid_focus_distance_kernel"
                    : @"estimate_focus_distance_kernel"));
            id<MTLFunction> focus_function =
                [render_library newFunctionWithName:focus_function_name];
            if (!focus_function) {
                set_error(error, error_len, "estimate_focus_distance_kernel not found in metallib");
                return 1;
            }
            focus_pipeline = new_compute_pipeline(
                device, focus_function, stitched_private_functions,
                stitch_binary_archive, populate_stitch_binary_archive,
                &ns_error);
            if (!focus_pipeline) {
                set_error(error, error_len, "failed to create focus pipeline: %s", ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        id<MTLComputePipelineState> stitch_validation_pipeline = nil;
        if (config->sdf_stitch_validation != 0u) {
            NSString *validation_function_name =
                config->sdf_topology_specialization != 0u && !tiny_linked_helper
                    ? @"topology_validation_kernel"
                    : @"stitched_validation_kernel";
            id<MTLFunction> validation_function = [render_library
                newFunctionWithName:validation_function_name];
            NSArray<id<MTLFunction>> *validation_private_functions =
                config->sdf_topology_specialization != 0u && !tiny_linked_helper
                    ? @[] : stitched_private_functions;
            stitch_validation_pipeline = validation_function
                ? new_compute_pipeline(
                    device, validation_function, validation_private_functions,
                    config->sdf_topology_specialization != 0u && !tiny_linked_helper
                        ? nil : stitch_binary_archive,
                    config->sdf_topology_specialization != 0u && !tiny_linked_helper
                        ? false : populate_stitch_binary_archive,
                    &ns_error)
                : nil;
            if (!stitch_validation_pipeline) {
                set_error(error, error_len,
                          "failed to create program validation pipeline: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        if (populate_stitch_binary_archive && stitch_binary_archive &&
            stitch_archive_path && stitch_archive_path[0] != '\0') {
            NSURL *archive_url = [NSURL fileURLWithPath:
                ns_string(stitch_archive_path)];
            if (![stitch_binary_archive serializeToURL:archive_url
                                                 error:&ns_error]) {
                set_error(error, error_len,
                          "failed to serialize stitched pipeline archive: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        if (runtime_source_build_start) {
            runtime_source_build_elapsed_ms =
                -[runtime_source_build_start timeIntervalSinceNow] * 1000.0;
        }
        id<MTLCommandQueue> queue = [device newCommandQueue];
        if (!queue) {
            set_error(error, error_len, "failed to create Metal command queue");
            return 1;
        }

        const size_t pixel_count = static_cast<size_t>(config->width) * config->height;
        const size_t linear_component_count = pixel_count * 4u;
        if (linear_output && linear_output_len < linear_component_count) {
            set_error(error, error_len,
                      "linear output buffer is too small (%zu < %zu)",
                      linear_output_len, linear_component_count);
            return 1;
        }
        id<MTLBuffer> out_buffer = [device newBufferWithLength:pixel_count * 4 options:MTLResourceStorageModeShared];
        id<MTLBuffer> cfg_buffer = [device newBufferWithBytes:config length:sizeof(FptRenderConfig) options:MTLResourceStorageModeShared];
        id<MTLBuffer> accum_buffer = [device newBufferWithLength:pixel_count * sizeof(float) * 4 options:MTLResourceStorageModePrivate];
        id<MTLBuffer> linear_output_buffer = linear_output
            ? [device newBufferWithLength:linear_component_count * sizeof(float)
                                  options:MTLResourceStorageModeShared]
            : nil;
        const size_t voxel_count = use_voxels
            ? static_cast<size_t>(config->voxel_resolution) * config->voxel_resolution * config->voxel_resolution
            : 0u;
        const bool use_direct_voxel_build = use_voxels &&
            config->voxel_build_mode == FPT_VOXEL_BUILD_DIRECT;
        id<MTLBuffer> voxel_buffer = use_voxels && !use_direct_voxel_build
            ? [device newBufferWithLength:voxel_count * 12u options:MTLResourceStorageModeShared]
            : nil;
        id<MTLBuffer> voxel_page_table_buffer = nil;
        id<MTLBuffer> regional_mask_buffer = nil;
        id<MTLBuffer> regional_program_id_buffer = nil;
        id<MTLBuffer> regional_program_header_buffer = nil;
        id<MTLBuffer> regional_instruction_pool_buffer = nil;
        id<MTLBuffer> regional_primitive_pool_buffer = nil;
        id<MTLBuffer> regional_validation_buffer = nil;
        id<MTLBuffer> regional_proof_buffer = nil;
        id<MTLBuffer> regional_profile_buffer = nil;
        id<MTLTexture> bound_grid_texture = nil;
        id<MTLTexture> derivative_lower_grid_texture = nil;
        id<MTLTexture> derivative_upper_grid_texture = nil;
        if (use_bound_grid) {
            const NSUInteger resolution = config->bound_grid_resolution;
            MTLTextureDescriptor *descriptor = [MTLTextureDescriptor new];
            descriptor.textureType = MTLTextureType3D;
            descriptor.pixelFormat = config->bound_grid_fp16 != 0u
                ? MTLPixelFormatRGBA16Float : MTLPixelFormatRG32Float;
            descriptor.width = resolution;
            descriptor.height = resolution;
            descriptor.depth = resolution;
            descriptor.mipmapLevelCount = 1u;
            descriptor.storageMode = MTLStorageModePrivate;
            descriptor.usage = MTLTextureUsageShaderRead | MTLTextureUsageShaderWrite;
            bound_grid_texture = [device newTextureWithDescriptor:descriptor];

            MTLTextureDescriptor *derivative_descriptor = [MTLTextureDescriptor new];
            derivative_descriptor.textureType = MTLTextureType3D;
            derivative_descriptor.pixelFormat = config->bound_grid_fp16 != 0u
                ? MTLPixelFormatRGBA16Float : MTLPixelFormatRGBA32Float;
            const NSUInteger derivative_resolution = config->bound_grid_directional != 0u
                ? resolution : 1u;
            derivative_descriptor.width = derivative_resolution;
            derivative_descriptor.height = derivative_resolution;
            derivative_descriptor.depth = derivative_resolution;
            derivative_descriptor.mipmapLevelCount = 1u;
            derivative_descriptor.storageMode = MTLStorageModePrivate;
            derivative_descriptor.usage = MTLTextureUsageShaderRead |
                                          MTLTextureUsageShaderWrite;
            derivative_lower_grid_texture =
                [device newTextureWithDescriptor:derivative_descriptor];
            if (config->bound_grid_fp16 != 0u) {
                derivative_descriptor.width = 1u;
                derivative_descriptor.height = 1u;
                derivative_descriptor.depth = 1u;
            }
            derivative_upper_grid_texture =
                [device newTextureWithDescriptor:derivative_descriptor];
        }
        if (use_regional) {
            const size_t resolution = config->regional_program_resolution;
            const size_t cell_count = resolution * resolution * resolution;
            regional_mask_buffer =
                [device newBufferWithLength:cell_count * sizeof(uint64_t)
                                    options:MTLResourceStorageModeShared];
            const size_t proof_count = config->bound_grid_profile != 0u
                ? cell_count : 1u;
            regional_proof_buffer =
                [device newBufferWithLength:proof_count *
                                            sizeof(RegionalProgramProofCpp)
                                    options:MTLResourceStorageModeShared];
            if (config->bound_grid_profile != 0u) {
                regional_validation_buffer = [device
                    newBufferWithLength:sizeof(RegionalProgramValidationCountsCpp)
                                options:MTLResourceStorageModeShared];
                regional_profile_buffer = [device
                    newBufferWithLength:pixel_count *
                                        sizeof(RegionalProgramLocalStatsCpp)
                                options:MTLResourceStorageModeShared];
            }
        }
        id<MTLBuffer> bound_grid_profile_buffer =
            use_bound_grid && config->bound_grid_profile != 0u
                ? [device newBufferWithLength:pixel_count * sizeof(BoundGridLocalStatsCpp)
                                      options:MTLResourceStorageModeShared]
                : nil;
        id<MTLBuffer> bound_grid_validation_buffer =
            use_bound_grid && config->bound_grid_profile != 0u
                ? [device newBufferWithLength:sizeof(BoundGridValidationCountsCpp)
                                      options:MTLResourceStorageModeShared]
                : nil;
        id<MTLBuffer> focus_buffer = focus_pipeline ? [device newBufferWithLength:sizeof(float) options:MTLResourceStorageModeShared] : nil;
        id<MTLBuffer> sdf_profile_buffer = sdf_profile_pipeline
            ? [device newBufferWithLength:sizeof(FptSdfProfileCountsCpp)
                                  options:MTLResourceStorageModeShared]
            : nil;
        if (!out_buffer || !cfg_buffer) {
            set_error(error, error_len, "failed to allocate Metal buffers");
            return 1;
        }
        if (!accum_buffer) {
            set_error(error, error_len, "failed to allocate Metal accumulation buffer");
            return 1;
        }
        if (use_voxels && !use_direct_voxel_build && !voxel_buffer) {
            set_error(error, error_len, "failed to allocate voxel field buffer");
            return 1;
        }
        if (use_bound_grid && (!bound_grid_texture ||
                               !derivative_lower_grid_texture ||
                               !derivative_upper_grid_texture)) {
            set_error(error, error_len, "failed to allocate bound-grid texture");
            return 1;
        }
        if (use_regional && (!regional_mask_buffer || !regional_proof_buffer ||
            (config->bound_grid_profile != 0u &&
             (!regional_validation_buffer || !regional_profile_buffer)))) {
            set_error(error, error_len, "failed to allocate regional program grid");
            return 1;
        }
        if (use_bound_grid && config->bound_grid_profile != 0u &&
            (!bound_grid_profile_buffer || !bound_grid_validation_buffer)) {
            set_error(error, error_len, "failed to allocate bound-grid profile buffers");
            return 1;
        }
        if (focus_pipeline && !focus_buffer) {
            set_error(error, error_len, "failed to allocate focus-distance buffer");
            return 1;
        }
        if (sdf_profile_pipeline && !sdf_profile_buffer) {
            set_error(error, error_len, "failed to allocate SDF profile buffer");
            return 1;
        }

        if (stitch_validation_pipeline) {
            struct StitchValidationCountsCpp {
                uint32_t distance_failures;
                uint32_t gradient_failures;
                uint32_t max_distance_error_bits;
                uint32_t max_gradient_error_bits;
            } initial_counts = {};
            id<MTLBuffer> validation_buffer = [device
                newBufferWithBytes:&initial_counts
                            length:sizeof(initial_counts)
                           options:MTLResourceStorageModeShared];
            if (!validation_buffer) {
                set_error(error, error_len,
                          "failed to allocate program validation buffer");
                return 1;
            }
            id<MTLCommandBuffer> validation_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> validation_encoder =
                [validation_command computeCommandEncoder];
            [validation_encoder setComputePipelineState:stitch_validation_pipeline];
            [validation_encoder setBuffer:validation_buffer offset:0 atIndex:0];
            [validation_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            constexpr NSUInteger sample_count = 1u << 20u;
            NSUInteger group_width = std::min<NSUInteger>(
                stitch_validation_pipeline.maxTotalThreadsPerThreadgroup, 256u);
            [validation_encoder dispatchThreads:MTLSizeMake(sample_count, 1u, 1u)
                           threadsPerThreadgroup:MTLSizeMake(group_width, 1u, 1u)];
            [validation_encoder endEncoding];
            [validation_command commit];
            [validation_command waitUntilCompleted];
            if (validation_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "program validation failed: %s",
                          validation_command.error.localizedDescription.UTF8String);
                return 1;
            }
            const auto *counts = static_cast<const StitchValidationCountsCpp *>(
                validation_buffer.contents);
            float max_distance_error = 0.0f;
            float max_gradient_error = 0.0f;
            std::memcpy(&max_distance_error, &counts->max_distance_error_bits,
                        sizeof(float));
            std::memcpy(&max_gradient_error, &counts->max_gradient_error_bits,
                        sizeof(float));
            if (stitch_validation_stats) {
                stitch_validation_stats->sample_count = sample_count;
                stitch_validation_stats->distance_failures =
                    counts->distance_failures;
                stitch_validation_stats->gradient_failures =
                    counts->gradient_failures;
                stitch_validation_stats->max_distance_error = max_distance_error;
                stitch_validation_stats->max_gradient_error = max_gradient_error;
            }
            if (counts->distance_failures != 0u ||
                counts->gradient_failures != 0u) {
                set_error(error, error_len,
                          "stitch validation found %u distance and %u gradient "
                          "failures (max errors %.9g, %.9g)",
                          counts->distance_failures, counts->gradient_failures,
                          max_distance_error, max_gradient_error);
                return 1;
            }
        }

        if (build_ms) *build_ms = runtime_source_build_elapsed_ms;
        if (use_regional) {
            NSDate *build_start = [NSDate date];
            id<MTLCommandBuffer> build_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> build_encoder =
                [build_command computeCommandEncoder];
            [build_encoder setComputePipelineState:regional_program_build_pipeline];
            [build_encoder setBuffer:regional_mask_buffer offset:0 atIndex:0];
            [build_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            [build_encoder setBuffer:regional_proof_buffer offset:0 atIndex:2];
            const NSUInteger resolution = config->regional_program_resolution;
            [build_encoder dispatchThreads:MTLSizeMake(resolution, resolution,
                                                       resolution)
                     threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
            [build_encoder endEncoding];
            [build_command commit];
            [build_command waitUntilCompleted];
            if (build_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "regional program build failed: %s",
                          build_command.error.localizedDescription.UTF8String);
                return 1;
            }
            const size_t cell_count = static_cast<size_t>(resolution) * resolution *
                                      resolution;
            const auto *masks = static_cast<const uint64_t *>(
                regional_mask_buffer.contents);
            uint64_t full_mask = 0u;
            const uint32_t instruction_count =
                std::min<uint32_t>(config->sdf_program_count,
                                   FPT_SDF_PROGRAM_MAX_OPS);
            for (uint32_t instruction = 0u; instruction < instruction_count;
                 ++instruction) {
                if (config->sdf_program[instruction].opcode != FPT_SDF_OP_ORBIT_ADD) {
                    full_mask |= uint64_t{1} << instruction;
                }
            }
            std::unordered_map<uint64_t, uint16_t> program_ids_by_mask;
            std::vector<uint16_t> program_ids(cell_count);
            std::vector<RegionalProgramHeaderCpp> program_headers;
            std::vector<FptSdfInstruction> instruction_pool;
            std::vector<uint16_t> primitive_pool;
            const bool use_flat_union_programs =
                config->sdf_flat_union_count > 0u;
            uint64_t fallback = 0u;
            uint64_t retained = 0u;
            for (size_t cell = 0u; cell < cell_count; ++cell) {
                const uint64_t mask = masks[cell];
                auto found = program_ids_by_mask.find(mask);
                uint16_t program_id = 0u;
                if (found == program_ids_by_mask.end()) {
                    if (program_headers.size() >= UINT16_MAX) {
                        set_error(error, error_len,
                                  "regional program count exceeds ushort capacity");
                        return 1;
                    }
                    program_id = static_cast<uint16_t>(program_headers.size());
                    RegionalProgramHeaderCpp header = {
                        static_cast<uint32_t>(instruction_pool.size()), 0u,
                        static_cast<uint32_t>(primitive_pool.size()), 0u};
                    if (use_flat_union_programs) {
                        const uint32_t primitive_count = std::min<uint32_t>(
                            config->sdf_flat_union_count,
                            FPT_SDF_FLAT_UNION_MAX_PRIMITIVES);
                        for (uint32_t primitive = 0u;
                             primitive < primitive_count; ++primitive) {
                            const uint32_t source_instruction =
                                config->sdf_flat_union_instances[primitive]
                                    .source_instruction;
                            if (source_instruction < 64u &&
                                (mask & (uint64_t{1} << source_instruction)) !=
                                    0u) {
                                primitive_pool.push_back(
                                    static_cast<uint16_t>(primitive));
                            }
                        }
                        header.primitive_count = static_cast<uint32_t>(
                            primitive_pool.size() - header.primitive_offset);
                    } else {
                        std::vector<FptSdfInstruction> regional_instructions;
                        for (uint32_t instruction = 0u;
                             instruction < instruction_count; ++instruction) {
                            if ((mask & (uint64_t{1} << instruction)) != 0u) {
                                FptSdfInstruction current =
                                    config->sdf_program[instruction];
                                if (current.opcode == FPT_SDF_OP_TRANSLATE &&
                                    !regional_instructions.empty() &&
                                    regional_instructions.back().opcode ==
                                        FPT_SDF_OP_TRANSLATE &&
                                    regional_instructions.back().flags ==
                                        current.flags &&
                                    regional_instructions.back().material_index ==
                                        current.material_index) {
                                    FptSdfInstruction &previous =
                                        regional_instructions.back();
                                    previous.data[0] += current.data[0];
                                    previous.data[1] += current.data[1];
                                    previous.data[2] += current.data[2];
                                    if (previous.data[0] == 0.0f &&
                                        previous.data[1] == 0.0f &&
                                        previous.data[2] == 0.0f) {
                                        regional_instructions.pop_back();
                                    }
                                } else {
                                    regional_instructions.push_back(current);
                                }
                            }
                        }
                        header.instruction_count = static_cast<uint32_t>(
                            regional_instructions.size());
                        instruction_pool.insert(instruction_pool.end(),
                                                regional_instructions.begin(),
                                                regional_instructions.end());
                    }
                    program_headers.push_back(header);
                    program_ids_by_mask.emplace(mask, program_id);
                } else {
                    program_id = found->second;
                }
                program_ids[cell] = program_id;
                retained += use_flat_union_programs
                    ? program_headers[program_id].primitive_count
                    : program_headers[program_id].instruction_count;
                fallback += mask == full_mask ? 1u : 0u;
            }
            regional_program_id_buffer =
                [device newBufferWithBytes:program_ids.data()
                                    length:program_ids.size() * sizeof(uint16_t)
                                   options:MTLResourceStorageModeShared];
            regional_program_header_buffer =
                [device newBufferWithBytes:program_headers.data()
                                    length:program_headers.size() *
                                           sizeof(RegionalProgramHeaderCpp)
                                   options:MTLResourceStorageModeShared];
            if (instruction_pool.empty()) {
                instruction_pool.push_back(FptSdfInstruction{});
            }
            regional_instruction_pool_buffer =
                [device newBufferWithBytes:instruction_pool.data()
                                   length:instruction_pool.size() *
                                           sizeof(FptSdfInstruction)
                                   options:MTLResourceStorageModeShared];
            if (primitive_pool.empty()) primitive_pool.push_back(0u);
            regional_primitive_pool_buffer =
                [device newBufferWithBytes:primitive_pool.data()
                                    length:primitive_pool.size() * sizeof(uint16_t)
                                   options:MTLResourceStorageModeShared];
            if (!regional_program_id_buffer || !regional_program_header_buffer ||
                !regional_instruction_pool_buffer ||
                !regional_primitive_pool_buffer) {
                set_error(error, error_len,
                          "failed to allocate compact regional program buffers");
                return 1;
            }
            if (build_ms) *build_ms = -[build_start timeIntervalSinceNow] * 1000.0;
            if (voxel_memory_bytes) {
                *voxel_memory_bytes = program_ids.size() * sizeof(uint16_t) +
                    program_headers.size() * sizeof(RegionalProgramHeaderCpp) +
                    instruction_pool.size() * sizeof(FptSdfInstruction) +
                    primitive_pool.size() * sizeof(uint16_t);
            }
            if (bound_grid_stats) {
                bound_grid_stats->regional_cells = cell_count;
                bound_grid_stats->regional_fallback_cells = fallback;
                bound_grid_stats->regional_unique_programs = program_headers.size();
                bound_grid_stats->regional_retained_instructions = retained;
                if (config->bound_grid_profile != 0u) {
                    const auto *proofs =
                        static_cast<const RegionalProgramProofCpp *>(
                            regional_proof_buffer.contents);
                    for (size_t cell = 0u; cell < cell_count; ++cell) {
                        const uint32_t flags = proofs[cell].flags;
                        if ((flags & RegionalProofStrictDominanceCpp) != 0u) {
                            bound_grid_stats->regional_pruned_cells++;
                        }
                        bound_grid_stats->regional_pruned_primitives +=
                            proofs[cell].pruned_primitives;
                        if ((flags & RegionalProofRepeatSeamFallbackCpp) != 0u) {
                            bound_grid_stats->regional_repeat_seam_fallback_cells++;
                        }
                        if ((flags & (RegionalProofUnsupportedFallbackCpp |
                                      RegionalProofInvalidPrimitiveFallbackCpp |
                                      RegionalProofNoPrimitiveFallbackCpp)) != 0u) {
                            bound_grid_stats->regional_unsupported_fallback_cells++;
                        }
                        if ((flags & RegionalProofNoDominanceCpp) != 0u) {
                            bound_grid_stats->regional_no_dominance_cells++;
                        }
                    }
                }
            }
            if (config->bound_grid_profile != 0u) {
                std::memset(regional_validation_buffer.contents, 0,
                            sizeof(RegionalProgramValidationCountsCpp));
                id<MTLCommandBuffer> validation_command = [queue commandBuffer];
                id<MTLComputeCommandEncoder> validation_encoder =
                    [validation_command computeCommandEncoder];
                [validation_encoder setComputePipelineState:
                    regional_program_validate_pipeline];
                [validation_encoder setBuffer:regional_validation_buffer
                                        offset:0 atIndex:0];
                [validation_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                [validation_encoder setBuffer:regional_program_id_buffer
                                        offset:0 atIndex:2];
                [validation_encoder setBuffer:regional_program_header_buffer
                                        offset:0 atIndex:3];
                [validation_encoder setBuffer:regional_instruction_pool_buffer
                                        offset:0 atIndex:4];
                [validation_encoder setBuffer:regional_primitive_pool_buffer
                                        offset:0 atIndex:5];
                [validation_encoder dispatchThreads:
                    MTLSizeMake(resolution, resolution, resolution)
                         threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
                [validation_encoder endEncoding];
                [validation_command commit];
                [validation_command waitUntilCompleted];
                if (validation_command.status == MTLCommandBufferStatusError) {
                    set_error(error, error_len,
                              "regional program validation failed: %s",
                              validation_command.error.localizedDescription.UTF8String);
                    return 1;
                }
                if (bound_grid_stats) {
                    const auto *validation = static_cast<const
                        RegionalProgramValidationCountsCpp *>(
                            regional_validation_buffer.contents);
                    bound_grid_stats->regional_sampled_distance_failures =
                        validation->sampled_distance_failures;
                }
            }
        }
        if (use_bound_grid) {
            NSDate *build_start = [NSDate date];
            id<MTLCommandBuffer> build_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> build_encoder = [build_command computeCommandEncoder];
            [build_encoder setComputePipelineState:bound_grid_build_pipeline];
            [build_encoder setTexture:bound_grid_texture atIndex:0];
            if (config->bound_grid_directional != 0u) {
                [build_encoder setTexture:derivative_lower_grid_texture atIndex:1];
                [build_encoder setTexture:derivative_upper_grid_texture atIndex:2];
            }
            [build_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            const NSUInteger resolution = config->bound_grid_resolution;
            [build_encoder dispatchThreads:MTLSizeMake(resolution, resolution, resolution)
                     threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
            [build_encoder endEncoding];
            [build_command commit];
            [build_command waitUntilCompleted];
            if (build_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "Metal bound-grid build failed: %s",
                          build_command.error.localizedDescription.UTF8String);
                return 1;
            }
            if (build_ms) *build_ms = -[build_start timeIntervalSinceNow] * 1000.0;
            if (voxel_memory_bytes) {
                const uint64_t bytes_per_cell = config->bound_grid_directional == 0u
                    ? 8u : (config->bound_grid_fp16 != 0u ? 16u : 40u);
                *voxel_memory_bytes = static_cast<uint64_t>(resolution) * resolution *
                                      resolution * bytes_per_cell;
            }
            if (config->bound_grid_profile != 0u) {
                std::memset(bound_grid_validation_buffer.contents, 0,
                            sizeof(BoundGridValidationCountsCpp));
                id<MTLCommandBuffer> validate_command = [queue commandBuffer];
                id<MTLComputeCommandEncoder> validate_encoder =
                    [validate_command computeCommandEncoder];
                [validate_encoder setComputePipelineState:bound_grid_validate_pipeline];
                [validate_encoder setBuffer:bound_grid_validation_buffer offset:0 atIndex:0];
                [validate_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                [validate_encoder setTexture:bound_grid_texture atIndex:0];
                [validate_encoder setTexture:derivative_lower_grid_texture atIndex:1];
                [validate_encoder setTexture:derivative_upper_grid_texture atIndex:2];
                [validate_encoder dispatchThreads:MTLSizeMake(resolution, resolution, resolution)
                           threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
                [validate_encoder endEncoding];
                [validate_command commit];
                [validate_command waitUntilCompleted];
                if (validate_command.status == MTLCommandBufferStatusError) {
                    set_error(error, error_len, "bound-grid validation failed: %s",
                              validate_command.error.localizedDescription.UTF8String);
                    return 1;
                }
                if (bound_grid_stats) {
                    const auto *validation = static_cast<const BoundGridValidationCountsCpp *>(
                        bound_grid_validation_buffer.contents);
                    bound_grid_stats->certified_cells = validation->certified_cells;
                    bound_grid_stats->unknown_cells = validation->unknown_cells;
                    bound_grid_stats->sampled_bound_failures =
                        validation->sampled_bound_failures;
                    bound_grid_stats->sampled_false_skips =
                        validation->sampled_false_skips;
                    bound_grid_stats->certified_derivative_cells =
                        validation->certified_derivative_cells;
                    bound_grid_stats->unknown_derivative_cells =
                        validation->unknown_derivative_cells;
                    bound_grid_stats->sampled_derivative_failures =
                        validation->sampled_derivative_failures;
                }
            }
        }
        if (use_voxels) {
            NSDate *voxel_build_start = [NSDate date];
            VoxelStorageResult storage;
            if (use_direct_voxel_build) {
                storage = build_direct_voxel_storage(device, queue, direct_voxel_build_pipeline,
                                                     cfg_buffer, *config);
            }
            if (!use_direct_voxel_build || storage.overflow) {
                voxel_buffer = [device newBufferWithLength:voxel_count * 12u
                                                   options:MTLResourceStorageModeShared];
                if (!voxel_buffer) {
                    set_error(error, error_len, "failed to allocate voxel staging fallback");
                    return 1;
                }
                id<MTLCommandBuffer> voxel_build_command = [queue commandBuffer];
                id<MTLComputeCommandEncoder> voxel_build_encoder = [voxel_build_command computeCommandEncoder];
                [voxel_build_encoder setComputePipelineState:voxel_build_pipeline];
                [voxel_build_encoder setBuffer:voxel_buffer offset:0 atIndex:0];
                [voxel_build_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                MTLSize voxel_grid = MTLSizeMake(config->voxel_resolution,
                                                 config->voxel_resolution,
                                                 config->voxel_resolution);
                [voxel_build_encoder dispatchThreads:voxel_grid threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
                [voxel_build_encoder endEncoding];
                [voxel_build_command commit];
                [voxel_build_command waitUntilCompleted];
                if (voxel_build_command.status == MTLCommandBufferStatusError) {
                    set_error(error, error_len, "Metal voxel build failed: %s",
                              voxel_build_command.error.localizedDescription.UTF8String);
                    return 1;
                }
                storage = finalize_voxel_storage(device, *config, voxel_buffer);
            }
            if (!storage.cells || !storage.page_table) {
                set_error(error, error_len, "failed to finalize voxel storage");
                return 1;
            }
            voxel_buffer = storage.cells;
            voxel_page_table_buffer = storage.page_table;
            if (voxel_memory_bytes) *voxel_memory_bytes = storage.resident_bytes;
            if (voxel_active_bricks) *voxel_active_bricks = storage.active_bricks;
            if (voxel_active_cells) *voxel_active_cells = storage.active_cells;
            if (voxel_rejected_bricks) *voxel_rejected_bricks = storage.rejected_bricks;
            if (build_ms) *build_ms = -[voxel_build_start timeIntervalSinceNow] * 1000.0;
        }

        NSDate *start_time = [NSDate date];
        if (focus_pipeline) {
            id<MTLCommandBuffer> focus_command_buffer = [queue commandBuffer];
            id<MTLComputeCommandEncoder> focus_encoder = [focus_command_buffer computeCommandEncoder];
            [focus_encoder setComputePipelineState:focus_pipeline];
            [focus_encoder setBuffer:focus_buffer offset:0 atIndex:0];
            [focus_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            if (use_voxels) [focus_encoder setBuffer:voxel_buffer offset:0 atIndex:2];
            if (use_voxels) [focus_encoder setBuffer:voxel_page_table_buffer offset:0 atIndex:3];
            if (use_regional) {
                [focus_encoder setBuffer:regional_program_id_buffer offset:0 atIndex:2];
                [focus_encoder setBuffer:regional_program_header_buffer offset:0 atIndex:3];
                [focus_encoder setBuffer:regional_instruction_pool_buffer offset:0 atIndex:4];
                [focus_encoder setBuffer:regional_primitive_pool_buffer offset:0 atIndex:5];
            }
            if (use_bound_grid) [focus_encoder setTexture:bound_grid_texture atIndex:0];
            if (use_bound_grid) [focus_encoder setTexture:derivative_lower_grid_texture atIndex:1];
            if (use_bound_grid) [focus_encoder setTexture:derivative_upper_grid_texture atIndex:2];
            [focus_encoder dispatchThreads:MTLSizeMake(1u, 1u, 1u) threadsPerThreadgroup:MTLSizeMake(1u, 1u, 1u)];
            [focus_encoder endEncoding];
            [focus_command_buffer commit];
            [focus_command_buffer waitUntilCompleted];
            if (focus_command_buffer.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "focus-distance prepass failed: %s", focus_command_buffer.error.localizedDescription.UTF8String);
                return 1;
            }
            hydrated_config.focus_distance = *static_cast<const float *>(focus_buffer.contents);
            std::memcpy(cfg_buffer.contents, &hydrated_config, sizeof(FptRenderConfig));
        }
        id<MTLCommandBuffer> command_buffer = [queue commandBuffer];
        MTLSize threads_per_group = threadgroup_for_pipeline(main_pipeline);
        uint32_t dispatch_width = config->width;
        uint32_t dispatch_height = config->height;
        interactive_preview_dispatch_extent(*config, dispatch_width, dispatch_height);
        MTLSize groups = groups_for_extent(dispatch_width, dispatch_height, threads_per_group);
        id<MTLComputeCommandEncoder> encoder = nil;
        if (use_tiled_accumulation) {
            uint32_t rows_per_dispatch = 32u;
            if (const char *configured_rows = std::getenv("FPT_MANDEL_TILE_ROWS")) {
                char *end = nullptr;
                const unsigned long parsed = std::strtoul(configured_rows, &end, 10);
                if (end != configured_rows && *end == '\0') {
                    rows_per_dispatch = std::clamp<uint32_t>(
                        static_cast<uint32_t>(parsed), 1u, 32u);
                }
            }
            for (uint32_t row = 0u; row < config->height;
                 row += rows_per_dispatch) {
                FptAccumulationTileCpp tile = {{0u, row}};
                const uint32_t tile_height =
                    std::min(rows_per_dispatch, config->height - row);
                MTLSize tile_groups = groups_for_extent(
                    config->width, tile_height, threads_per_group);
                id<MTLCommandBuffer> tile_buffer = [queue commandBuffer];
                encoder = [tile_buffer computeCommandEncoder];
                [encoder setComputePipelineState:main_pipeline];
                [encoder setBuffer:accum_buffer offset:0 atIndex:0];
                [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                [encoder setBytes:&tile length:sizeof(tile) atIndex:2];
                [encoder dispatchThreadgroups:tile_groups
                         threadsPerThreadgroup:threads_per_group];
                [encoder endEncoding];
                [tile_buffer commit];
                [tile_buffer waitUntilCompleted];
                if (tile_buffer.status == MTLCommandBufferStatusError) {
                    set_error(error, error_len,
                              "Metal accumulation tile failed at row %u: %s",
                              row,
                              tile_buffer.error.localizedDescription.UTF8String);
                    return 1;
                }
            }
            command_buffer = [queue commandBuffer];
        } else if (use_chunked_accumulation) {
            const uint32_t chunk_samples = std::clamp<uint32_t>(config->sdf_chunk_samples, 1u, 64u);
            for (uint32_t start = 0u; start < requested_samples; start += chunk_samples) {
                FptAccumulationChunkCpp chunk = {start, std::min(chunk_samples, requested_samples - start)};
                id<MTLCommandBuffer> chunk_buffer = [queue commandBuffer];
                encoder = [chunk_buffer computeCommandEncoder];
                [encoder setComputePipelineState:main_pipeline];
                [encoder setBuffer:accum_buffer offset:0 atIndex:0];
                [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                [encoder setBytes:&chunk length:sizeof(chunk) atIndex:2];
                if (use_voxels) [encoder setBuffer:voxel_buffer offset:0 atIndex:3];
                if (use_voxels) [encoder setBuffer:voxel_page_table_buffer offset:0 atIndex:4];
                if (use_regional) {
                    [encoder setBuffer:regional_program_id_buffer offset:0 atIndex:3];
                    [encoder setBuffer:regional_program_header_buffer offset:0 atIndex:4];
                    [encoder setBuffer:regional_instruction_pool_buffer offset:0 atIndex:5];
                    [encoder setBuffer:regional_primitive_pool_buffer offset:0 atIndex:6];
                }
                if (use_bound_grid) [encoder setTexture:bound_grid_texture atIndex:0];
                if (use_bound_grid) [encoder setTexture:derivative_lower_grid_texture atIndex:1];
                if (use_bound_grid) [encoder setTexture:derivative_upper_grid_texture atIndex:2];
                [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
                [encoder endEncoding];
                [chunk_buffer commit];
                [chunk_buffer waitUntilCompleted];
                if (chunk_buffer.status == MTLCommandBufferStatusError) {
                    set_error(error, error_len, "Metal accumulation chunk failed: %s", chunk_buffer.error.localizedDescription.UTF8String);
                    return 1;
                }
            }
            command_buffer = [queue commandBuffer];
        } else if (use_batch_accumulation || config->preview) {
            encoder = [command_buffer computeCommandEncoder];
            [encoder setComputePipelineState:main_pipeline];
            [encoder setBuffer:accum_buffer offset:0 atIndex:0];
            [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            if (use_voxels) [encoder setBuffer:voxel_buffer offset:0 atIndex:3];
            if (use_voxels) [encoder setBuffer:voxel_page_table_buffer offset:0 atIndex:4];
            if (use_regional) {
                [encoder setBuffer:regional_program_id_buffer offset:0 atIndex:3];
                [encoder setBuffer:regional_program_header_buffer offset:0 atIndex:4];
                [encoder setBuffer:regional_instruction_pool_buffer offset:0 atIndex:5];
                [encoder setBuffer:regional_primitive_pool_buffer offset:0 atIndex:6];
            }
            if (use_bound_grid) [encoder setTexture:bound_grid_texture atIndex:0];
            if (use_bound_grid) [encoder setTexture:derivative_lower_grid_texture atIndex:1];
            if (use_bound_grid) [encoder setTexture:derivative_upper_grid_texture atIndex:2];
            [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
            [encoder endEncoding];
        } else {
            for (uint32_t frame = 0; frame < requested_samples; ++frame) {
                encoder = [command_buffer computeCommandEncoder];
                [encoder setComputePipelineState:main_pipeline];
                [encoder setBuffer:accum_buffer offset:0 atIndex:0];
                [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                [encoder setBytes:&frame length:sizeof(frame) atIndex:2];
                if (use_voxels) [encoder setBuffer:voxel_buffer offset:0 atIndex:3];
                if (use_voxels) [encoder setBuffer:voxel_page_table_buffer offset:0 atIndex:4];
                if (use_regional) {
                    [encoder setBuffer:regional_program_id_buffer offset:0 atIndex:3];
                    [encoder setBuffer:regional_program_header_buffer offset:0 atIndex:4];
                    [encoder setBuffer:regional_instruction_pool_buffer offset:0 atIndex:5];
                    [encoder setBuffer:regional_primitive_pool_buffer offset:0 atIndex:6];
                }
                if (use_bound_grid) [encoder setTexture:bound_grid_texture atIndex:0];
                if (use_bound_grid) [encoder setTexture:derivative_lower_grid_texture atIndex:1];
                if (use_bound_grid) [encoder setTexture:derivative_upper_grid_texture atIndex:2];
                [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
                [encoder endEncoding];
            }
        }
        encoder = [command_buffer computeCommandEncoder];
        id<MTLBuffer> present_config_buffer = cfg_buffer;
        MTLSize present_groups = groups;
        if (config->preview != 0u && config->sdf_id == FPT_SDF_MANDELBULBER &&
            int32_t(std::round(config->vset_values[132])) != 0) {
            FptRenderConfig present_config = *config;
            present_config.vset_values[131] = 0.0f;
            present_config.vset_values[132] = 0.0f;
            present_config_buffer = [device
                newBufferWithBytes:&present_config
                            length:sizeof(present_config)
                           options:MTLResourceStorageModeShared];
            if (!present_config_buffer) {
                set_error(error, error_len,
                          "failed to allocate spatial preview presentation config");
                return 1;
            }
            present_groups = groups_for_extent(
                config->width, config->height, threads_per_group);
        }
        [encoder setComputePipelineState:present_pipeline];
        [encoder setBuffer:out_buffer offset:0 atIndex:0];
        [encoder setBuffer:accum_buffer offset:0 atIndex:1];
        [encoder setBuffer:present_config_buffer offset:0 atIndex:2];
        [encoder dispatchThreadgroups:present_groups
                 threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        [command_buffer commit];
        [command_buffer waitUntilCompleted];
        const double render_elapsed_ms =
            -[start_time timeIntervalSinceNow] * 1000.0;
        if (elapsed_ms) *elapsed_ms = render_elapsed_ms;
        if (command_buffer.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "Metal command buffer failed: %s", command_buffer.error.localizedDescription.UTF8String);
            return 1;
        }
        if (linear_output) {
            if (!linear_output_buffer) {
                set_error(error, error_len,
                          "failed to allocate linear probe output buffer");
                return 1;
            }
            id<MTLCommandBuffer> copy_command = [queue commandBuffer];
            id<MTLBlitCommandEncoder> blit = [copy_command blitCommandEncoder];
            [blit copyFromBuffer:accum_buffer sourceOffset:0
                       toBuffer:linear_output_buffer destinationOffset:0
                           size:linear_component_count * sizeof(float)];
            [blit endEncoding];
            [copy_command commit];
            [copy_command waitUntilCompleted];
            if (copy_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len,
                          "linear probe readback failed: %s",
                          copy_command.error.localizedDescription.UTF8String);
                return 1;
            }
            std::memcpy(linear_output, linear_output_buffer.contents,
                        linear_component_count * sizeof(float));
        }

        if (sdf_profile_pipeline && sdf_profile_buffer) {
            std::memset(sdf_profile_buffer.contents, 0,
                        sizeof(FptSdfProfileCountsCpp));
            FptSdfProfileConfigCpp profile = {0u, 4u, 0u, 0u};
            id<MTLCommandBuffer> profile_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> profile_encoder =
                [profile_command computeCommandEncoder];
            [profile_encoder setComputePipelineState:sdf_profile_pipeline];
            [profile_encoder setBuffer:sdf_profile_buffer offset:0 atIndex:0];
            [profile_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            [profile_encoder setBytes:&profile length:sizeof(profile) atIndex:2];
            MTLSize profile_threads =
                threadgroup_for_pipeline(sdf_profile_pipeline);
            MTLSize profile_groups = groups_for_extent(
                config->width, config->height, profile_threads);
            [profile_encoder dispatchThreadgroups:profile_groups
                             threadsPerThreadgroup:profile_threads];
            [profile_encoder endEncoding];
            [profile_command commit];
            [profile_command waitUntilCompleted];
            if (profile_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "SDF profile failed: %s",
                          profile_command.error.localizedDescription.UTF8String);
                return 1;
            }
            if (sdf_profile_stats) {
                const auto &counts = *static_cast<const FptSdfProfileCountsCpp *>(
                    sdf_profile_buffer.contents);
                sdf_profile_stats->primary_steps = counts.primary_steps;
                sdf_profile_stats->secondary_steps = counts.secondary_steps;
                sdf_profile_stats->shadow_steps = counts.shadow_steps;
                sdf_profile_stats->normal_evals = counts.normal_evals;
                sdf_profile_stats->bounces = counts.bounces;
                sdf_profile_stats->pixels = counts.pixels;
                sdf_profile_stats->distance_evals = counts.distance_evals;
                sdf_profile_stats->march_orbit_iterations =
                    counts.march_orbit_iterations;
                sdf_profile_stats->refinement_steps = counts.refinement_steps;
                sdf_profile_stats->normal_field_evals = counts.normal_field_evals;
                sdf_profile_stats->material_evals = counts.material_evals;
                sdf_profile_stats->max_ray_steps = counts.max_ray_steps;
                sdf_profile_stats->max_pixel_steps = counts.max_pixel_steps;
                for (uint32_t phase = 0u; phase < 4u; ++phase) {
                    sdf_profile_stats->distance_evals_by_phase[phase] =
                        counts.distance_evals_by_phase[phase];
                    sdf_profile_stats->orbit_iterations_by_phase[phase] =
                        counts.orbit_iterations_by_phase[phase];
                }
                for (uint32_t slot = 0u; slot < 9u; ++slot) {
                    sdf_profile_stats->formula_slot_iterations[slot] =
                        counts.formula_slot_iterations[slot];
                }
                sdf_profile_stats->refinement_distance_evals =
                    counts.refinement_distance_evals;
                const double units[5] = {
                    static_cast<double>(counts.primary_steps),
                    static_cast<double>(counts.secondary_steps),
                    static_cast<double>(counts.shadow_steps),
                    counts.normal_field_evals > 0u
                        ? static_cast<double>(counts.normal_field_evals)
                        : static_cast<double>(counts.normal_evals),
                    static_cast<double>(counts.bounces)};
                const double total = units[0] + units[1] + units[2] +
                                     units[3] + units[4];
                if (total > 0.0) {
                    sdf_profile_stats->primary_ms_estimate =
                        render_elapsed_ms * units[0] / total;
                    sdf_profile_stats->secondary_ms_estimate =
                        render_elapsed_ms * units[1] / total;
                    sdf_profile_stats->shadow_ms_estimate =
                        render_elapsed_ms * units[2] / total;
                    sdf_profile_stats->normal_ms_estimate =
                        render_elapsed_ms * units[3] / total;
                    sdf_profile_stats->bounce_ms_estimate =
                        render_elapsed_ms * units[4] / total;
                }
            }
        }

        if (use_regional && config->bound_grid_profile != 0u) {
            std::memset(regional_profile_buffer.contents, 0,
                        pixel_count * sizeof(RegionalProgramLocalStatsCpp));
            id<MTLCommandBuffer> profile_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> profile_encoder =
                [profile_command computeCommandEncoder];
            [profile_encoder setComputePipelineState:regional_program_profile_pipeline];
            [profile_encoder setBuffer:regional_profile_buffer offset:0 atIndex:0];
            [profile_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            [profile_encoder setBuffer:regional_program_id_buffer offset:0 atIndex:2];
            [profile_encoder setBuffer:regional_program_header_buffer offset:0 atIndex:3];
            [profile_encoder setBuffer:regional_instruction_pool_buffer offset:0 atIndex:4];
            [profile_encoder setBuffer:regional_primitive_pool_buffer offset:0 atIndex:5];
            MTLSize profile_threads =
                threadgroup_for_pipeline(regional_program_profile_pipeline);
            MTLSize profile_groups = groups_for_extent(config->width, config->height,
                                                       profile_threads);
            [profile_encoder dispatchThreadgroups:profile_groups
                             threadsPerThreadgroup:profile_threads];
            [profile_encoder endEncoding];
            [profile_command commit];
            [profile_command waitUntilCompleted];
            if (profile_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len,
                          "regional-program traversal profile failed: %s",
                          profile_command.error.localizedDescription.UTF8String);
                return 1;
            }
            if (bound_grid_stats) {
                const auto *profile =
                    static_cast<const RegionalProgramLocalStatsCpp *>(
                        regional_profile_buffer.contents);
                for (size_t pixel = 0u; pixel < pixel_count; ++pixel) {
                    for (uint32_t ray_class = 0u; ray_class < 3u; ++ray_class) {
                        bound_grid_stats->regional_distance_evaluations[ray_class] +=
                            profile[pixel].distance_evaluations[ray_class];
                        bound_grid_stats->regional_atlas_evaluations[ray_class] +=
                            profile[pixel].atlas_evaluations[ray_class];
                        bound_grid_stats->regional_full_program_evaluations[ray_class] +=
                            profile[pixel].full_program_evaluations[ray_class];
                        bound_grid_stats->regional_cell_entries[ray_class] +=
                            profile[pixel].cell_entries[ray_class];
                        bound_grid_stats->regional_same_cell_reuses[ray_class] +=
                            profile[pixel].same_cell_reuses[ray_class];
                        bound_grid_stats->regional_same_program_reuses[ray_class] +=
                            profile[pixel].same_program_reuses[ray_class];
                        bound_grid_stats->regional_program_id_loads[ray_class] +=
                            profile[pixel].program_id_loads[ray_class];
                        bound_grid_stats->regional_header_loads[ray_class] +=
                            profile[pixel].header_loads[ray_class];
                        bound_grid_stats->regional_dynamic_instructions[ray_class] +=
                            profile[pixel].dynamic_instructions[ray_class];
                    }
                    bound_grid_stats->regional_profiled_paths +=
                        profile[pixel].profiled_paths;
                }
            }
        }

        if (use_bound_grid && config->bound_grid_profile != 0u) {
            std::memset(bound_grid_profile_buffer.contents, 0,
                        pixel_count * sizeof(BoundGridLocalStatsCpp));
            id<MTLCommandBuffer> profile_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> profile_encoder =
                [profile_command computeCommandEncoder];
            [profile_encoder setComputePipelineState:bound_grid_profile_pipeline];
            [profile_encoder setBuffer:bound_grid_profile_buffer offset:0 atIndex:0];
            [profile_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            [profile_encoder setTexture:bound_grid_texture atIndex:0];
            [profile_encoder setTexture:derivative_lower_grid_texture atIndex:1];
            [profile_encoder setTexture:derivative_upper_grid_texture atIndex:2];
            MTLSize profile_threads = threadgroup_for_pipeline(bound_grid_profile_pipeline);
            MTLSize profile_groups = groups_for_extent(config->width, config->height,
                                                       profile_threads);
            [profile_encoder dispatchThreadgroups:profile_groups
                             threadsPerThreadgroup:profile_threads];
            [profile_encoder endEncoding];
            [profile_command commit];
            [profile_command waitUntilCompleted];
            if (profile_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "bound-grid traversal profile failed: %s",
                          profile_command.error.localizedDescription.UTF8String);
                return 1;
            }
            if (bound_grid_stats) {
                const auto *profile = static_cast<const BoundGridLocalStatsCpp *>(
                    bound_grid_profile_buffer.contents);
                for (size_t pixel = 0u; pixel < pixel_count; ++pixel) {
                    for (uint32_t ray_class = 0u; ray_class < 3u; ++ray_class) {
                        bound_grid_stats->macro_cells[ray_class] +=
                            profile[pixel].macro_cells[ray_class];
                        bound_grid_stats->certified_skips[ray_class] +=
                            profile[pixel].certified_skips[ray_class];
                        bound_grid_stats->candidate_intervals[ray_class] +=
                            profile[pixel].candidate_intervals[ray_class];
                        bound_grid_stats->candidate_misses[ray_class] +=
                            profile[pixel].candidate_misses[ray_class];
                        bound_grid_stats->candidate_hits[ray_class] +=
                            profile[pixel].candidate_hits[ray_class];
                        bound_grid_stats->unknown_intervals[ray_class] +=
                            profile[pixel].unknown_intervals[ray_class];
                        bound_grid_stats->field_evaluations[ray_class] +=
                            profile[pixel].field_evaluations[ray_class];
                        bound_grid_stats->directional_steps[ray_class] +=
                            profile[pixel].directional_steps[ray_class];
                        bound_grid_stats->cell_exit_clamps[ray_class] +=
                            profile[pixel].cell_exit_clamps[ray_class];
                        bound_grid_stats->unknown_derivative_intervals[ray_class] +=
                            profile[pixel].unknown_derivative_intervals[ray_class];
                    }
                    bound_grid_stats->profiled_paths += profile[pixel].profiled_paths;
                }
            }
        }

        std::vector<uint8_t> rgba(pixel_count * 4);
        std::memcpy(rgba.data(), out_buffer.contents, rgba.size());
        if (unique_colour_sample(rgba) < 2) {
            set_error(error, error_len, "render output is blank or single-colour");
            return 1;
        }
        if (!write_png(output_path, config->width, config->height, rgba, error, error_len)) {
            return 1;
        }
        return 0;
    }
}

extern "C" int fpt_metal_diagnostic_render(const char *metallib_path,
                                            const char *output_path,
                                            const char *structural_output_path,
                                            const char *shader_source,
                                            size_t shader_source_len,
                                            const struct FptRenderConfig *config,
                                            const struct FptDiagnosticConfig *diagnostic,
                                            double *elapsed_ms,
                                            char *error,
                                            size_t error_len) {
    @autoreleasepool {
        if (!config || !diagnostic) {
            set_error(error, error_len, "missing diagnostic render config");
            return 1;
        }
        FptRenderConfig hydrated_config = *config;
        if (!hydrate_hdri_config(hydrated_config, error, error_len)) return 1;
        config = &hydrated_config;
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        if (!device) {
            set_error(error, error_len, "Metal is unavailable on this machine");
            return 1;
        }
        NSError *ns_error = nil;
        id<MTLLibrary> library = nil;
        if (shader_source && shader_source_len > 0u) {
            NSString *source = [[NSString alloc]
                initWithBytes:shader_source
                       length:shader_source_len
                     encoding:NSUTF8StringEncoding];
            bool cache_hit = false;
            library = source ? compile_runtime_source_library(
                device, source, &cache_hit, &ns_error) : nil;
        } else {
            library = [device newLibraryWithURL:
                [NSURL fileURLWithPath:ns_string(metallib_path)] error:&ns_error];
        }
        if (!library) {
            set_error(error, error_len, "failed to load metallib %s: %s", metallib_path, ns_error.localizedDescription.UTF8String);
            return 1;
        }
        const bool use_voxels = config->renderer_backend == FPT_RENDERER_VOXEL;
        NSString *diagnostic_function_name = use_voxels
            ? @"voxel_diagnostic_kernel"
            : @"sdf_diagnostic_kernel";
        id<MTLFunction> function = [library newFunctionWithName:diagnostic_function_name];
        if (!function) {
            set_error(error, error_len, "sdf_diagnostic_kernel not found in metallib");
            return 1;
        }
        id<MTLComputePipelineState> pipeline = [device newComputePipelineStateWithFunction:function error:&ns_error];
        if (!pipeline) {
            set_error(error, error_len, "failed to create SDF diagnostic pipeline: %s", ns_error.localizedDescription.UTF8String);
            return 1;
        }
        const bool write_structural = structural_output_path && structural_output_path[0] != '\0';
        if (write_structural && use_voxels) {
            set_error(error, error_len, "structural diagnostic output requires the SDF renderer");
            return 1;
        }
        id<MTLComputePipelineState> structural_pipeline = nil;
        if (write_structural) {
            id<MTLFunction> structural_function =
                [library newFunctionWithName:@"sdf_structural_diagnostic_kernel"];
            structural_pipeline = structural_function
                ? [device newComputePipelineStateWithFunction:structural_function error:&ns_error]
                : nil;
            if (!structural_pipeline) {
                set_error(error, error_len,
                          "failed to create structural diagnostic pipeline: %s",
                          ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        id<MTLComputePipelineState> voxel_build_pipeline = nil;
        if (use_voxels) {
            id<MTLFunction> voxel_build_function = [library newFunctionWithName:@"voxel_build_kernel"];
            voxel_build_pipeline = voxel_build_function
                ? [device newComputePipelineStateWithFunction:voxel_build_function error:&ns_error]
                : nil;
            if (!voxel_build_pipeline) {
                set_error(error, error_len, "failed to create voxel diagnostic build pipeline: %s", ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        id<MTLCommandQueue> queue = [device newCommandQueue];
        const size_t pixel_count = static_cast<size_t>(config->width) * config->height;
        id<MTLBuffer> out_buffer = [device newBufferWithLength:pixel_count * 4 options:MTLResourceStorageModeShared];
        id<MTLBuffer> structural_buffer = write_structural
            ? [device newBufferWithLength:pixel_count * 32u options:MTLResourceStorageModeShared]
            : nil;
        id<MTLBuffer> cfg_buffer = [device newBufferWithBytes:config length:sizeof(FptRenderConfig) options:MTLResourceStorageModeShared];
        id<MTLBuffer> diag_buffer = [device newBufferWithBytes:diagnostic length:sizeof(FptDiagnosticConfig) options:MTLResourceStorageModeShared];
        const size_t voxel_count = use_voxels
            ? static_cast<size_t>(config->voxel_resolution) * config->voxel_resolution * config->voxel_resolution
            : 0u;
        id<MTLBuffer> voxel_buffer = use_voxels
            ? [device newBufferWithLength:voxel_count * 12u options:MTLResourceStorageModeShared]
            : nil;
        id<MTLBuffer> voxel_page_table_buffer = nil;
        if (!queue || !out_buffer || !cfg_buffer || !diag_buffer ||
            (write_structural && !structural_buffer) || (use_voxels && !voxel_buffer)) {
            set_error(error, error_len, "failed to allocate SDF diagnostic buffers");
            return 1;
        }
        std::unique_lock<std::mutex> diagnostic_gpu_lock;
        if (std::getenv("FPT_SERIALIZE_DIAGNOSTIC_GPU")) {
            diagnostic_gpu_lock = std::unique_lock<std::mutex>(diagnostic_gpu_mutex());
        }
        NSDate *start_time = [NSDate date];
        double gpu_elapsed_seconds = 0.0;
        if (use_voxels) {
            id<MTLCommandBuffer> build_command = [queue commandBuffer];
            id<MTLComputeCommandEncoder> build_encoder = [build_command computeCommandEncoder];
            [build_encoder setComputePipelineState:voxel_build_pipeline];
            [build_encoder setBuffer:voxel_buffer offset:0 atIndex:0];
            [build_encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            MTLSize voxel_grid = MTLSizeMake(config->voxel_resolution,
                                             config->voxel_resolution,
                                             config->voxel_resolution);
            [build_encoder dispatchThreads:voxel_grid threadsPerThreadgroup:MTLSizeMake(4u, 4u, 4u)];
            [build_encoder endEncoding];
            [build_command commit];
            [build_command waitUntilCompleted];
            if (build_command.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "voxel diagnostic build failed: %s", build_command.error.localizedDescription.UTF8String);
                return 1;
            }
            if (build_command.GPUEndTime >= build_command.GPUStartTime) {
                gpu_elapsed_seconds += build_command.GPUEndTime - build_command.GPUStartTime;
            }
            VoxelStorageResult storage = finalize_voxel_storage(device, *config, voxel_buffer);
            if (!storage.cells || !storage.page_table) {
                set_error(error, error_len, "failed to finalize diagnostic voxel storage");
                return 1;
            }
            voxel_buffer = storage.cells;
            voxel_page_table_buffer = storage.page_table;
        }
        MTLSize threads_per_group = threadgroup_for_pipeline(pipeline);
        const uint32_t rows_per_dispatch = config->sdf_id == FPT_SDF_MANDELBULBER
            ? 8u
            : config->height;
        for (uint32_t row = 0u; row < config->height; row += rows_per_dispatch) {
            FptDiagnosticConfig tile_diagnostic = *diagnostic;
            tile_diagnostic.dispatch_origin[0] = 0u;
            tile_diagnostic.dispatch_origin[1] = row;
            std::memcpy(diag_buffer.contents, &tile_diagnostic, sizeof(tile_diagnostic));
            const uint32_t tile_height = std::min(rows_per_dispatch, config->height - row);
            id<MTLCommandBuffer> command_buffer = [queue commandBuffer];
            id<MTLComputeCommandEncoder> encoder = [command_buffer computeCommandEncoder];
            [encoder setComputePipelineState:pipeline];
            [encoder setBuffer:out_buffer offset:0 atIndex:0];
            [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
            [encoder setBuffer:diag_buffer offset:0 atIndex:2];
            if (use_voxels) [encoder setBuffer:voxel_buffer offset:0 atIndex:3];
            if (use_voxels) [encoder setBuffer:voxel_page_table_buffer offset:0 atIndex:4];
            MTLSize groups = groups_for_extent(config->width, tile_height, threads_per_group);
            [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
            if (write_structural) {
                const MTLSize structural_threads = threadgroup_for_pipeline(structural_pipeline);
                [encoder setComputePipelineState:structural_pipeline];
                [encoder setBuffer:structural_buffer offset:0 atIndex:0];
                [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
                [encoder setBuffer:diag_buffer offset:0 atIndex:2];
                const MTLSize structural_groups =
                    groups_for_extent(config->width, tile_height, structural_threads);
                [encoder dispatchThreadgroups:structural_groups
                         threadsPerThreadgroup:structural_threads];
            }
            [encoder endEncoding];
            [command_buffer commit];
            [command_buffer waitUntilCompleted];
            if (command_buffer.status == MTLCommandBufferStatusError) {
                set_error(error, error_len, "SDF diagnostic command buffer failed at row %u: %s",
                          row, command_buffer.error.localizedDescription.UTF8String);
                return 1;
            }
            if (command_buffer.GPUEndTime >= command_buffer.GPUStartTime) {
                gpu_elapsed_seconds += command_buffer.GPUEndTime - command_buffer.GPUStartTime;
            }
        }
        const double wall_elapsed_ms = -[start_time timeIntervalSinceNow] * 1000.0;
        if (elapsed_ms) {
            *elapsed_ms = gpu_elapsed_seconds > 0.0
                ? gpu_elapsed_seconds * 1000.0
                : wall_elapsed_ms;
        }
        if (diagnostic_gpu_lock.owns_lock()) diagnostic_gpu_lock.unlock();
        std::vector<uint8_t> rgba(pixel_count * 4);
        std::memcpy(rgba.data(), out_buffer.contents, rgba.size());
        if (!write_png(output_path, config->width, config->height, rgba, error, error_len)) return 1;
        if (write_structural) {
            const std::filesystem::path structural_path(structural_output_path);
            if (structural_path.has_parent_path()) {
                std::filesystem::create_directories(structural_path.parent_path());
            }
            std::ofstream structural_file(structural_path, std::ios::binary);
            if (!structural_file) {
                set_error(error, error_len, "failed to create structural diagnostic output %s",
                          structural_output_path);
                return 1;
            }
            structural_file.write(static_cast<const char *>(structural_buffer.contents),
                                  static_cast<std::streamsize>(pixel_count * 32u));
            if (!structural_file) {
                set_error(error, error_len, "failed to write structural diagnostic output %s",
                          structural_output_path);
                return 1;
            }
        }
        return 0;
    }
}

extern "C" int fpt_metal_preview(const char *metallib_path,
                                  const char *stitch_metallib_path,
                                  const char *const *stitch_archive_paths,
                                  const char *shader_source,
                                  size_t shader_source_len,
                                  const struct FptRenderConfig *config,
                                  const struct FptRenderConfig *scene_configs,
                                  uint32_t scene_config_count,
                                  uint32_t scene_config_index,
                                  char *error,
                                  size_t error_len) {
    @autoreleasepool {
        if (!config) {
            set_error(error, error_len, "missing preview config");
            return 1;
        }
        NSError *ns_error = nil;
        FPTPreviewController *controller = [[FPTPreviewController alloc] initWithConfig:config
                                                                          sceneConfigs:scene_configs
                                                                            sceneCount:scene_config_count
                                                                            sceneIndex:scene_config_index
                                                                               metallib:ns_string(metallib_path)
                                                                        stitchMetallib:ns_string(stitch_metallib_path)
                                                                     stitchArchivePaths:stitch_archive_paths
                                                                          shaderSource:shader_source
                                                                    shaderSourceLength:shader_source_len
                                                                                  error:&ns_error];
        if (!controller) {
            set_error(error, error_len, "failed to start preview: %s", ns_error.localizedDescription.UTF8String);
            return 1;
        }
        [controller run];
        return 0;
    }
}

extern "C" int fpt_compare_images(const char *baseline_path,
                                  const char *candidate_path,
                                  const char *report_path,
                                  char *error,
                                  size_t error_len) {
    @autoreleasepool {
        Image baseline;
        Image candidate;
        if (!read_image(baseline_path, baseline, error, error_len)) return 1;
        if (!read_image(candidate_path, candidate, error, error_len)) return 1;
        if (baseline.width != candidate.width || baseline.height != candidate.height) {
            set_error(error, error_len, "dimension mismatch: baseline %ux%u candidate %ux%u",
                      baseline.width, baseline.height, candidate.width, candidate.height);
            return 1;
        }

        double abs_sum = 0.0;
        double sq_sum = 0.0;
        uint32_t max_abs = 0;
        uint64_t changed_pixels = 0;
        const size_t pixels = static_cast<size_t>(baseline.width) * baseline.height;
        for (size_t p = 0; p < pixels; ++p) {
            bool changed = false;
            for (size_t ch = 0; ch < 3; ++ch) {
                const int a = baseline.rgba[p * 4 + ch];
                const int b = candidate.rgba[p * 4 + ch];
                const uint32_t d = static_cast<uint32_t>(std::abs(a - b));
                abs_sum += d;
                sq_sum += double(d) * double(d);
                max_abs = std::max(max_abs, d);
                if (d > 0) changed = true;
            }
            if (changed) ++changed_pixels;
        }
        const double sample_count = double(pixels * 3);
        const double mae = abs_sum / sample_count;
        const double rmse = std::sqrt(sq_sum / sample_count);
        const double changed_pct = pixels ? (100.0 * double(changed_pixels) / double(pixels)) : 0.0;
        const size_t baseline_unique = unique_colour_sample(baseline.rgba);
        const size_t candidate_unique = unique_colour_sample(candidate.rgba);
        const Gate gate = gate_for_path(candidate_path);
        const double ssim = luminance_ssim(baseline, candidate);
        const double low_ssim = low_frequency_luminance_ssim(baseline, candidate);
        const bool recognition_gate = candidate_unique >= 16 && mae < gate.mae_threshold;
        const bool strict_gate = candidate_unique >= 64 && mae < gate.strict_mae_threshold && low_ssim >= gate.strict_ssim_threshold;
        const std::string sheet_path = replace_extension(report_path, ".comparison.png");
        uint32_t sheet_w = 0;
        uint32_t sheet_h = 0;
        std::vector<uint8_t> sheet = comparison_sheet(baseline, candidate, sheet_w, sheet_h);
        if (!write_png(sheet_path.c_str(), sheet_w, sheet_h, sheet, error, error_len)) {
            return 1;
        }

        std::ofstream out(report_path);
        if (!out) {
            set_error(error, error_len, "failed to open report %s", report_path);
            return 1;
        }
        out << "{\n"
            << "  \"baseline\": \"" << baseline_path << "\",\n"
            << "  \"candidate\": \"" << candidate_path << "\",\n"
            << "  \"width\": " << baseline.width << ",\n"
            << "  \"height\": " << baseline.height << ",\n"
            << "  \"mean_absolute_error\": " << mae << ",\n"
            << "  \"root_mean_square_error\": " << rmse << ",\n"
            << "  \"luminance_ssim\": " << ssim << ",\n"
            << "  \"low_frequency_luminance_ssim\": " << low_ssim << ",\n"
            << "  \"max_absolute_error\": " << max_abs << ",\n"
            << "  \"changed_pixel_percent\": " << changed_pct << ",\n"
            << "  \"gate_class\": \"" << gate.name << "\",\n"
            << "  \"gate_mae_threshold\": " << gate.mae_threshold << ",\n"
            << "  \"strict_mae_threshold\": " << gate.strict_mae_threshold << ",\n"
            << "  \"strict_ssim_threshold\": " << gate.strict_ssim_threshold << ",\n"
            << "  \"comparison_sheet\": \"" << sheet_path << "\",\n"
            << "  \"unique_colours\": {\n"
            << "    \"baseline\": " << baseline_unique << ",\n"
            << "    \"candidate\": " << candidate_unique << "\n"
            << "  },\n"
            << "  \"recognition_gate\": " << (recognition_gate ? "true" : "false") << ",\n"
            << "  \"strict_gate\": " << (strict_gate ? "true" : "false") << "\n"
            << "}\n";
        return 0;
    }
}
