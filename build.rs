use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command, label: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to run {label}: {error}"));
    assert!(status.success(), "{label} failed with {status}");
}

fn main() {
    println!("cargo:rerun-if-changed=src/metal_bridge.h");
    println!("cargo:rerun-if-changed=src/metal_bridge.mm");
    println!("cargo:rerun-if-changed=shaders/Shaders.metal");

    cc::Build::new()
        .cpp(true)
        .file("src/metal_bridge.mm")
        .include("src")
        .flag("-std=c++17")
        .flag("-fobjc-arc")
        .flag("-ObjC++")
        .flag("-Wno-deprecated-declarations")
        .compile("fpt_metal_bridge");

    for framework in [
        "Foundation",
        "AppKit",
        "Metal",
        "CoreGraphics",
        "ImageIO",
        "CoreVideo",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=c++");

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let air = out.join("Shaders.air");
    let metallib = out.join("Shaders.metallib");
    run(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-c",
                "shaders/Shaders.metal",
                "-o",
            ])
            .arg(&air),
        "Metal shader compilation",
    );
    run(
        Command::new("xcrun")
            .args(["-sdk", "macosx", "metallib"])
            .arg(&air)
            .arg("-o")
            .arg(&metallib),
        "Metal library linking",
    );
    println!("cargo:rustc-env=FPT_METALLIB_PATH={}", metallib.display());
}
