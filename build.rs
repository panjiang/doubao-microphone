fn main() {
    println!("cargo:rerun-if-changed=src/macos_native.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new().file("src/macos_native.m").flag("-fobjc-arc").compile("ptt_macos");
        for framework in ["AppKit", "ApplicationServices", "SystemConfiguration", "Carbon"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
    println!("cargo:rerun-if-changed=src/umdf_native.c");
    println!("cargo:rerun-if-changed=src/windows_network.c");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut native = cc::Build::new();
        if std::env::var_os("CARGO_FEATURE_WINDOWS_UMDF_TRIAL").is_some() {
            native.define("DOUBAO_UMDF_TRIAL", None);
        }
        native
            .file("src/umdf_native.c")
            .file("src/windows_network.c")
            .compile("ptt_umdf");
        println!("cargo:rustc-link-lib=hid");
        println!("cargo:rustc-link-lib=setupapi");
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=iphlpapi");
        embed_windows_icon();
    }
    println!("cargo:rerun-if-changed=web/dist");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    assert!(
        root.join("web/dist/phone.html").is_file() && root.join("web/dist/admin.html").is_file(),
        "Build the web assets first: cd web && npm ci --ignore-scripts && npm run build"
    );
}

fn embed_windows_icon() {
    use std::{env, path::PathBuf, process::Command};
    println!("cargo:rerun-if-changed=packaging/windows-icon.rc");
    println!("cargo:rerun-if-changed=assets/icons/doubao-microphone.ico");
    println!("cargo:rerun-if-env-changed=RC");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("windows-icon.res");
    let compiler = cc::Build::new().get_compiler();
    let rc = env::var_os("RC")
        .map(PathBuf::from)
        .or_else(|| {
            compiler.get_envs()
                .rfind(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("PATH"))
                .and_then(|(_, paths)| {
                    env::split_paths(paths)
                        .map(|path| path.join("rc.exe"))
                        .find(|path| path.is_file())
                })
        })
        .unwrap_or_else(|| PathBuf::from("rc.exe"));
    let status = Command::new(rc)
        .envs(compiler.get_envs())
        .current_dir(root.join("packaging"))
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg("windows-icon.rc")
        .status()
        .expect("Windows SDK resource compiler (rc.exe) is required");
    assert!(
        status.success(),
        "Compiling the Windows application icon failed"
    );
    println!("cargo:rustc-link-arg-bin=doubao-microphone={}", resource.display());
}
