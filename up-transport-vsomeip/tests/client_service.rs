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

use log::{error, info};
use std::fs::canonicalize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::time::Instant;
use up_rust::{UCode, UListener, UMessage, UMessageBuilder, UPayloadFormat, UTransport, UUri};
use up_transport_vsomeip::UPTransportVsomeip;

const TEST_DURATION: u64 = 1000;
const MAX_ITERATIONS: usize = 100;

pub struct ResponseListener {
    received_response: AtomicUsize,
}
impl ResponseListener {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            received_response: AtomicUsize::new(0),
        }
    }

    pub fn received_response(&self) -> usize {
        self.received_response.load(Ordering::SeqCst)
    }
}
#[async_trait::async_trait]
impl UListener for ResponseListener {
    async fn on_receive(&self, msg: UMessage) {
        info!("Received Response:\n{:?}", msg);

        let payload = {
            match msg.payload {
                None => {
                    panic!("Unable to retrieve bytes")
                }
                Some(payload) => payload,
            }
        };

        let payload_bytes = payload.to_vec();
        info!("Received response payload_bytes of: {payload_bytes:?}");
        let Ok(response_payload_string) = std::str::from_utf8(&payload_bytes) else {
            panic!("unable to convert payload_bytes to string");
        };
        info!("Response payload_string: {response_payload_string}");

        self.received_response.fetch_add(1, Ordering::SeqCst);
    }
}
pub struct RequestListener {
    client: Weak<UPTransportVsomeip>,
    received_request: AtomicUsize,
}

impl RequestListener {
    #[allow(clippy::new_without_default)]
    pub fn new(client: Arc<UPTransportVsomeip>) -> Self {
        Self {
            client: Arc::downgrade(&client),
            received_request: AtomicUsize::new(0),
        }
    }

