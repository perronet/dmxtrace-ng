extern crate bindgen;

use std::env;
use std::process::Command;
use std::path::PathBuf;

static LIB_DIR: &str = "vendor";
static LIB_OUTPUT_DIR: &str = "vendor/output/usr/lib64";

fn main() {
    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo:rerun-if-changed={}/libtraceevent", LIB_DIR);
    println!("cargo:rerun-if-changed={}/libtracefs", LIB_DIR);
    println!("cargo:rerun-if-changed={}/trace-cmd", LIB_DIR);

    // Compile the trace-cmd libraries.
    Command::new("./build_libs.sh")
    .current_dir(LIB_DIR)
    .status()
    .expect("Failed to build trace-cmd libraries.");

    // Linker options for rustc (link trace-cmd libraries).
    println!("cargo:rustc-link-lib=dylib=traceevent");
    println!("cargo:rustc-link-lib=dylib=tracefs");
    println!("cargo:rustc-link-lib=dylib=tracecmd");
    println!("cargo:rustc-link-search=native=rbftrace-tracing/{}", LIB_OUTPUT_DIR);

    // Without this we won't be able to find the library when running cargo run
    // Note: this *only works* with cargo run
    println!("cargo:rustc-env=LD_LIBRARY_PATH={}", LIB_OUTPUT_DIR);

    // Generate bindings
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .clang_arg(format!("-I{}/libtraceevent/src", LIB_DIR))
        .clang_arg(format!("-I{}/libtracefs/include", LIB_DIR))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_path.join("bindings.rs")).expect("Couldn't write bindings!");
}
