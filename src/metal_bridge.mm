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
#include <string>
#include <vector>

namespace {

void set_error(char *error, size_t error_len, const char *fmt, ...) {
    if (!error || error_len == 0) return;
    va_list args;
    va_start(args, fmt);
    vsnprintf(error, error_len, fmt, args);
    va_end(args);
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

struct FptAccumulationChunkCpp {
    uint32_t start_sample;
    uint32_t sample_count;
};

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
    uint64_t resident_bytes = 0u;
};

VoxelStorageResult finalize_voxel_storage(id<MTLDevice> device,
                                          const FptRenderConfig &config,
                                          id<MTLBuffer> dense_cells) {
    VoxelStorageResult result;
    const uint32_t resolution = config.voxel_resolution;
    const uint32_t zero = 0u;
    if (config.voxel_storage != FPT_VOXEL_STORAGE_SPARSE_BRICKS) {
        result.cells = dense_cells;
        result.page_table = [device newBufferWithBytes:&zero
                                                length:sizeof(zero)
                                               options:MTLResourceStorageModeShared];
        result.resident_bytes = dense_cells.length + result.page_table.length;
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
                        }
                    }
                }
            }
        }
    }
    result.resident_bytes = result.cells.length + result.page_table.length;
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
    label.maximumNumberOfLines = 6;
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
    uint32_t shadow_steps;
    uint32_t normal_evals;
    uint32_t bounces;
    uint32_t pixels;
};

