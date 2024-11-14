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

use log::{error, info, trace};
use protobuf::EnumOrUnknown;
use std::env::current_dir;
use std::fs::canonicalize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::time::Instant;
use up_rust::UMessageType::UMESSAGE_TYPE_UNSPECIFIED;
use up_rust::UPayloadFormat::UPAYLOAD_FORMAT_PROTOBUF;
use up_rust::{UCode, UListener, UMessage, UMessageBuilder, UMessageType, UTransport, UUri, UUID};
use up_transport_vsomeip::UPTransportVsomeip;

const TEST_DURATION: u64 = 500;
const MAX_ITERATIONS: usize = 100;
const STREAMER_UE_ID: u32 = 0x9876;

const CLIENT_AUTHORITY_NAME: &str = "foo";
const CLIENT_UE_ID: u32 = 0x1234;
const CLIENT_UE_VERSION_NUMBER: u32 = 1;

const PTP_AUTHORITY_NAME: &str = "foo";
const PTP_UE_ID: u32 = 0x2345;
const PTP_UE_VERSION_NUMBER: u32 = 1;
const PTP_METHOD_RESOURCE_ID: u32 = 0x0421;

const SERVICE_AUTHORITY_NAME: &str = "foo";
const SERVICE_UE_ID: u32 = 0x3456;
const SERVICE_UE_VERSION_NUMBER: u32 = 1;
const SERVICE_METHOD_RESOURCE_ID: u32 = 0x0421;

const NON_POINT_TO_POINT_LISTENED_AUTHORITY: &str = "oops";

fn client_reply_uuri() -> UUri {
    UUri {
        authority_name: CLIENT_AUTHORITY_NAME.to_string(),
        ue_id: CLIENT_UE_ID,
        ue_version_major: CLIENT_UE_VERSION_NUMBER,
        resource_id: 0x0000,
        ..Default::default()
    }
}

fn ptp_reply_uuri() -> UUri {
    UUri {
        authority_name: PTP_AUTHORITY_NAME.to_string(),
        ue_id: PTP_UE_ID,
        ue_version_major: PTP_UE_VERSION_NUMBER,
        resource_id: 0x0000,
        ..Default::default()
    }
}

fn ptp_method_uuri() -> UUri {
    UUri {
        authority_name: PTP_AUTHORITY_NAME.to_string(),
        ue_id: PTP_UE_ID,
        ue_version_major: PTP_UE_VERSION_NUMBER,
        resource_id: PTP_METHOD_RESOURCE_ID,
        ..Default::default()
    }
}

fn service_uuri() -> UUri {
    UUri {
        authority_name: SERVICE_AUTHORITY_NAME.to_string(),
        ue_id: SERVICE_UE_ID,
        ue_version_major: SERVICE_UE_VERSION_NUMBER,
        resource_id: SERVICE_METHOD_RESOURCE_ID,
        ..Default::default()
    }
}

pub struct PointToPointListener {
    client: Weak<UPTransportVsomeip>,
    received_request: AtomicUsize,
    received_response: AtomicUsize,
}

