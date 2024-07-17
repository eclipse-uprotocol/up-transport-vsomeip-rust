/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

// use prost_build::Config;
use protobuf_codegen::Customize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> std::io::Result<()> {
    // use vendored protoc instead of relying on user provided protobuf installation
    env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());

    // if let Err(err) = get_and_build_protos(
    if let Err(err) = get_and_build_protos(
        &[
            "https://raw.githubusercontent.com/protocolbuffers/protobuf/main/src/google/protobuf/descriptor.proto",
            "https://raw.githubusercontent.com/googleapis/googleapis/master/google/type/timeofday.proto",
            "https://raw.githubusercontent.com/eclipse-uprotocol/up-spec/main/up-core-api/uprotocol/uoptions.proto",
            "https://raw.githubusercontent.com/COVESA/uservices/main/src/main/proto/example/hello_world/v1/hello_world_topics.proto",
            "https://raw.githubusercontent.com/PLeVasseur/uservices/feature/update-uprotocol-options-path/src/main/proto/example/hello_world/v1/hello_world_service.proto",
        ],
    "helloworld",
    ) {
        let error_message = format!("Failed to fetch and build protobuf file: {err:?}");
        return Err(std::io::Error::new(std::io::ErrorKind::Other, error_message));
    }

    Ok(())
}

// Fetch protobuf definitions from `url`, and build them
fn get_and_build_protos(
    urls: &[&str],
    output_folder: &str,
) -> core::result::Result<(), Box<dyn std::error::Error>> {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let proto_folder = Path::new(&out_dir).join("proto");
    let mut proto_files = Vec::new();

    for url in urls {
        let file_name = url.split('/').last().unwrap();
        let mut file_path_buf = PathBuf::from(&proto_folder);

        // Check if the URL is from googleapis to determine the correct path
        if url.contains("googleapis/googleapis") {
            file_path_buf.push("google/type");
        }

        if url.contains("protocolbuffers/protobuf") {
            file_path_buf.push("google/protobuf");
        }

        if url.contains("example/hello_world/v1") {
            file_path_buf.push("example/hello_world/v1")
        }

        if url.contains("uprotocol/uoptions.proto") {
            file_path_buf.push("uprotocol");
        }

        file_path_buf.push(file_name); // Push the file name to the path buffer

        // Create the directory path if it doesn't exist
        if let Some(parent) = file_path_buf.parent() {
            fs::create_dir_all(parent)?;
        }

        // Download the .proto file
        if let Err(err) = download_and_write_file(url, &file_path_buf) {
            panic!("Failed to download and write file: {err:?}");
        }

        proto_files.push(file_path_buf);
    }

    protobuf_codegen::Codegen::new()
        .protoc()
        // use vendored protoc instead of relying on user provided protobuf installation
        .protoc_path(&protoc_bin_vendored::protoc_bin_path().unwrap())
        .customize(Customize::default().tokio_bytes(true))
        .include(proto_folder)
        .inputs(proto_files)
        .cargo_out_dir(output_folder)
        .run_from_script();

    Ok(())
}

fn download_and_write_file(
    url: &str,
    dest_path: &PathBuf,
) -> core::result::Result<(), Box<dyn std::error::Error>> {
    // Send a GET request to the URL
    reqwest::blocking::get(url)
        .map_err(Box::from)
        .and_then(|mut response| {
            if let Some(parent_path) = dest_path.parent() {
                std::fs::create_dir_all(parent_path)?;
            }
            let mut out_file = fs::File::create(dest_path)?;
            response
                .copy_to(&mut out_file)
                .map(|_| ())
                .map_err(|e| e.to_string().into())
        })
}
