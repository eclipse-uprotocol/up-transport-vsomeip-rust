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
use tracing::info;
use vsomeip_sys::glue::{
    make_application_wrapper, make_message_wrapper, make_payload_wrapper, make_runtime_wrapper,
};
use vsomeip_sys::vsomeip::runtime;

use common::{SAMPLE_INSTANCE_ID, SAMPLE_INTERFACE_VERSION, SAMPLE_METHOD_ID, SAMPLE_SERVICE_ID};

const APP_NAME: &str = "Hello World Client";

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

    app_wrapper.get_pinned().request_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_INTERFACE_VERSION,
        vsomeip_sys::vsomeip::ANY_MINOR,
    );

    loop {
        let request = make_message_wrapper(runtime_wrapper.get_pinned().create_request(true));
        request
            .get_message_base_pinned()
            .set_service(SAMPLE_SERVICE_ID);
        request
            .get_message_base_pinned()
            .set_instance(SAMPLE_INSTANCE_ID);
        request
            .get_message_base_pinned()
            .set_interface_version(SAMPLE_INTERFACE_VERSION);
        request
            .get_message_base_pinned()
            .set_method(SAMPLE_METHOD_ID);

        let mut payload_wrapper =
            make_payload_wrapper(runtime_wrapper.get_pinned().create_payload());

        payload_wrapper.set_data_safe(b"Hello, vsomeip!");
        request.set_message_payload(&mut payload_wrapper);

        let shared_ptr_message = request.as_ref().unwrap().get_shared_ptr();
        info!("sending request");
        app_wrapper.get_pinned().send(shared_ptr_message);

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
