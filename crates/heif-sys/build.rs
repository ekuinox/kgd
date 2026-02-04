fn main() {
    #[cfg(not(unix))]
    eprintln!("TODO: support build without unix");

    #[cfg(unix)]
    self::unix::build();
}

#[cfg(unix)]
mod unix {
    use std::{env, path::PathBuf, process::Command};

    fn has_ninja() -> bool {
        Command::new("ninja")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn build() {
        // libheif のソースが変更されたときに再ビルドする
        println!("cargo:rerun-if-changed=libheif");

        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let build_dir = out_dir.join("build");

        let mut config = cmake::Config::new("libheif");
        config.out_dir(&build_dir);

        if has_ninja() {
            config.generator("Ninja");
        }

        config
            .define("BUILD_SHARED_LIBS", "OFF")
            .define("WITH_PLUGIN_LOADING", "OFF")
            .define("WITH_LIBDE265", "ON")
            .define("WITH_JPEG_ENCODER", "ON")
            .define("WITH_JPEG_DECODER", "ON")
            // libsharpyuv は Ubuntu 22.04 の libwebp 1.2 には含まれていないため無効化
            // (libwebp 1.3+ で追加された)
            .define("WITH_LIBSHARPYUV", "OFF");

        // クロスコンパイル時、CMake がターゲットアーキテクチャのライブラリを見つけられるようにする
        if let Ok(target) = env::var("TARGET")
            && target.contains("aarch64")
            && target.contains("linux")
        {
            // pkg-config がターゲットアーキテクチャのライブラリを見つけられるようにする
            config.env("PKG_CONFIG_PATH", "/usr/lib/aarch64-linux-gnu/pkgconfig");
            config.env("PKG_CONFIG_LIBDIR", "/usr/lib/aarch64-linux-gnu/pkgconfig");
            config.env("PKG_CONFIG_SYSROOT_DIR", "/");

            // 依存ライブラリのパスを明示的に指定
            let lib_dir = "/usr/lib/aarch64-linux-gnu";
            let include_dir = "/usr/include";

            config.define("LIBDE265_INCLUDE_DIR", include_dir);
            config.define("LIBDE265_LIBRARY", format!("{}/libde265.so", lib_dir));
            config.define("X265_INCLUDE_DIR", include_dir);
            config.define("X265_LIBRARY", format!("{}/libx265.so", lib_dir));
            config.define("AOM_INCLUDE_DIR", include_dir);
            config.define("AOM_LIBRARY", format!("{}/libaom.so", lib_dir));
            config.define("JPEG_INCLUDE_DIR", include_dir);
            config.define("JPEG_LIBRARY", format!("{}/libjpeg.so", lib_dir));
            config.define("ZLIB_INCLUDE_DIR", include_dir);
            config.define("ZLIB_LIBRARY", format!("{}/libz.so", lib_dir));
        }

        let dst = config.build();

        println!("cargo:rustc-link-search=native={}/lib", dst.display());
        println!("cargo:rustc-link-lib=static=heif");

        // C++ standard library
        println!("cargo:rustc-link-lib=dylib=stdc++");

        // libheif dependencies (system libraries)
        println!("cargo:rustc-link-lib=dylib=de265");
        println!("cargo:rustc-link-lib=dylib=x265");
        println!("cargo:rustc-link-lib=dylib=aom");
        println!("cargo:rustc-link-lib=dylib=z");
        println!("cargo:rustc-link-lib=dylib=jpeg");

        let header_path = dst.join("include/libheif/heif.h");

        let bindings = bindgen::Builder::default()
            .header(header_path.to_str().unwrap())
            .clang_arg(format!("-I{}/include", dst.display()))
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .generate()
            .expect("Unable to generate bindings");

        bindings
            .write_to_file(out_dir.join("bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
