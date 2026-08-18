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

use cxx::{let_cxx_string, SharedPtr};
use tracing::{debug, error, info};
use vsomeip_sys::glue::{
    make_application_wrapper, make_message_wrapper, make_runtime_wrapper, MessageHandlerFnPtr,
};
use vsomeip_sys::vsomeip::{message, runtime};

use common::{SAMPLE_INSTANCE_ID, SAMPLE_INTERFACE_VERSION, SAMPLE_METHOD_ID, SAMPLE_SERVICE_ID};

const APP_NAME: &str = "Hello World Service";

mod common;

extern "C" fn on_request(msg: &SharedPtr<message>) {
    let cloned_msg = msg.clone();
    let msg_wrapper = make_message_wrapper(cloned_msg);

    let msg_type = msg_wrapper.get_message_base_pinned().get_message_type();
    debug!("received message of type: {msg_type}");

    let Some(payload_wrapper) = msg_wrapper.get_message_payload() else {
        error!("Unable to get PayloadWrapper from MessageWrapper");
        return;
    };
    let payload = payload_wrapper.get_data_safe();

    info!("processing request with payload: {payload:?}");

    let payload_string = std::str::from_utf8(&payload);
    match payload_string {
        Ok(str) => {
            debug!("payload_string: {str}");
        }
        Err(err) => {
            error!("unable to convert bytes to string: {err:?}");
        }
    }
}

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

    app_wrapper.register_message_handler_fn_ptr_safe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_METHOD_ID,
        MessageHandlerFnPtr(on_request),
    );

    app_wrapper.get_pinned().offer_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_INTERFACE_VERSION,
        0,
    );

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c signal");
    Ok(())
}
