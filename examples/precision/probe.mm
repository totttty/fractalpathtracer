// SPDX-License-Identifier: Apache-2.0
// Standalone diagnostic only: deliberately does not use the production fast-math bridge.
#import <Foundation/Foundation.h>
#import <Metal/Metal.h>
#include <algorithm>
#include <fstream>
#include <iostream>
#include <vector>

int main(int argc, char **argv) {
    if (argc != 4) {
        std::cerr << "usage: probe source.metal input.bin output.bin\n";
        return 2;
    }
    @autoreleasepool {
        NSError *error = nil;
        id<MTLDevice> device = MTLCreateSystemDefaultDevice();
        NSString *source = [NSString stringWithContentsOfFile:@(argv[1])
            encoding:NSUTF8StringEncoding error:&error];
        if (!source || !device) return 3;
        MTLCompileOptions *options = [MTLCompileOptions new];
        options.languageVersion = MTLLanguageVersion2_4;
        if (@available(macOS 15.0, *)) {
            options.mathMode = MTLMathModeSafe;
        } else {
            #pragma clang diagnostic push
            #pragma clang diagnostic ignored "-Wdeprecated-declarations"
            options.fastMathEnabled = NO;
            #pragma clang diagnostic pop
        }
        id<MTLLibrary> library = [device newLibraryWithSource:source options:options error:&error];
        if (!library) { std::cerr << error.localizedDescription.UTF8String; return 4; }
        id<MTLComputePipelineState> pipeline = [device
            newComputePipelineStateWithFunction:[library newFunctionWithName:@"probe"] error:&error];
        if (!pipeline) { std::cerr << error.localizedDescription.UTF8String; return 5; }
        std::ifstream input(argv[2], std::ios::binary | std::ios::ate);
        if (!input) return 6;
        const auto length = input.tellg();
        if (length <= 0 || length > 64 * 1024 * 1024 || length % 36) return 7;
        size_t size = static_cast<size_t>(length), count = size / 36;
        input.seekg(0);
        std::vector<char> bytes(size);
        if (!input.read(bytes.data(), size)) return 8;
        id<MTLBuffer> points = [device newBufferWithBytes:bytes.data() length:size
            options:MTLResourceStorageModeShared];
        id<MTLBuffer> output = [device newBufferWithLength:count * 32 options:MTLResourceStorageModeShared];
        if (!points || !output) return 9;
        id<MTLCommandQueue> queue = [device newCommandQueue];
        double totalMs = 0, maxMs = 0;
        for (size_t begin = 0; begin < count; begin += 256) {
            id<MTLCommandBuffer> commands = [queue commandBuffer];
            id<MTLComputeCommandEncoder> encoder = [commands computeCommandEncoder];
            if (!commands || !encoder) return 10;
            [encoder setComputePipelineState:pipeline];
            [encoder setBuffer:points offset:begin * 36 atIndex:0];
            [encoder setBuffer:output offset:begin * 32 atIndex:1];
            [encoder dispatchThreads:MTLSizeMake(std::min(size_t(256), count - begin), 1, 1)
                threadsPerThreadgroup:MTLSizeMake(std::min(NSUInteger(32), pipeline.maxTotalThreadsPerThreadgroup), 1, 1)];
            [encoder endEncoding];
            [commands commit];
            [commands waitUntilCompleted];
            if (commands.status != MTLCommandBufferStatusCompleted) {
                std::cerr << commands.error.localizedDescription.UTF8String;
                return 11;
            }
            double ms = (commands.GPUEndTime - commands.GPUStartTime) * 1000;
            totalMs += ms;
            maxMs = std::max(maxMs, ms);
        }
        std::ofstream result(argv[3], std::ios::binary);
        result.write(static_cast<const char *>(output.contents), count * 32);
        std::cout << "{\"gpu_ms\":" << totalMs << ",\"max_dispatch_ms\":" << maxMs
                  << ",\"max_threads\":" << pipeline.maxTotalThreadsPerThreadgroup
                  << ",\"execution_width\":" << pipeline.threadExecutionWidth
                  << ",\"rays_per_dispatch\":256,\"fast_math\":false}\n";
        return result ? 0 : 12;
    }
}
