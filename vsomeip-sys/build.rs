/********************************************************************************
 * Copyright (c) 2024 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use std::env;
use std::path::PathBuf;

#[cfg(feature = "bundled")]
use std::path::Path;
#[cfg(feature = "bundled")]
use std::{fs, io};

#[cfg(feature = "bundled")]
fn vsomeip_includes() -> PathBuf {
    let crate_root =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR environment variable is not set");
    PathBuf::from(&crate_root).join("vsomeip").join("interface")
}

#[cfg(not(feature = "bundled"))]
fn vsomeip_includes() -> PathBuf {
    let vsomeip_install_path = env::var("VSOMEIP_INSTALL_PATH")
        .expect("You must supply the path to a vsomeip library install, e.g. /usr/local");
    PathBuf::from(&vsomeip_install_path).join("include")
}

#[cfg(feature = "bundled")]
fn vsomeip_install_path() -> String {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let vsomeip_install_path_default = format!(
        "{}",
        PathBuf::from(out_dir)
            .join("vsomeip")
            .join("vsomeip-install")
            .display()
    );
    env::var("VSOMEIP_INSTALL_PATH").unwrap_or(vsomeip_install_path_default)
}

#[cfg(feature = "bundled")]
fn vsomeip_lib_path() -> String {
    let vsomeip_install_path = vsomeip_install_path();
    let vsomeip_lib_path = PathBuf::from(&vsomeip_install_path).join("lib");
    format!("{}", vsomeip_lib_path.display())
}

#[cfg(not(feature = "bundled"))]
fn vsomeip_lib_path() -> String {
    let vsomeip_install_path = env::var("VSOMEIP_INSTALL_PATH")
        .expect("You must supply the path to a vsomeip library install, e.g. /usr/local");
    let vsomeip_lib_path = PathBuf::from(&vsomeip_install_path).join("lib");
    format!("{}", vsomeip_lib_path.display())
}

fn main() -> miette::Result<()> {
    #[cfg(feature = "bundled")]
    build::build();

    let vsomeip_interface_path = vsomeip_includes();
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let generic_cpp_stdlib = env::var("GENERIC_CPP_STDLIB_PATH")
        .expect("You must supply the path to generic C++ stdlib, e.g. /usr/include/c++/11");
    let arch_specific_cpp_stdlib = env::var("ARCH_SPECIFIC_CPP_STDLIB_PATH").expect("You must supply the path to architecture-specific C++ stdlib, e.g. /usr/include/x86_64-linux-gnu/c++/11");

    let project_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let runtime_wrapper_dir = project_root.join("src/glue/include"); // Update the path as necessary

    // Somewhat useful debugging
    println!(
        "debug: vsomeip_interface_path  : {}",
        vsomeip_interface_path.display()
    );
    println!("debug: generic_cpp_stdlib  : {}", generic_cpp_stdlib);
    println!(
        "debug: arch_specific_cpp_stdlib  : {}",
        arch_specific_cpp_stdlib
    );

    bindings::generate_bindings(
        &out_dir,
        &vsomeip_interface_path,
        &generic_cpp_stdlib,
        &arch_specific_cpp_stdlib,
        project_root,
        &runtime_wrapper_dir,
    )?;

    Ok(())
}

#[cfg(feature = "bundled")]
mod build {
    use cmake::Config;
    use std::env;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::copy_dir_all;

    pub fn build() {
        let submodule_folder = "vsomeip";

        let crate_root = env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR environment variable is not set");

        let patch_folder = PathBuf::from(&crate_root).join("patches");
        println!("debug: patch_folder: {}", patch_folder.display());

        let submodule_git = PathBuf::from(&crate_root).join(format!("{}/.git", submodule_folder));
        println!("debug: submodule_git: {:?}", submodule_git);

        // Make sure that the Git submodule is checked out
        if !Path::new(&submodule_git).exists() {
            let submodule_checkout = Command::new("git")
                .arg("-C")
                .arg(submodule_folder)
                .arg("submodule")
                .arg("update")
                .arg("--init")
                .status();

            println!("debug: submodule_checkout: {:?}", submodule_checkout);
        }

        let out_dir = env::var_os("OUT_DIR").unwrap();
        let submodule_to_patch = PathBuf::from(&crate_root).join(submodule_folder);
        let vsomeip_build_dir = PathBuf::from(out_dir).join("vsomeip").join("vsomeip_build");
        let copy_res = copy_dir_all(&submodule_to_patch, &vsomeip_build_dir);
        println!(
            "debug: copying vsomeip to output directory to build: {:?}",
            copy_res
        );

        println!(
            "debug: trying to apply patch to: {}",
            vsomeip_build_dir.display()
        );
        let disable_test_patch = patch_folder.join("disable_tests.patch");
        let disable_test_patch_str = format!("{}", disable_test_patch.display());
        println!("disable_test_patch_str: {}", disable_test_patch_str);
        let output = Command::new("patch")
            .arg("-d")
            .arg(&vsomeip_build_dir)
            .arg("-p1")
            .arg("-i")
            .arg(disable_test_patch_str)
            .output()
            .expect("Failed to apply patch");
        println!("debug: after trying to apply patch: {:?}", output);

        let out_dir = env::var_os("OUT_DIR").unwrap();
        let vsomeip_install_path_default = format!(
            "{}",
            PathBuf::from(out_dir)
                .join("vsomeip")
                .join("vsomeip-install")
                .display()
        );
        let vsomeip_install_path =
            env::var("VSOMEIP_INSTALL_PATH").unwrap_or(vsomeip_install_path_default);

        println!("debug: vsomeip_project_root set");
        println!(
            "debug: vsomeip_project_root: {}",
            vsomeip_build_dir.display()
        );
        println!("debug: vsomeip_install_path: {}", vsomeip_install_path);

        let vsomeip_cmake_build = Config::new(vsomeip_build_dir)
            .define("CMAKE_INSTALL_PREFIX", vsomeip_install_path.clone())
            .define("ENABLE_SIGNAL_HANDLING", "1")
            .build_target("install")
            .build();

        println!(
            "debug: vsomeip_cmake_build: {}",
            vsomeip_cmake_build.display()
        );
    }
}
mod bindings {
    use crate::vsomeip_lib_path;
    use std::ffi::OsString;
    use std::fs;
    use std::fs::File;
    use std::io::{BufRead, BufReader, BufWriter, Write};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    pub fn generate_bindings(
        out_dir: &OsString,
        vsomeip_interface_path: &PathBuf,
        generic_cpp_stdlib: &String,
        arch_specific_cpp_stdlib: &String,
        project_root: PathBuf,
        runtime_wrapper_dir: &PathBuf,
    ) -> miette::Result<()> {
        // we use autocxx to generate bindings for all those requested in src/lib.rs in the include_cpp! {} macro
        let mut b = autocxx_build::Builder::new(
            "src/lib.rs",
            [&vsomeip_interface_path, &runtime_wrapper_dir],
        )
        .extra_clang_args(&[
            format!("-I{}", generic_cpp_stdlib).as_str(),
            format!("-I{}", arch_specific_cpp_stdlib).as_str(),
            format!("-I{}", vsomeip_interface_path.display()).as_str(),
        ])
        .build()?;
        b.flag_if_supported("-std=c++17")
            .flag_if_supported("-Wno-deprecated-declarations") // suppress warnings from C++
            .flag_if_supported("-Wno-unused-function") // compiler compiling vsomeip
            .compile("autocxx-portion");
        println!("cargo:rerun-if-changed=src/lib.rs");
        println!("cargo:rustc-link-lib=vsomeip3");
        let vsomeip_lib_path = vsomeip_lib_path();
        println!("cargo:rustc-link-search=native={}", vsomeip_lib_path);

        let include_dir = project_root.join("src/glue"); // Update the path as necessary

        // we use cxx to generate bindings for those couple of functions for which autocxx fails
        // due to usage of std::function inside of the function body
        cxx_build::bridge("src/cxx_bridge.rs")
            .file("src/glue/application_registrations.cpp")
            .file("src/glue/src/application_wrapper.cpp")
            .include(&include_dir)
            .include(vsomeip_interface_path)
            .include(runtime_wrapper_dir)
            .flag_if_supported("-Wno-deprecated-declarations") // suppress warnings from C++
            .flag_if_supported("-Wno-unused-function") // compiler compiling vsomeip
            .flag_if_supported("-std=c++17")
            .extra_warnings(true)
            .compile("cxx-portion");
        println!("cargo:rerun-if-changed=src/cxx_bridge.rs");

        // we rewrite the autocxx generated code to suppress the cargo warning about unused imports
        let file_path = Path::new(&out_dir)
            .join("autocxx-build-dir")
            .join("rs")
            .join("autocxx-ffi-default-gen.rs");
        println!("debug: file_path : {}", file_path.display());

        if !file_path.exists() {
            panic!("Unable to find autocxx generated code to rewrite");
        }

        // Run rustfmt on the file
        let status = Command::new("rustfmt")
            .arg(&file_path)
            .status()
            .expect("Failed to execute rustfmt");

        if !status.success() {
            panic!("Failed to format autocxx generated file");
        }

        // Open the input file for reading
        let input_file = File::open(&file_path).expect("Failed to open the input file for reading");
        let reader = BufReader::new(input_file);

        // Create a temporary file for writing the modified content
        let temp_file_path = file_path.with_extension("tmp");
        let temp_file =
            File::create(&temp_file_path).expect("Failed to create a temporary file for writing");
        let mut writer = BufWriter::new(temp_file);

        for line in reader.lines() {
            let mut line = line.expect("Failed to read a line from the input file");
            line = fix_unused_imports(line);
            line = fix_unsafe_fn_unused(line);
            line = fix_doc_build(line);
            line = add_enum_debug(line);

            writeln!(writer, "{}", line).expect("Failed to write a line to the temporary file");
        }

        writer.flush().expect("Failed to flush the writer buffer");
        fs::rename(temp_file_path, file_path)
            .expect("Failed to rename the temporary file to the original file");

        println!("debug: rewrote the autocxx file");
        Ok(())
    }

    fn fix_unused_imports(line: String) -> String {
        let mut fixed_line = line.replace(
            "pub use bindgen::root::std_chrono_duration_long_AutocxxConcrete;",
            "",
        );
        fixed_line = fixed_line.replace(
            "pub use bindgen::root::std_chrono_duration_int64_t_AutocxxConcrete;",
            "",
        );
        fixed_line = fixed_line.replace(
            "pub use super::super::bindgen::root::std::chrono::seconds;",
            "",
        );

        fixed_line
    }

    fn fix_unsafe_fn_unused(line: String) -> String {
        let mut fixed_line =
            line.replace("pub unsafe fn create_payload1", "unsafe fn create_payload1");
        fixed_line = fixed_line.replace(
            "pub unsafe fn set_data(self: Pin<&mut payload>, _data: *const u8, _length: u32);",
            "pub(crate) unsafe fn set_data(self: Pin<&mut payload>, _data: *const u8, _length: u32);",
        );

        fixed_line
    }

    fn fix_doc_build(line: String) -> String {
        // we may have to fix more in the future
        #[allow(clippy::let_and_return)]
        let fixed_line = line.replace("successfully [de]registered", "successfully de/registered");
        fixed_line
    }

    fn add_enum_debug(line: String) -> String {
        // we may have to fix more in the future
        #[allow(clippy::let_and_return)]
        let fixed_line = line.replace(
            "#[derive(Clone, Hash, PartialEq, Eq)]",
            "#[derive(Clone, Hash, PartialEq, Eq, Debug)]",
        );
        fixed_line
    }
}

#[cfg(feature = "bundled")]
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