    pub fn received_request(&self) -> usize {
        self.received_request.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl UListener for RequestListener {
    async fn on_receive(&self, msg: UMessage) {
        self.received_request.fetch_add(1, Ordering::SeqCst);
        info!("Received Request:\n{:?}", msg);

        let payload = {
            match msg.payload {
                None => {
                    panic!("Unable to retrieve bytes")
                }
                Some(payload) => payload,
            }
        };

        let payload_bytes = payload.to_vec();
        info!("Received request payload_bytes of: {payload_bytes:?}");
        let Ok(payload_string) = std::str::from_utf8(&payload_bytes) else {
            panic!("Unable to unpack string from payload_bytes");
        };
        info!("Request payload_string: {payload_string}");

        let response_payload_string = format!("Here's a response to: {payload_string}");
        let response_payload_bytes = response_payload_string.into_bytes();

        let response_msg = UMessageBuilder::response_for_request(&msg.attributes)
            .with_comm_status(UCode::OK)
            .build_with_payload(response_payload_bytes, UPayloadFormat::UPAYLOAD_FORMAT_TEXT);
        let Ok(response_msg) = response_msg else {
            panic!(
                "Unable to create response_msg: {:?}",
                response_msg.err().unwrap()
            );
        };
        if let Some(client) = self.client.upgrade() {
            let send_res = client.send(response_msg).await;
            if let Err(err) = send_res {
                panic!("Unable to send response_msg: {:?}", err);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn client_service() {
    env_logger::init();
    // console_subscriber::init();

    let service_authority_name = "foo";
    let streamer_ue_id = 0x7878;

    let service_1_ue_id = 0x1234;
    let service_1_ue_version_major = 1;
    let service_1_resource_id_a = 0x0421;

    let client_authority_name = "bar";
    let client_ue_id = 0x0345;
    let client_ue_version_major = 1;
    let client_resource_id = 0x0000;

    let client_config = "vsomeip_configs/client.json";
    let client_config = canonicalize(client_config).ok();
    println!("client_config: {client_config:?}");

    let client_uuri = UUri::try_from_parts(client_authority_name, streamer_ue_id, 1, 0).unwrap();
    let client_res = UPTransportVsomeip::new_with_config(
        client_uuri,
        &service_authority_name.to_string(),
        &client_config.unwrap(),
        None,
    );

    let Ok(client) = client_res else {
        panic!(
            "Unable to establish client: {:?}",
            client_res.err().unwrap()
        );
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let client_uuri = UUri::try_from_parts(
        client_authority_name,
        client_ue_id as u32,
        client_ue_version_major,
        client_resource_id,
    )
    .unwrap();

    let service_1_uuri_method_a = UUri::try_from_parts(
        service_authority_name,
        service_1_ue_id as u32,
        service_1_ue_version_major,
        service_1_resource_id_a,
    )
    .unwrap();

    let response_listener_check = Arc::new(ResponseListener::new());
    let response_listener: Arc<dyn UListener> = response_listener_check.clone();

    let reg_res_1 = client
        .register_listener(
            &service_1_uuri_method_a,
            Some(&client_uuri),
            response_listener.clone(),
        )
        .await;
    if let Err(err) = reg_res_1 {
        panic!("Unable to register for returning Response: {:?}", err);
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let service_config = "vsomeip_configs/service.json";
    let service_config = canonicalize(service_config).ok();
    println!("service_config: {service_config:?}");

    let service_uuri = UUri::try_from_parts(service_authority_name, streamer_ue_id, 1, 0).unwrap();
    let service_res = UPTransportVsomeip::new_with_config(
        service_uuri,
        &client_authority_name.to_string(),
        &service_config.unwrap(),
        None,
    );

    let Ok(service) = service_res else {
        panic!("Unable to establish subscriber");
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let service = Arc::new(service);

    let service_1_uuri = UUri::try_from_parts(
        service_authority_name,
        service_1_ue_id as u32,
        service_1_ue_version_major,
        service_1_resource_id_a,
    )
    .unwrap();

    let request_listener_check = Arc::new(RequestListener::new(service.clone()));
    let request_listener: Arc<dyn UListener> = request_listener_check.clone();

    let reg_service_1 = service
        .register_listener(
            &UUri::any(),
            Some(&service_1_uuri),
            request_listener.clone(),
        )
        .await;

    if let Err(err) = reg_service_1 {
        error!("Unable to register: {:?}", err);
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Track the start time and set the duration for the loop
    let duration = Duration::from_millis(TEST_DURATION);
    let start_time = Instant::now();

    let mut iterations = 0;
    // let iterations_to_run = 1;
    let mut i = 20;
    // while iterations < iterations_to_run {
    while (Instant::now().duration_since(start_time) < duration) && (iterations < MAX_ITERATIONS) {
        let payload_string = format!("request@i={i}");
        let payload = payload_string.into_bytes();
        let request_msg_res_1_a =
            UMessageBuilder::request(service_1_uuri_method_a.clone(), client_uuri.clone(), 10000)
                .build_with_payload(payload, UPayloadFormat::UPAYLOAD_FORMAT_TEXT);

        let Ok(request_msg_1_a) = request_msg_res_1_a else {
            panic!(
                "Unable to create Request UMessage: {:?}",
                request_msg_res_1_a.err().unwrap()
            );
        };

        let send_res_1_a = client.send(request_msg_1_a).await;

        if let Err(err) = send_res_1_a {
            panic!("Unable to send Request UMessage: {:?}", err);
        }

        iterations += 1;
        i += 1;
    }

    tokio::time::sleep(Duration::from_millis(2000)).await;

    println!("iterations: {}", iterations);
    println!(
        "request_listener_check.received_request(): {}",
        request_listener_check.received_request()
    );
    println!(
        "response_listener_check.received_response(): {}",
        response_listener_check.received_response()
    );

    assert_eq!(iterations, request_listener_check.received_request());
    assert_eq!(iterations, response_listener_check.received_response());
}
