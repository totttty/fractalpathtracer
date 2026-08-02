use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, process::Command};

fn run(command: &mut Command, label: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("failed to run {label}: {error}"));
    assert!(status.success(), "{label} failed with {status}");
}

fn output(command: &mut Command, label: &str) -> String {
    let result = command
        .output()
        .unwrap_or_else(|error| panic!("failed to run {label}: {error}"));
    assert!(
        result.status.success(),
        "{label} failed with {}",
        result.status
    );
    String::from_utf8_lossy(&result.stdout).trim().to_owned()
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
    let builtin_air = out.join("ShadersBuiltin.air");
    let builtin_metallib = out.join("ShadersBuiltin.metallib");
    let cage_air = out.join("ShadersCage.air");
    let cage_metallib = out.join("ShadersCage.metallib");
    let tower_air = out.join("ShadersTower.air");
    let tower_metallib = out.join("ShadersTower.metallib");
    let stitch_air = out.join("ShadersStitchHost.air");
    let stitch_metallib = out.join("ShadersStitchHost.metallib");
    run(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-fmetal-math-mode=fast",
                "-fmetal-math-fp32-functions=fast",
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
    run(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-fmetal-math-mode=fast",
                "-fmetal-math-fp32-functions=fast",
                "-DFPT_BUILTIN_CONFIG=1",
                "-c",
                "shaders/Shaders.metal",
                "-o",
            ])
            .arg(&builtin_air),
        "compact built-in Metal shader compilation",
    );
    run(
        Command::new("xcrun")
            .args(["-sdk", "macosx", "metallib"])
            .arg(&builtin_air)
            .arg("-o")
            .arg(&builtin_metallib),
        "compact built-in Metal library linking",
    );
    run(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-fmetal-math-mode=fast",
                "-fmetal-math-fp32-functions=fast",
                "-DFPT_BUILTIN_CONFIG=1",
                "-DFPT_BUILTIN_CAGE_ONLY=1",
                "-c",
                "shaders/Shaders.metal",
                "-o",
            ])
            .arg(&cage_air),
        "Cage-specialized Metal shader compilation",
    );
    run(
        Command::new("xcrun")
            .args(["-sdk", "macosx", "metallib"])
            .arg(&cage_air)
            .arg("-o")
            .arg(&cage_metallib),
        "Cage-specialized Metal library linking",
    );
    run(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-fmetal-math-mode=fast",
                "-fmetal-math-fp32-functions=fast",
                "-DFPT_BUILTIN_CONFIG=1",
                "-DFPT_BUILTIN_TOWER_ONLY=1",
                "-c",
                "shaders/Shaders.metal",
                "-o",
            ])
            .arg(&tower_air),
        "Tower-specialized Metal shader compilation",
    );
    run(
        Command::new("xcrun")
            .args(["-sdk", "macosx", "metallib"])
            .arg(&tower_air)
            .arg("-o")
            .arg(&tower_metallib),
        "Tower-specialized Metal library linking",
    );
    run(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=macos-metal2.4",
                "-fmetal-math-mode=fast",
                "-fmetal-math-fp32-functions=fast",
                "-DFPT_STITCH_HOST=1",
                "-c",
                "shaders/Shaders.metal",
                "-o",
            ])
            .arg(&stitch_air),
        "Metal stitch-host AIR compilation",
    );
    run(
        Command::new("xcrun")
            .args(["-sdk", "macosx", "metallib"])
            .arg(&stitch_air)
            .arg("--split-module-without-linking")
            .arg("-o")
            .arg(&stitch_metallib),
        "Metal stitch-host library packaging",
    );
    let metallib_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(&metallib).expect("read compiled Metal library"))
    );
    let builtin_metallib_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(&builtin_metallib).expect("read compact built-in Metal library"),)
    );
    let cage_metallib_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(&cage_metallib).expect("read Cage-specialized Metal library"))
    );
    let tower_metallib_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(&tower_metallib).expect("read Tower-specialized Metal library"),)
    );
    let stitch_metallib_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(&stitch_metallib).expect("read stitch-host Metal library"))
    );
    let metal_version = output(
        Command::new("xcrun").args(["-sdk", "macosx", "metal", "--version"]),
        "Metal compiler identity",
    );
    let sdk_build = output(
        Command::new("xcrun").args(["-sdk", "macosx", "--show-sdk-build-version"]),
        "Metal SDK identity",
    );
    let metal_compiler_identity = format!(
        "{:x}",
        Sha256::digest(format!("{metal_version}\n{sdk_build}").as_bytes())
    );
    println!("cargo:rustc-env=FPT_METALLIB_PATH={}", metallib.display());
    println!("cargo:rustc-env=FPT_METALLIB_SHA={metallib_sha}");
    println!(
        "cargo:rustc-env=FPT_BUILTIN_METALLIB_PATH={}",
        builtin_metallib.display()
    );
    println!("cargo:rustc-env=FPT_BUILTIN_METALLIB_SHA={builtin_metallib_sha}");
    println!(
        "cargo:rustc-env=FPT_CAGE_METALLIB_PATH={}",
        cage_metallib.display()
    );
    println!("cargo:rustc-env=FPT_CAGE_METALLIB_SHA={cage_metallib_sha}");
    println!(
        "cargo:rustc-env=FPT_TOWER_METALLIB_PATH={}",
        tower_metallib.display()
    );
    println!("cargo:rustc-env=FPT_TOWER_METALLIB_SHA={tower_metallib_sha}");
    println!(
        "cargo:rustc-env=FPT_STITCH_METALLIB_PATH={}",
        stitch_metallib.display()
    );
    println!("cargo:rustc-env=FPT_STITCH_METALLIB_SHA={stitch_metallib_sha}");
    println!("cargo:rustc-env=FPT_METAL_COMPILER_IDENTITY={metal_compiler_identity}");
}
