use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=macos/recorder.m");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS recorder architecture"),
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("hoi4-window-recorder");
    let status = Command::new("xcrun")
        .args([
            "clang",
            "-arch",
            arch,
            "-mmacosx-version-min=12.3",
            "-fobjc-arc",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wno-unused-parameter",
            "-framework",
            "Foundation",
            "-framework",
            "ScreenCaptureKit",
            "-framework",
            "AVFoundation",
            "-framework",
            "ApplicationServices",
            "-framework",
            "CoreMedia",
            "macos/recorder.m",
            "-o",
        ])
        .arg(output)
        .status()
        .expect("macOS recording bridge requires Xcode command line tools");
    assert!(
        status.success(),
        "macOS recording bridge compilation failed"
    );
}
