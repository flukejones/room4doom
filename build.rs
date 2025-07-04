use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(Debug)");
    // #[cfg(all(target_os = "macos", feature = "sdl2-bundled"))]
    // println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");

    //#[cfg(all(target_os = "linux", feature = "sdl2-bundled"))]
    //println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

    let target = env::var("TARGET").unwrap();
    if target.contains("pc-windows") {
        let mut manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        manifest_dir.pop();
        let mut lib_dir = manifest_dir.clone();
        let mut dll_dir = manifest_dir.clone();
        if target.contains("msvc") {
            lib_dir.push("msvc");
            dll_dir.push("msvc");
        } else {
            lib_dir.push("gnu-mingw");
            dll_dir.push("gnu-mingw");
        }
        lib_dir.push("lib");
        dll_dir.push("dll");
        if target.contains("x86_64") {
            lib_dir.push("64");
            dll_dir.push("64");
        } else {
            lib_dir.push("32");
            dll_dir.push("32");
        }
        println!("cargo:rustc-link-search=all={}", lib_dir.display());
        for entry in std::fs::read_dir(dll_dir).expect("Can't read DLL dir") {
            let entry_path = entry.expect("Invalid fs entry").path();
            let file_name_result = entry_path.file_name();
            let mut new_file_path = manifest_dir.parent().unwrap().to_path_buf();
            if let Some(file_name) = file_name_result {
                let file_name = file_name.to_str().unwrap();
                if file_name.ends_with(".dll") {
                    new_file_path.push(file_name);
                    std::fs::copy(&entry_path, new_file_path).expect("Can't copy from DLL dir");
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
}
