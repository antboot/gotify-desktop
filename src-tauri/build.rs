fn main() {
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/macos_app_info.m")
            .flag("-fobjc-arc")
            .compile("macos_app_info");
        println!("cargo:rerun-if-changed=src/macos_app_info.m");
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=UniformTypeIdentifiers");
    }

    tauri_build::build();
}
