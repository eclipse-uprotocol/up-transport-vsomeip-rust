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

use async_trait::async_trait;
use hello_world_protos::hello_world_service::{HelloRequest, HelloResponse};
use log::{error, trace};
use std::fs::canonicalize;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use up_rust::communication::{
    InMemoryRpcServer, RequestHandler, RpcServer, ServiceInvocationError, UPayload,
};
use up_rust::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY;
use up_rust::{UAttributes, UCode, UStatus, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const HELLO_SERVICE_ID: u16 = 0x6000;
const HELLO_INSTANCE_ID: u32 = 0x0001;
const HELLO_METHOD_ID: u16 = 0x7FFF;

/// IMPORTANT: should match vsomeip::DEFAULT_MAJOR in (interface/vsomeip/constants.hpp):
// but Autosar works better if vsomeip::DEFAULT_MAJOR is 1 (thus Autosar needs custom vsomeip build)
const HELLO_SERVICE_MAJOR: u8 = 1;
const _HELLO_SERVICE_MINOR: u32 = 0;

const HELLO_SERVICE_AUTHORITY: &str = "linux";
const HELLO_SERVICE_UE_ID: u32 = (HELLO_INSTANCE_ID << 16u32) | HELLO_SERVICE_ID as u32;
const _HELLO_SERVICE_UE_VERSION_MAJOR: u8 = HELLO_SERVICE_MAJOR;
const HELLO_SERVICE_RESOURCE_ID: u16 = HELLO_METHOD_ID;

const CLIENT_AUTHORITY: &str = "me_authority";
const _CLIENT_UE_ID: u16 = 0x5678;
const _CLIENT_UE_VERSION_MAJOR: u8 = 1;
const _CLIENT_RESOURCE_ID: u16 = 0;

const _REQUEST_TTL: u32 = 1000;

struct ServiceRequestHandler;
impl ServiceRequestHandler {
    pub fn new() -> Self {
        Self
    }
}
#[async_trait]
impl RequestHandler for ServiceRequestHandler {
    async fn handle_request(
        &self,
        resource_id: u16,
        _uattributes: &UAttributes,
        request_payload: Option<UPayload>,
    ) -> Result<Option<UPayload>, ServiceInvocationError> {
        println!("ServiceRequestHandler: Received a resource_id: {resource_id} request_payload: {request_payload:?}");

        let hello_request_vsomeip_unspecified_payload_format = request_payload.unwrap();
        let hello_request_protobuf_payload_format = UPayload::new(
            hello_request_vsomeip_unspecified_payload_format.payload(),
            UPAYLOAD_FORMAT_PROTOBUF_WRAPPED_IN_ANY,
        );
        let hello_request =
            hello_request_protobuf_payload_format.extract_protobuf::<HelloRequest>();

        let hello_request = match hello_request {
            Ok(hello_request) => {
                println!("hello_request: {hello_request:?}");
                hello_request
            }
            Err(err) => {
                error!("Unable to parse HelloRequest: {err:?}");
                return Err(ServiceInvocationError::RpcError(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "Unable to parse hello_request",
                )));
            }
        };

        let hello_response = HelloResponse {
            message: format!("The response to the request: {}", hello_request.name),
            ..Default::default()
        };

        println!("Making response to send back: {hello_response:?}");

        Ok(Some(UPayload::try_from_protobuf(hello_response).unwrap()))
    }
}

#[tokio::main]
async fn main() -> Result<(), UStatus> {
    env_logger::init();

    println!("mE_service");

    let crate_dir = env!("CARGO_MANIFEST_DIR");
    // TODO: Make configurable to pass the path to the vsomeip config as a command line argument
    let vsomeip_config = PathBuf::from(crate_dir).join("vsomeip_configs/hello_service.json");
    let vsomeip_config = canonicalize(vsomeip_config).ok();
    trace!("vsomeip_config: {vsomeip_config:?}");

    // There will be a single vsomeip_transport, as there is a connection into device and a streamer
    // TODO: Add error handling if we fail to create a UPTransportVsomeip
    let service_uuri = UUri::try_from_parts(
        HELLO_SERVICE_AUTHORITY,
        HELLO_SERVICE_UE_ID,
        HELLO_SERVICE_MAJOR,
        // HELLO_SERVICE_RESOURCE_ID,
        0,
    )
    .unwrap();
    let service = Arc::new(
        UPTransportVsomeip::new_with_config(
            service_uuri,
            &CLIENT_AUTHORITY.to_string(),
            &vsomeip_config.unwrap(),
            None,
        )
        .unwrap(),
    );
    let l2_service = InMemoryRpcServer::new(service.clone(), service.clone());

    let service_request_handler = Arc::new(ServiceRequestHandler::new());
    l2_service
        .register_endpoint(None, HELLO_SERVICE_RESOURCE_ID, service_request_handler)
        .await
        .expect("Unable to register endpoint");

    thread::park();
    Ok(())
}
