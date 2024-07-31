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

use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::trace;
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use up_rust::communication::{CallOptions, InMemoryRpcClient, RpcClient, UPayload};
use up_rust::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY;
use up_rust::{UStatus, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const HELLO_SERVICE_ID: u16 = 0x6000;
const HELLO_INSTANCE_ID: u16 = 0x0001;
const HELLO_METHOD_ID: u16 = 0x7FFF;

/// IMPORTANT: should match vsomeip::DEFAULT_MAJOR in (interface/vsomeip/constants.hpp):
// but Autosar works better if vsomeip::DEFAULT_MAJOR is 1 (thus Autosar needs custom vsomeip build)
const HELLO_SERVICE_MAJOR: u8 = 1;
const _HELLO_SERVICE_MINOR: u32 = 0;

const HELLO_SERVICE_AUTHORITY: &str = "linux";
const HELLO_SERVICE_UE_ID: u32 = (HELLO_INSTANCE_ID as u32) << 16 | HELLO_SERVICE_ID as u32;
const HELLO_SERVICE_UE_VERSION_MAJOR: u8 = HELLO_SERVICE_MAJOR;
const HELLO_SERVICE_RESOURCE_ID: u16 = HELLO_METHOD_ID;

const CLIENT_AUTHORITY: &str = "me_authority";
const CLIENT_UE_ID: u32 = 0x5678;
const CLIENT_UE_VERSION_MAJOR: u8 = 1;
const CLIENT_RESOURCE_ID: u16 = 0;

const REQUEST_TTL: u32 = 1000;

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_client");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip_configs/hello_service.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let client_uuri = UUri::try_from_parts(
        CLIENT_AUTHORITY,
        CLIENT_UE_ID,
        CLIENT_UE_VERSION_MAJOR,
        CLIENT_RESOURCE_ID,
    )
    .unwrap();
    let client = Arc::new(
        UPTransportVsomeip::new_with_config(
            client_uuri,
            &HELLO_SERVICE_AUTHORITY.to_string(),
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );

    let l2_client = InMemoryRpcClient::new(client.clone(), client.clone())
        .await
        .unwrap();

    let sink = UUri::try_from_parts(
        HELLO_SERVICE_AUTHORITY,
        HELLO_SERVICE_UE_ID,
        HELLO_SERVICE_UE_VERSION_MAJOR,
        HELLO_SERVICE_RESOURCE_ID,
    )
    .unwrap();

    let mut i = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let hello_request = HelloRequest {
            name: format!("me_client@i={}", i).to_string(),
            ..Default::default()
        };
        i += 1;
        println!("Sending Request message with payload:\n{hello_request:?}");

        let call_options = CallOptions::for_rpc_request(REQUEST_TTL, None, None, None);
        let invoke_res = l2_client
            .invoke_method(
                sink.clone(),
                call_options,
                Some(UPayload::try_from_protobuf(hello_request).unwrap()),
            )
            .await;

        let Ok(response) = invoke_res else {
            panic!(
                "Hit an error attempting to invoke method: {:?}",
                invoke_res.err().unwrap()
            );
        };

        let hello_response_vsomeip_unspecified_payload_format = response.unwrap();
        let hello_response_protobuf_payload_format = UPayload::new(
            hello_response_vsomeip_unspecified_payload_format.payload(),
            UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY,
        );

        let Ok(hello_response) =
            hello_response_protobuf_payload_format.extract_protobuf::<HelloResponse>()
        else {
            panic!("Unable to parse into HelloResponse");
        };

        println!("Here we received response: {hello_response:?}");
    }
}
