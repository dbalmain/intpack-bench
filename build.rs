fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let v = std::process::Command::new(rustc).arg("--version").output().ok();
    let v = v.and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    println!("cargo:rustc-env=IPB_RUSTC={}", v.trim());
    println!("cargo:rerun-if-changed=build.rs");
}
