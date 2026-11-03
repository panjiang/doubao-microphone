fn main() {
    println!("cargo:rerun-if-changed=src/umdf_native.c");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .file("src/umdf_native.c")
            .compile("ptt_umdf");
        println!("cargo:rustc-link-lib=hid");
        println!("cargo:rustc-link-lib=setupapi");
        println!("cargo:rustc-link-lib=user32");
    }
    println!("cargo:rerun-if-changed=web/dist");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    assert!(
        root.join("web/dist/phone.html").is_file() && root.join("web/dist/admin.html").is_file(),
        "Build the web assets first: cd web && npm ci --ignore-scripts && npm run build"
    );
}
