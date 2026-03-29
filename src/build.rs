use std::env;
use std::path::PathBuf;

fn main() {
    #[cfg(target_os = "linux")]
    {
        // Check for X11 libraries
        println!("cargo:rustc-link-lib=X11");
        println!("cargo:rustc-link-lib=Xrandr");
        println!("cargo:rustc-link-lib=Xinerama");
        println!("cargo:rustc-link-lib=Xcursor");
        println!("cargo:rustc-link-lib=Xi");
        
        // Check for Wayland libraries (optional)
        if env::var("CARGO_FEATURE_WAYLAND").is_ok() {
            println!("cargo:rustc-link-lib=wayland-client");
            println!("cargo:rustc-link-lib=wayland-egl");
        }
        
        // Set library search paths
        let lib_paths = vec![
            "/usr/lib/x86_64-linux-gnu",
            "/usr/lib64",
            "/usr/lib",
        ];
        
        for path in lib_paths {
            println!("cargo:rustc-link-search={}", path);
        }
        
        // Set pkg-config for better detection
        if let Ok(pkg_config) = which::which("pkg-config") {
            println!("cargo:rustc-env=PKG_CONFIG_PATH={}", pkg_config.display());
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rustc-link-lib=gdi32");
    }
    
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Cocoa");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
