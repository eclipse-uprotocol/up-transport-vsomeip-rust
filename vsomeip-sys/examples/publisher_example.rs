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

use cxx::let_cxx_string;
use std::time::Duration;
use tracing::debug;
use vsomeip_sys::glue::{make_application_wrapper, make_payload_wrapper, make_runtime_wrapper};
use vsomeip_sys::vsomeip::runtime;

use common::{
    SAMPLE_EVENTGROUP_ID, SAMPLE_EVENT_ID, SAMPLE_INSTANCE_ID, SAMPLE_INTERFACE_VERSION,
    SAMPLE_SERVICE_ID,
};

const APP_NAME: &str = "Publisher";

mod common;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt::try_init();
    let client_registered = common::start_app(APP_NAME);

    // wait for this client to be registered with the runtime before starting to send messages
    client_registered.notified().await;

    let my_runtime = runtime::get();
    let runtime_wrapper = make_runtime_wrapper(my_runtime);

    let_cxx_string!(app_name = APP_NAME);
    let Some(app_wrapper) =
        make_application_wrapper(runtime_wrapper.get_pinned().get_application(&app_name))
    else {
        return Err(Box::from(format!(
            "No vsomeip application found for name: {APP_NAME}"
        )));
    };

    app_wrapper.get_pinned().offer_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_INTERFACE_VERSION,
        0,
    );

    app_wrapper.offer_single_event_safe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENT_ID,
        SAMPLE_EVENTGROUP_ID,
    );

    let payload_data: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09];

    loop {
        let payload_wrapper = make_payload_wrapper(runtime_wrapper.get_pinned().create_payload());
        payload_wrapper.set_data_safe(&payload_data);

        debug!("Sending notification [service ID: {SAMPLE_SERVICE_ID:#04X}, instance ID: {SAMPLE_INSTANCE_ID:#04X}, event ID: {SAMPLE_EVENT_ID:#04X}]: {payload_data:?}");

        app_wrapper.get_pinned().notify(
            SAMPLE_SERVICE_ID,
            SAMPLE_INSTANCE_ID,
            SAMPLE_EVENT_ID,
            payload_wrapper.get_shared_ptr(),
            true,
        );

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
