fn main() {
    println!("cargo:rerun-if-changed=web/dist");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    assert!(
        root.join("web/dist/phone.html").is_file() && root.join("web/dist/admin.html").is_file(),
        "Build the web assets first: cd web && npm ci --ignore-scripts && npm run build"
    );
}
