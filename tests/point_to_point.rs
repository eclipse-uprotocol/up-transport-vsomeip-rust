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
use protobuf::Enum;
use std::env::current_dir;
use std::fs::canonicalize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use up_rust::UMessageType::UMESSAGE_TYPE_UNSPECIFIED;
use up_rust::{
    UCode, UListener, UMessage, UMessageBuilder, UMessageType, UStatus, UTransport, UUri,
};
use up_transport_vsomeip::UPTransportVsomeip;

const TEST_SLACK: usize = 1;

pub struct PointToPointListener {
    client: Arc<UPTransportVsomeip>,
    received_request: AtomicUsize,
    received_response: AtomicUsize,
}

impl PointToPointListener {
    #[allow(clippy::new_without_default)]
    pub fn new(client: Arc<UPTransportVsomeip>) -> Self {
        Self {
            client,
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

        match msg
            .attributes
            .type_
            .enum_value_or(UMESSAGE_TYPE_UNSPECIFIED)
        {
            UMESSAGE_TYPE_UNSPECIFIED => {
                panic!("Not supported message type: UNSPECIFIED");
            }
            UMessageType::UMESSAGE_TYPE_PUBLISH => {
                panic!("uProtocol PUBLISH received. This shouldn't happen!");
            }
            UMessageType::UMESSAGE_TYPE_REQUEST => {
                trace!("PointToPointListener got a request");
                self.received_request.fetch_add(1, Ordering::SeqCst);
                let response_build_res =
                    UMessageBuilder::response_for_request(msg.attributes.as_ref().unwrap())
                        .with_comm_status(UCode::OK.value())
                        .build();
                let Ok(response_msg) = response_build_res else {
                    panic!(
                        "Unable to make uProtocol Response message: {:?}",
                        response_build_res.err().unwrap()
                    );
                };
                trace!("Sending Response from PointToPointListener: {response_msg:?}");
                let _ = self.client.send(response_msg).await.inspect_err(|err| {
                    panic!("Unable to send response: {err:?}");
                });
                info!("Able to send RESPONSE");
            }
            UMessageType::UMESSAGE_TYPE_RESPONSE => {
                trace!("PointToPointListener got a response");
                self.received_response.fetch_add(1, Ordering::SeqCst);
                return;
            }
            UMessageType::UMESSAGE_TYPE_NOTIFICATION => {
                panic!("Not supported message type: NOTIFICATION");
            }
        }
    }

    async fn on_error(&self, err: UStatus) {
        info!("{:?}", err);
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

    async fn on_error(&self, err: UStatus) {
        info!("{:?}", err);
    }
}

pub struct RequestListener {
    client: Arc<UPTransportVsomeip>,
    received_request: AtomicUsize,
}

impl RequestListener {
    #[allow(clippy::new_without_default)]
    pub fn new(client: Arc<UPTransportVsomeip>) -> Self {
        Self {
            client,
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

        let response_msg = UMessageBuilder::response_for_request(&msg.attributes)
            .with_comm_status(UCode::OK.value())
            .build();
        let Ok(response_msg) = response_msg else {
            panic!(
                "Unable to create response_msg: {:?}",
                response_msg.err().unwrap()
            );
        };
        let client = self.client.clone();
        let send_res = client.send(response_msg).await;
        if let Err(err) = send_res {
            panic!("Unable to send response_msg: {:?}", err);
        }
    }

    async fn on_error(&self, err: UStatus) {
        info!("{:?}", err);
    }
}
fn any_uuri() -> UUri {
    UUri {
        authority_name: "*".to_string(), // any authority
        ue_id: 0x0000_FFFF,              // any instance, any service
        ue_version_major: 0xFF,          // any
        resource_id: 0xFFFF,             // any
        ..Default::default()
    }
}

fn any_from_authority(authority_name: &str) -> UUri {
    UUri {
        authority_name: authority_name.to_string(),
        ue_id: 0x0000_FFFF,     // any instance, any service
        ue_version_major: 0xFF, // any
        resource_id: 0xFFFF,    // any
        ..Default::default()
    }
}

#[tokio::test]
async fn point_to_point() {
    env_logger::init();

    let service_authority_name = "foo";
    let streamer_ue_id = 0x9876;

    let service_1_ue_id = 0x1236;
    let service_1_ue_version_major = 1;

    let current_dir = current_dir();
    info!("{current_dir:?}");

    // let vsomeip_config_path = "vsomeip_configs/example_ustreamer.json";
    let vsomeip_config_path = "vsomeip_configs/point_to_point_integ.json";
    let abs_vsomeip_config_path = canonicalize(vsomeip_config_path).ok();
    info!("abs_vsomeip_config_path: {abs_vsomeip_config_path:?}");

    let point_to_point_client_res = UPTransportVsomeip::new_with_config(
        &service_authority_name.to_string(),
        &"me_authority".to_string(),
        streamer_ue_id,
        &abs_vsomeip_config_path.unwrap(),
    );
    let Ok(point_to_point_client) = point_to_point_client_res else {
        panic!("Unable to establish UTransport");
    };
    let point_to_point_client = Arc::new(point_to_point_client);

    let source = any_uuri();
    let sink = any_from_authority(service_authority_name);

    let point_to_point_listener_check =
        Arc::new(PointToPointListener::new(point_to_point_client.clone()));
    let point_to_point_listener: Arc<dyn UListener> = point_to_point_listener_check.clone();
    let reg_res = point_to_point_client
        .register_listener(&source, Some(&sink), point_to_point_listener)
        .await;
    if let Err(err) = reg_res {
        panic!("Unable to register with UTransport: {err}");
    }

    // craft a Request that can be served
    let service_2_ue_id = 0x1234;
    let service_2_ue_version_major = 1;
    let service_2_resource_id = 0x0421;

    let req_sink = UUri {
        authority_name: "foo".to_string(),
        ue_id: service_2_ue_id,
        ue_version_major: service_2_ue_version_major,
        resource_id: service_2_resource_id,
        ..Default::default()
    };

    let req_source = UUri {
        authority_name: "bar".to_string(),
        ue_id: service_1_ue_id,
        ue_version_major: service_1_ue_version_major,
        resource_id: 0x0, // i.e. me
        ..Default::default()
    };

    let client_authority_name = "bar";
    let streamer_ue_id = 0x7878;
    let client_ue_id = 0x0345;
    let client_ue_version_major = 1;
    let client_resource_id = 0x0000;

    let service_1_ue_id = 0x1236;
    let service_1_ue_version_major = 1;
    let service_1_resource_id_a = 0x0421;

    let client_uuri = UUri {
        authority_name: client_authority_name.to_string(),
        ue_id: client_ue_id as u32,
        ue_version_major: client_ue_version_major,
        resource_id: client_resource_id,
        ..Default::default()
    };

    let service_1_uuri_method_a = UUri {
        authority_name: service_authority_name.to_string(),
        ue_id: service_1_ue_id as u32,
        ue_version_major: service_1_ue_version_major,
        resource_id: service_1_resource_id_a,
        ..Default::default()
    };
    // let client_config = "vsomeip_configs/client.json";
    let client_config = "vsomeip_configs/point_to_point_integ.json";
    let client_config = canonicalize(client_config).ok();
    info!("client_config: {client_config:?}");

    let client_res = UPTransportVsomeip::new_with_config(
        &client_authority_name.to_string(),
        &"me_authority".to_string(),
        streamer_ue_id,
        &client_config.unwrap(),
    );

    let Ok(client) = client_res else {
        panic!("Unable to establish client");
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let response_listener_check = Arc::new(ResponseListener::new());
    let response_listener: Arc<dyn UListener> = response_listener_check.clone();

    trace!("Registering a ResponseListener");
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

    // let service_config = "vsomeip_configs/service.json";
    let service_config = "vsomeip_configs/point_to_point_integ.json";
    let service_config = canonicalize(service_config).ok();
    info!("service_config: {service_config:?}");

    let service_res = UPTransportVsomeip::new_with_config(
        &service_authority_name.to_string(),
        &"me_authority".to_string(),
        streamer_ue_id,
        &service_config.unwrap(),
    );

    let Ok(service) = service_res else {
        panic!("Unable to establish service");
    };

    tokio::time::sleep(Duration::from_millis(1000)).await;

    let service = Arc::new(service);

    let service_2_uuri = UUri {
        authority_name: service_authority_name.to_string(),
        ue_id: service_2_ue_id,
        ue_version_major: service_2_ue_version_major,
        resource_id: service_2_resource_id,
        ..Default::default()
    };

    let request_listener_check = Arc::new(RequestListener::new(service.clone()));
    let request_listener: Arc<dyn UListener> = request_listener_check.clone();

    let reg_service_1 = service
        .register_listener(&any_uuri(), Some(&service_2_uuri), request_listener.clone())
        .await;

    if let Err(err) = reg_service_1 {
        error!("Unable to register: {:?}", err);
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Track the start time and set the duration for the loop
    let duration = Duration::from_millis(1000);
    let start_time = Instant::now();

    let mut iterations = 0;
    while Instant::now().duration_since(start_time) < duration {
        let request_msg_res_1_a =
            UMessageBuilder::request(service_1_uuri_method_a.clone(), client_uuri.clone(), 10000)
                .build();

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

        let request_msg_res = UMessageBuilder::request(req_sink.clone(), req_source.clone(), 10000)
            .build()
            .unwrap();
        trace!("Sending message from PointToPoint client: {request_msg_res}");
        let send_res = point_to_point_client.send(request_msg_res.clone()).await;

        if let Err(err) = send_res {
            panic!("Unable to send message: {err:?}");
        }

        iterations += 1;
    }

    tokio::time::sleep(Duration::from_millis(1000)).await;

    trace!(
        "request_listener_check.received_request(): {}",
        request_listener_check.received_request()
    );
    trace!(
        "point_to_point_listener_check.received_request(): {}",
        point_to_point_listener_check.received_request()
    );
    trace!(
        "point_to_point_listener_check.received_response(): {}",
        point_to_point_listener_check.received_response()
    );
    trace!(
        "response_listener_check.received_response(): {}",
        response_listener_check.received_response()
    );

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

    // TODO: Troubleshoot why we miss the first request / response in the PointToPointListener
    assert!(iterations - request_listener_check.received_request() <= TEST_SLACK);
    assert!(iterations - point_to_point_listener_check.received_request() <= TEST_SLACK);
    assert!(iterations - point_to_point_listener_check.received_response() <= TEST_SLACK);
    assert!(iterations - response_listener_check.received_response() <= TEST_SLACK);
}