NSString *format_sdf_work_breakdown(const FptSdfProfileCountsCpp &counts,
                                    double sample_gpu_ms,
                                    const FptRenderConfig &config) {
    const double primary_units = static_cast<double>(counts.primary_steps);
    const double shadow_units = static_cast<double>(counts.shadow_steps);
    const bool program_gradient = config.sdf_id == FPT_SDF_PROGRAM &&
                                  (config.sdf_normal_mode == 0u || config.sdf_normal_mode == 2u);
    const double normal_cost = program_gradient ? 1.0 : (config.sdf_normal_mode == 1u ? 4.0 : 6.0);
    const double normal_units = static_cast<double>(counts.normal_evals) * normal_cost;
    const double bounce_units = static_cast<double>(counts.bounces);
    const double total_units = primary_units + shadow_units + normal_units + bounce_units;
    if (sample_gpu_ms <= 0.0 || total_units <= 0.0 || counts.pixels == 0u) {
        return @"SDF work est: profiling...";
    }
    const double primary_ms = sample_gpu_ms * primary_units / total_units;
    const double shadow_ms = sample_gpu_ms * shadow_units / total_units;
    const double normal_ms = sample_gpu_ms * normal_units / total_units;
    const double bounce_ms = sample_gpu_ms * bounce_units / total_units;
    return [NSString stringWithFormat:@"SDF est: primary %.2f ms, shadow %.2f ms, normal %.2f ms, bounce %.2f ms",
            primary_ms,
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
    uint32_t _sceneIndex;
    CVDisplayLinkRef _displayLink;
    std::atomic_bool _displayTickPending;
}
@property(nonatomic) struct FptRenderConfig config;
@property(nonatomic, strong) id<MTLDevice> device;
@property(nonatomic, strong) id<MTLCommandQueue> queue;
@property(nonatomic, strong) id<MTLComputePipelineState> pipeline;
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
- (instancetype)initWithConfig:(const struct FptRenderConfig *)config
                  sceneConfigs:(const struct FptRenderConfig *)sceneConfigs
                    sceneCount:(uint32_t)sceneCount
                    sceneIndex:(uint32_t)sceneIndex
                      metallib:(NSString *)metallib
                         error:(NSError **)error;
- (void)run;
- (BOOL)setMovementKey:(unichar)key down:(BOOL)down;
- (void)handleSpecialKey:(NSEvent *)event;
- (void)mouseLookWithDeltaX:(CGFloat)deltaX deltaY:(CGFloat)deltaY;
- (void)switchSceneByOffset:(int)offset;
- (void)displayLinkTick;
- (void)renderFrame;
- (BOOL)rebuildVoxelField;
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
                         error:(NSError **)error {
    self = [super init];
    if (!self) return nil;
    _displayLink = nullptr;
    _displayTickPending.store(false);
    _needsRender = YES;
    _accumulationComplete = NO;
    _config = *config;
    if (sceneConfigs && sceneCount > 0u) {
        _sceneConfigs.assign(sceneConfigs, sceneConfigs + sceneCount);
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
        _sceneIndex = 0u;
    }
    _device = MTLCreateSystemDefaultDevice();
    if (!_device) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:1 userInfo:@{NSLocalizedDescriptionKey: @"Metal is unavailable"}];
        return nil;
    }
    id<MTLLibrary> library = [_device newLibraryWithURL:[NSURL fileURLWithPath:metallib] error:error];
    if (!library) return nil;
    const bool useVoxels = _config.renderer_backend == FPT_RENDERER_VOXEL;
    NSString *functionName = useVoxels
        ? (_config.preview ? @"voxel_preview_linear_kernel" : @"voxel_accumulate_kernel")
        : (_config.preview ? @"preview_linear_kernel" : @"accumulate_kernel");
    id<MTLFunction> function = [library newFunctionWithName:functionName];
    if (!function) {
        if (error) *error = [NSError errorWithDomain:@"FPTMetal" code:2 userInfo:@{NSLocalizedDescriptionKey: @"preview compute kernel not found"}];
        return nil;
    }
    _pipeline = [_device newComputePipelineStateWithFunction:function error:error];
    if (!_pipeline) return nil;
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
    return self;
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
    if (self.config.renderer_backend == FPT_RENDERER_VOXEL) {
        if (![self rebuildVoxelField]) return;
        self.sdfWorkBreakdown = @"Voxel field: persistent";
    } else {
        self.sdfWorkBreakdown = self.config.sdf_profile != 0u ? @"SDF est: profiling..." : @"SDF profile: disabled (--sdf-profile)";
    }
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
    if (self.config.preview && !self.needsRender && !moving) return;
    if (!self.config.preview && self.accumulationComplete && !self.needsRender && !moving) return;
    self.needsRender = NO;
    NSDate *frameStart = [NSDate date];
    [self updateInteractiveMotion];
    const size_t pixel_count = static_cast<size_t>(self.config.width) * self.config.height;
    id<MTLBuffer> cfg_buffer = [self.device newBufferWithBytes:&_config length:sizeof(FptRenderConfig) options:MTLResourceStorageModeShared];
    if (!self.outBuffer || !cfg_buffer) return;
    MTLSize threads_per_group = threadgroup_for_pipeline(self.pipeline);
    MTLSize groups = groups_for_extent(self.config.width, self.config.height, threads_per_group);
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
        [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
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

extern "C" int fpt_metal_render(const char *metallib_path,
                                 const char *output_path,
                                 const struct FptRenderConfig *config,
                                 double *build_ms,
                                 double *elapsed_ms,
                                 uint64_t *voxel_memory_bytes,
                                 uint32_t *voxel_active_bricks,
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
        const bool use_voxels = config->renderer_backend == FPT_RENDERER_VOXEL;
        if (voxel_memory_bytes) *voxel_memory_bytes = 0u;
        if (voxel_active_bricks) *voxel_active_bricks = 0u;
        const uint32_t requested_samples = std::clamp<uint32_t>(config->samples, 1u, 512u);
        const bool auto_batch_accumulation = requested_samples <= 16u && config->sdf_id != FPT_SDF_CAGE_FRACTAL;
        const bool use_batch_accumulation = !config->preview &&
                                            (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_BATCH ||
                                             (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_AUTO && auto_batch_accumulation));
        const bool use_chunked_accumulation = !config->preview &&
                                              (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_CHUNKED ||
                                               (config->sdf_accumulation_mode == FPT_SDF_ACCUMULATION_AUTO && !auto_batch_accumulation));
        NSString *main_function_name = nil;
        if (use_voxels) {
            main_function_name = config->preview ? @"voxel_preview_linear_kernel" :
                (use_batch_accumulation ? @"voxel_accumulate_all_kernel" :
                 (use_chunked_accumulation ? @"voxel_accumulate_chunk_kernel" : @"voxel_accumulate_kernel"));
        } else {
            main_function_name = config->preview ? @"preview_linear_kernel" :
                (use_batch_accumulation ? @"accumulate_all_kernel" :
                 (use_chunked_accumulation ? @"accumulate_chunk_kernel" : @"accumulate_kernel"));
        }
        id<MTLFunction> main_function = [library newFunctionWithName:main_function_name];
        if (!main_function) {
            set_error(error, error_len, "%s not found in metallib", main_function_name.UTF8String);
            return 1;
        }
        id<MTLComputePipelineState> main_pipeline = [device newComputePipelineStateWithFunction:main_function error:&ns_error];
        if (!main_pipeline) {
            set_error(error, error_len, "failed to create compute pipeline: %s", ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLFunction> present_function = [library newFunctionWithName:@"present_kernel"];
        if (!present_function) {
            set_error(error, error_len, "present_kernel not found in metallib");
            return 1;
        }
        id<MTLComputePipelineState> present_pipeline = [device newComputePipelineStateWithFunction:present_function error:&ns_error];
        if (!present_pipeline) {
            set_error(error, error_len, "failed to create present pipeline: %s", ns_error.localizedDescription.UTF8String);
            return 1;
        }
        id<MTLComputePipelineState> voxel_build_pipeline = nil;
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
        }
        id<MTLComputePipelineState> focus_pipeline = nil;
        if (config->focus_distance <= 0.0f) {
            id<MTLFunction> focus_function = [library newFunctionWithName:use_voxels
                ? @"estimate_voxel_focus_distance_kernel"
                : @"estimate_focus_distance_kernel"];
            if (!focus_function) {
                set_error(error, error_len, "estimate_focus_distance_kernel not found in metallib");
                return 1;
            }
            focus_pipeline = [device newComputePipelineStateWithFunction:focus_function error:&ns_error];
            if (!focus_pipeline) {
                set_error(error, error_len, "failed to create focus pipeline: %s", ns_error.localizedDescription.UTF8String);
                return 1;
            }
        }
        id<MTLCommandQueue> queue = [device newCommandQueue];
        if (!queue) {
            set_error(error, error_len, "failed to create Metal command queue");
            return 1;
        }

        const size_t pixel_count = static_cast<size_t>(config->width) * config->height;
        id<MTLBuffer> out_buffer = [device newBufferWithLength:pixel_count * 4 options:MTLResourceStorageModeShared];
        id<MTLBuffer> cfg_buffer = [device newBufferWithBytes:config length:sizeof(FptRenderConfig) options:MTLResourceStorageModeShared];
        id<MTLBuffer> accum_buffer = [device newBufferWithLength:pixel_count * sizeof(float) * 4 options:MTLResourceStorageModePrivate];
        const size_t voxel_count = use_voxels
            ? static_cast<size_t>(config->voxel_resolution) * config->voxel_resolution * config->voxel_resolution
            : 0u;
        id<MTLBuffer> voxel_buffer = use_voxels
            ? [device newBufferWithLength:voxel_count * 12u options:MTLResourceStorageModeShared]
            : nil;
        id<MTLBuffer> voxel_page_table_buffer = nil;
        id<MTLBuffer> focus_buffer = focus_pipeline ? [device newBufferWithLength:sizeof(float) options:MTLResourceStorageModeShared] : nil;
        if (!out_buffer || !cfg_buffer) {
            set_error(error, error_len, "failed to allocate Metal buffers");
            return 1;
        }
        if (!accum_buffer) {
            set_error(error, error_len, "failed to allocate Metal accumulation buffer");
            return 1;
        }
        if (use_voxels && !voxel_buffer) {
            set_error(error, error_len, "failed to allocate voxel field buffer");
            return 1;
        }
        if (focus_pipeline && !focus_buffer) {
            set_error(error, error_len, "failed to allocate focus-distance buffer");
            return 1;
        }

        if (build_ms) *build_ms = 0.0;
        if (use_voxels) {
            NSDate *voxel_build_start = [NSDate date];
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
                set_error(error, error_len, "Metal voxel build failed: %s", voxel_build_command.error.localizedDescription.UTF8String);
                return 1;
            }
            VoxelStorageResult storage = finalize_voxel_storage(device, *config, voxel_buffer);
            if (!storage.cells || !storage.page_table) {
                set_error(error, error_len, "failed to finalize voxel storage");
                return 1;
            }
            voxel_buffer = storage.cells;
            voxel_page_table_buffer = storage.page_table;
            if (voxel_memory_bytes) *voxel_memory_bytes = storage.resident_bytes;
            if (voxel_active_bricks) *voxel_active_bricks = storage.active_bricks;
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
        MTLSize groups = groups_for_extent(config->width, config->height, threads_per_group);
        id<MTLComputeCommandEncoder> encoder = nil;
        if (use_chunked_accumulation) {
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
                [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
                [encoder endEncoding];
            }
        }
        encoder = [command_buffer computeCommandEncoder];
        [encoder setComputePipelineState:present_pipeline];
        [encoder setBuffer:out_buffer offset:0 atIndex:0];
        [encoder setBuffer:accum_buffer offset:0 atIndex:1];
        [encoder setBuffer:cfg_buffer offset:0 atIndex:2];
        [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        [command_buffer commit];
        [command_buffer waitUntilCompleted];
        if (elapsed_ms) {
            *elapsed_ms = -[start_time timeIntervalSinceNow] * 1000.0;
        }
        if (command_buffer.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "Metal command buffer failed: %s", command_buffer.error.localizedDescription.UTF8String);
            return 1;
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
        id<MTLLibrary> library = [device newLibraryWithURL:[NSURL fileURLWithPath:ns_string(metallib_path)] error:&ns_error];
        if (!library) {
            set_error(error, error_len, "failed to load metallib %s: %s", metallib_path, ns_error.localizedDescription.UTF8String);
            return 1;
        }
        const bool use_voxels = config->renderer_backend == FPT_RENDERER_VOXEL;
        id<MTLFunction> function = [library newFunctionWithName:use_voxels
            ? @"voxel_diagnostic_kernel"
            : @"sdf_diagnostic_kernel"];
        if (!function) {
            set_error(error, error_len, "sdf_diagnostic_kernel not found in metallib");
            return 1;
        }
        id<MTLComputePipelineState> pipeline = [device newComputePipelineStateWithFunction:function error:&ns_error];
        if (!pipeline) {
            set_error(error, error_len, "failed to create SDF diagnostic pipeline: %s", ns_error.localizedDescription.UTF8String);
            return 1;
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
        id<MTLBuffer> cfg_buffer = [device newBufferWithBytes:config length:sizeof(FptRenderConfig) options:MTLResourceStorageModeShared];
        id<MTLBuffer> diag_buffer = [device newBufferWithBytes:diagnostic length:sizeof(FptDiagnosticConfig) options:MTLResourceStorageModeShared];
        const size_t voxel_count = use_voxels
            ? static_cast<size_t>(config->voxel_resolution) * config->voxel_resolution * config->voxel_resolution
            : 0u;
        id<MTLBuffer> voxel_buffer = use_voxels
            ? [device newBufferWithLength:voxel_count * 12u options:MTLResourceStorageModeShared]
            : nil;
        id<MTLBuffer> voxel_page_table_buffer = nil;
        if (!queue || !out_buffer || !cfg_buffer || !diag_buffer || (use_voxels && !voxel_buffer)) {
            set_error(error, error_len, "failed to allocate SDF diagnostic buffers");
            return 1;
        }
        NSDate *start_time = [NSDate date];
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
            VoxelStorageResult storage = finalize_voxel_storage(device, *config, voxel_buffer);
            if (!storage.cells || !storage.page_table) {
                set_error(error, error_len, "failed to finalize diagnostic voxel storage");
                return 1;
            }
            voxel_buffer = storage.cells;
            voxel_page_table_buffer = storage.page_table;
        }
        id<MTLCommandBuffer> command_buffer = [queue commandBuffer];
        id<MTLComputeCommandEncoder> encoder = [command_buffer computeCommandEncoder];
        [encoder setComputePipelineState:pipeline];
        [encoder setBuffer:out_buffer offset:0 atIndex:0];
        [encoder setBuffer:cfg_buffer offset:0 atIndex:1];
        [encoder setBuffer:diag_buffer offset:0 atIndex:2];
        if (use_voxels) [encoder setBuffer:voxel_buffer offset:0 atIndex:3];
        if (use_voxels) [encoder setBuffer:voxel_page_table_buffer offset:0 atIndex:4];
        MTLSize threads_per_group = threadgroup_for_pipeline(pipeline);
        MTLSize groups = groups_for_extent(config->width, config->height, threads_per_group);
        [encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads_per_group];
        [encoder endEncoding];
        [command_buffer commit];
        [command_buffer waitUntilCompleted];
        if (elapsed_ms) *elapsed_ms = -[start_time timeIntervalSinceNow] * 1000.0;
        if (command_buffer.status == MTLCommandBufferStatusError) {
            set_error(error, error_len, "SDF diagnostic command buffer failed: %s", command_buffer.error.localizedDescription.UTF8String);
            return 1;
        }
        std::vector<uint8_t> rgba(pixel_count * 4);
        std::memcpy(rgba.data(), out_buffer.contents, rgba.size());
        if (!write_png(output_path, config->width, config->height, rgba, error, error_len)) return 1;
        return 0;
    }
}

extern "C" int fpt_metal_preview(const char *metallib_path,
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
