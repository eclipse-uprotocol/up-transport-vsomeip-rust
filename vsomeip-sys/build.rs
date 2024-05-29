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

use decompress::ExtractOptsBuilder;
use reqwest::blocking::Client;
use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs};

const VSOMEIP_TAGGED_RELEASE_BASE: &str = "https://github.com/COVESA/vsomeip/archive/refs/tags/";
const VSOMEIP_VERSION_ARCHIVE: &str = "3.4.10.tar.gz";

fn main() -> miette::Result<()> {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let vsomeip_archive_dest = Path::new(&out_dir).join("vsomeip").join("vsomeip.tar.gz");
    let vsomeip_decompressed_folder = Path::new(&out_dir).join("vsomeip").join("vsomeip-src");
    let vsomeip_archive_url = format!("{VSOMEIP_TAGGED_RELEASE_BASE}{VSOMEIP_VERSION_ARCHIVE}");

    download_and_write_file(&vsomeip_archive_url, &vsomeip_archive_dest)
        .expect("Unable to download released archive");
    decompress::decompress(
        vsomeip_archive_dest,
        vsomeip_decompressed_folder.clone(),
        &ExtractOptsBuilder::default().strip(1).build().unwrap(),
    )
    .expect("Unable to extract tar.gz");

    let project_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let runtime_wrapper_dir = project_root.join("src/glue/include"); // Update the path as necessary
    let interface_path = vsomeip_decompressed_folder.join("interface");

    // for some reason unless we explicitly provide paths to headers for the stdlib here we have issues
    // I don't think we should really _have_ to do this though, as their locations are
    // more or less consistent on every instance of the different platforms
    // reference: https://github.com/google/autocxx/issues/1347#issuecomment-1928551787

    // we use autocxx to generate bindings for all those requested in src/lib.rs in the include_cpp! {} macro
    let mut b = autocxx_build::Builder::new("src/lib.rs", [&interface_path, &runtime_wrapper_dir])
        .extra_clang_args(&[
            "-I/usr/include/c++/11",
            "-I/usr/include/x86_64-linux-gnu/c++/11",
        ])
        .build()?;
    b.flag_if_supported("-std=c++17")
        .flag_if_supported("-Wno-deprecated-declarations") // suppress warnings from C++
        .flag_if_supported("-Wno-unused-function") // compiler compiling vsomeip
        .compile("autocxx-portion");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rustc-link-lib=vsomeip3");
    println!("cargo:rustc-link-search=native=/usr/local/lib");

    let include_dir = project_root.join("src/glue"); // Update the path as necessary

    // we use cxx to generate bindings for those couple of functions for which autocxx fails
    // due to usage of std::function inside of the function body
    cxx_build::bridge("src/cxx_bridge.rs")
        .file("src/glue/application_registrations.cpp")
        .file("src/glue/src/application_wrapper.cpp")
        .include(&include_dir)
        .flag_if_supported("-std=c++17")
        .compile("cxx-portion");
    println!("cargo:rerun-if-changed=src/cxx_bridge.rs");

    // we rewrite the autocxx generated code to suppress the cargo warning about unused imports
    let file_path = Path::new(&out_dir)
        .join("autocxx-build-dir")
        .join("rs")
        .join("autocxx-ffi-default-gen.rs");
    if let Ok(mut contents) = fs::read_to_string(&file_path) {
        // Insert #[allow(unused_imports)] for specific lines
        contents = contents.replace(
            "pub use bindgen :: root :: std_chrono_duration_int64_t_AutocxxConcrete ;",
            "#[allow(unused_imports)]  pub use bindgen :: root :: std_chrono_duration_int64_t_AutocxxConcrete ;"
        );

        contents = contents.replace(
            "pub use super :: super :: bindgen :: root :: std :: chrono :: seconds ;",
            "#[allow(unused_imports)]  pub use super :: super :: bindgen :: root :: std :: chrono :: seconds ;"
        );

        // Removing pub from an unsafe function we never use to suppress warning
        contents = contents.replace("pub unsafe fn create_payload1", "unsafe fn create_payload1");

        // Rewriting a doc comment translated from C++ to not use [] link syntax
        contents = contents.replace("successfully [de]registered", "successfully de/registered");

        // Adding a derived Debug for the message_type_e enum
        contents = contents.replace(
            "# [repr (u8)] # [derive (Clone , Hash , PartialEq , Eq)] pub enum message_type_e",
              "# [repr (u8)] # [derive (Clone , Hash , PartialEq , Eq, Debug)] pub enum message_type_e"
        );

        fs::write(&file_path, contents).expect("Unable to write file");
    }

    Ok(())
}

// Retrieves a file from `url` (from GitHub, for instance) and places it in the build directory (`OUT_DIR`) with the name
// provided by `destination` parameter.
fn download_and_write_file(url: &str, dest_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120)) // Set a timeout of 60 seconds
        .build()?;
    let mut retries = 3;

    while retries > 0 {
        match client.get(url).send() {
            Ok(response) => {
                // Log the response headers
                println!("Headers: {:?}", response.headers());

                // Check rate limiting headers
                let rate_limit_remaining = response.headers().get("X-RateLimit-Remaining");
                let rate_limit_reset = response.headers().get("X-RateLimit-Reset");
                println!("Rate Limit Remaining: {:?}", rate_limit_remaining);
                println!("Rate Limit Reset: {:?}", rate_limit_reset);

                // Get the response body as bytes
                let response_body = response.bytes()?;
                println!("Body length: {:?}", response_body.len());

                // Create parent directories if necessary
                if let Some(parent_path) = dest_path.parent() {
                    std::fs::create_dir_all(parent_path)?;
                }

                // Create or open the destination file
                let mut out_file = fs::File::create(dest_path)?;

                // Write the response body to the file
                let result: Result<(), Box<dyn Error>> = out_file
                    .write_all(&response_body)
                    .map_err(|e| e.to_string().into());

                // Return the result if successful
                if result.is_ok() {
                    return result;
                } else {
                    println!("Error copying response body to file: {:?}", result);
                }
            }
            Err(e) => {
                println!("Error: {:?}", e);
                retries -= 1;
                if retries > 0 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                } else {
                    return Err(Box::from(e));
                }
            }
        }
    }

    Err("Failed to download file after multiple attempts".into())
}
// fn download_and_write_file(
//     url: &str,
//     dest_path: &PathBuf,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     // Send a GET request to the URL
//     match reqwest::blocking::get(url) {
//         Ok(mut response) => {
//             if let Some(parent_path) = dest_path.parent() {
//                 std::fs::create_dir_all(parent_path)?;
//             }
//             let mut out_file = fs::File::create(dest_path)?;
//
//             let result: Result<(), Box<dyn std::error::Error>> = response
//                 .copy_to(&mut out_file)
//                 .map(|_| ())
//                 .map_err(|e| e.to_string().into());
//
//             result
//         }
//         Err(e) => Err(Box::from(e)),
//     }
// }