impl PointToPointListener {
    #[allow(clippy::new_without_default)]
    pub fn new(client: Arc<UPTransportVsomeip>) -> Self {
        Self {
            client: Arc::downgrade(&client),
            received_request: AtomicUsize::new(0),
            received_response: AtomicUsize::new(0),
        }
    }
    pub fn received_request(&self) -> usize {
        self.received_request.load(Ordering::SeqCst)
    }
    pub fn received_response(&self) -> usize {
        self.received_response.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl UListener for PointToPointListener {
    async fn on_receive(&self, msg: UMessage) {
        info!("Received in point-to-point listener:\n{:?}", msg);

        let received_source_authority = msg.attributes.source.clone().unwrap().authority_name;
        if received_source_authority == NON_POINT_TO_POINT_LISTENED_AUTHORITY {
            panic!(
                "Received a message on point to point listener that we should not have:\n{msg:?}"
            );
        }

        match msg
            .attributes
            .type_
            .enum_value_or(UMESSAGE_TYPE_UNSPECIFIED)
        {
            UMESSAGE_TYPE_UNSPECIFIED => {
                panic!("Not supported message type: UNSPECIFIED:\n{:?}", msg);
            }
            UMessageType::UMESSAGE_TYPE_PUBLISH => {
                panic!("uProtocol PUBLISH received. This shouldn't happen!");
            }
            UMessageType::UMESSAGE_TYPE_REQUEST => {
                trace!("PointToPointListener got a request");
                self.received_request.fetch_add(1, Ordering::SeqCst);

                let original_id = msg
                    .attributes
                    .as_ref()
                    .unwrap()
                    .id
                    .as_ref()
                    .unwrap()
                    .clone();

                info!(
                    "within point to point listener, original_id: {}",
                    original_id.to_hyphenated_string()
                );

                let mut builder = UMessageBuilder::request(service_uuri(), ptp_reply_uuri(), 1000);
                let Ok(forwarding_request) = builder.build_with_protobuf_payload(&original_id)
                else {
                    panic!("Unable to make uProtocol Request message to forward to service");
                };

                info!("constructed forwarding_request: {forwarding_request:?}");

                let Some(client) = self.client.upgrade() else {
                    panic!("Unable to get ahold of the transport within PointToPointListener");
                };

                let _ = client.send(forwarding_request).await.inspect_err(|err| {
                    error!("err: Unable to send response: {err:?}");
                    panic!("Unable to send response: {err:?}");
                });

                info!("Able to forward request");
            }
            UMessageType::UMESSAGE_TYPE_RESPONSE => {
                trace!("PointToPointListener got a response: {:?}", msg);
                self.received_response.fetch_add(1, Ordering::SeqCst);

                let mut msg_with_correct_payload_format = msg.clone();
                if let Some(attributes) = msg_with_correct_payload_format.attributes.as_mut() {
                    attributes.payload_format = EnumOrUnknown::from(UPAYLOAD_FORMAT_PROTOBUF);
                }

                trace!(
                    "corrected response with protobuf payload format: {:?}",
                    msg_with_correct_payload_format
                );

                let original_id: Result<UUID, _> =
                    msg_with_correct_payload_format.extract_protobuf();

                let original_id = {
                    match original_id {
                        Err(err) => {
                            panic!("{err}");
                        }
                        Ok(original_id) => original_id,
                    }
                };

                trace!("point to point response, original_id: {original_id}");

                let builder =
                    UMessageBuilder::response(client_reply_uuri(), original_id, ptp_method_uuri())
                        .build();
                let response_msg = {
                    match builder {
                        Ok(msg) => msg,
                        Err(err) => {
                            panic!("{err}");
                        }
                    }
                };

                trace!("response_msg: {response_msg:?}");

                let Some(client) = self.client.upgrade() else {
                    panic!("Unable to get ahold of the transport within PointToPointListener");
                };

                let _ = client.send(response_msg).await.inspect_err(|err| {
                    panic!("Unable to send response: {err:?}");
                });

                info!("Able to forward response");

                return;
            }
            UMessageType::UMESSAGE_TYPE_NOTIFICATION => {
                panic!("Not supported message type: NOTIFICATION");
            }
        }
    }
}

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
        info!("ResponseListener: Received Response:\n{:?}", msg);
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

        let mut msg_with_correct_payload_format = msg.clone();
        if let Some(attributes) = msg_with_correct_payload_format.attributes.as_mut() {
            attributes.payload_format = EnumOrUnknown::from(UPAYLOAD_FORMAT_PROTOBUF);
        }

        info!("Corrected Request:\n{:?}", msg_with_correct_payload_format);

        let original_id: Result<UUID, _> = msg_with_correct_payload_format.extract_protobuf();

        let original_id = {
            match original_id {
                Err(err) => {
                    panic!("{err}");
                }
                Ok(original_id) => original_id,
            }
        };

        info!("original_id: {}", original_id.to_hyphenated_string());

        let response_msg =
            UMessageBuilder::response_for_request(&msg_with_correct_payload_format.attributes)
                .with_comm_status(UCode::OK)
                .build_with_protobuf_payload(&original_id);

        info!("response_msg: {response_msg:?}");

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
fn any_from_authority(authority_name: &str) -> UUri {
    let mut any_with_authority = UUri::any();
    any_with_authority.authority_name = authority_name.to_string();
    any_with_authority
}

#[tokio::test(flavor = "multi_thread")]
async fn point_to_point() {
    env_logger::init();

    let current_dir = current_dir();
    info!("{current_dir:?}");

    let vsomeip_config_path = "vsomeip_configs/point_to_point_integ.json";
    let abs_vsomeip_config_path = canonicalize(vsomeip_config_path).ok();
    info!("abs_vsomeip_config_path: {abs_vsomeip_config_path:?}");

    let point_to_point_uri =
        UUri::try_from_parts(PTP_AUTHORITY_NAME, STREAMER_UE_ID, 1, 0).unwrap();
    trace!("Initializing point to point: Start");
    let point_to_point_client_res = UPTransportVsomeip::new_with_config(
        point_to_point_uri,
        &PTP_AUTHORITY_NAME.to_string(),
        &abs_vsomeip_config_path.unwrap(),
        None,
    );
    trace!("Initializing point to point: End");
    let Ok(point_to_point_client) = point_to_point_client_res else {
        if let Err(e) = point_to_point_client_res {
            panic!("Unable to establish UTransport: {e:?}");
        }
        panic!("Unable to establish UTransport");
    };
    let point_to_point_client = Arc::new(point_to_point_client);

    let source = UUri::any();
    let sink = any_from_authority(PTP_AUTHORITY_NAME);

    let point_to_point_listener_check =
        Arc::new(PointToPointListener::new(point_to_point_client.clone()));
    let point_to_point_listener: Arc<dyn UListener> = point_to_point_listener_check.clone();
    trace!("Registering point to point listener: Start");
    let reg_res = point_to_point_client
        .register_listener(&source, Some(&sink), point_to_point_listener)
        .await;
    trace!("Registering point to point listener: End");
    if let Err(err) = reg_res {
        panic!("Unable to register with UTransport: {err}");
    }

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let client_config = "vsomeip_configs/client.json";
    let client_config = canonicalize(client_config).ok();
    info!("client_config: {client_config:?}");

    let client_uri = UUri::try_from_parts(CLIENT_AUTHORITY_NAME, CLIENT_UE_ID, 1, 0).unwrap();
    let client_res = UPTransportVsomeip::new_with_config(
        client_uri,
        &CLIENT_AUTHORITY_NAME.to_string(),
        &client_config.unwrap(),
        None,
    );

    let Ok(client) = client_res else {
        if let Err(e) = client_res {
            panic!("Unable to establish client: {:?}", e);
        }
        panic!();
    };

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let response_listener_check = Arc::new(ResponseListener::new());
    let response_listener: Arc<dyn UListener> = response_listener_check.clone();

    trace!("Registering a ResponseListener");
    let reg_res_1 = client
        .register_listener(
            &ptp_method_uuri(),
            Some(&client_reply_uuri()),
            response_listener.clone(),
        )
        .await;
    if let Err(err) = reg_res_1 {
        panic!("Unable to register for returning Response: {:?}", err);
    }

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let service_config = "vsomeip_configs/service.json";
    let service_config = canonicalize(service_config).ok();
    info!("service_config: {service_config:?}");

    let service_uri = UUri::try_from_parts(SERVICE_AUTHORITY_NAME, SERVICE_UE_ID, 1, 0).unwrap();
    let service_res = UPTransportVsomeip::new_with_config(
        service_uri,
        &SERVICE_AUTHORITY_NAME.to_string(),
        &service_config.unwrap(),
        None,
    );

    let Ok(service) = service_res else {
        if let Err(e) = service_res {
            panic!("Unable to establish service: {:?}", e);
        }
        panic!();
    };

    tokio::time::sleep(Duration::from_millis(2000)).await;

    let service = Arc::new(service);

    let request_listener_check = Arc::new(RequestListener::new(service.clone()));
    let request_listener: Arc<dyn UListener> = request_listener_check.clone();

    let reg_service_1 = service
        .register_listener(
            &UUri::any(),
            Some(&service_uuri()),
            request_listener.clone(),
        )
        .await;

    if let Err(err) = reg_service_1 {
        error!("Unable to register: {:?}", err);
    }

    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Track the start time and set the duration for the loop
    let duration = Duration::from_millis(TEST_DURATION);
    let start_time = Instant::now();

    let mut iterations = 0;
    while (Instant::now().duration_since(start_time) < duration) && (iterations < MAX_ITERATIONS) {
        let request_msg_res =
            UMessageBuilder::request(ptp_method_uuri(), client_reply_uuri(), 10000)
                .build()
                .unwrap();
        trace!("Sending message from client: {request_msg_res}");
        let send_res = client.send(request_msg_res.clone()).await;

        if let Err(err) = send_res {
            panic!("Unable to send message: {err:?}");
        }

        iterations += 1;
    }

    tokio::time::sleep(Duration::from_millis(2000)).await;

    println!("iterations: {}", iterations);

    println!(
        "request_listener_check.received_request(): {}",
        request_listener_check.received_request()
    );
    println!(
        "point_to_point_listener_check.received_request(): {}",
        point_to_point_listener_check.received_request()
    );
    println!(
        "point_to_point_listener_check.received_response(): {}",
        point_to_point_listener_check.received_response()
    );
    println!(
        "response_listener_check.received_response(): {}",
        response_listener_check.received_response()
    );

    assert_eq!(iterations, request_listener_check.received_request());
    assert_eq!(iterations, point_to_point_listener_check.received_request());
    assert_eq!(
        iterations,
        point_to_point_listener_check.received_response()
    );
    assert_eq!(iterations, response_listener_check.received_response());
}
