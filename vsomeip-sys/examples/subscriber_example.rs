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
    make_application_wrapper, make_message_wrapper, make_runtime_wrapper, AvailabilityHandlerFnPtr,
    MessageHandlerFnPtr, SubscriptionStatusHandlerFnPtr,
};
use vsomeip_sys::vsomeip::{self, ANY_MAJOR};
use vsomeip_sys::vsomeip::{message, runtime, ANY_MINOR};

use common::{SAMPLE_EVENTGROUP_ID, SAMPLE_EVENT_ID, SAMPLE_INSTANCE_ID, SAMPLE_SERVICE_ID};

use crate::common::SAMPLE_INTERFACE_VERSION;

const APP_NAME: &str = "Subscriber";

mod common;

extern "C" fn on_availability_change(
    service_id: vsomeip::service_t,
    instance: vsomeip::instance_t,
    available: bool,
) {
    info!("Service [ID: {service_id:#04X}, instance: {instance:#04X}] is available: {available}");
}

extern "C" fn on_message(msg: &SharedPtr<message>) {
    let cloned_msg = msg.clone();
    let msg_wrapper = make_message_wrapper(cloned_msg);

    let msg_type = msg_wrapper.get_message_base_pinned().get_message_type();
    debug!("received event with message_type_e: {msg_type}");

    if let Some(payload_wrapper) = msg_wrapper.get_message_payload() {
        let payload = payload_wrapper.get_data_safe();
        info!("received event with payload: {payload:?}");
    } else {
        error!("Unable to get PayloadWrapper from MessageWrapper");
    };
}

extern "C" fn on_subscription_status(
    service: vsomeip::service_t,
    instance: vsomeip::instance_t,
    eventgroup: vsomeip::eventgroup_t,
    event: vsomeip::event_t,
    status: u16,
) {
    info!("Subscription status of service [ID: {service:#04X}, instance: {instance:#04X}, eventgroup: {eventgroup:#04X}, event: {event:#04X}] has changed: {status:#04X}");
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

    let client_id = app_wrapper.get_pinned().get_client();
    info!("Started SOME/IP application with client ID: {client_id:#04X}");

    app_wrapper.register_availability_handler_fn_ptr_safe(
        vsomeip::ANY_SERVICE,
        vsomeip::ANY_INSTANCE,
        AvailabilityHandlerFnPtr(on_availability_change),
        ANY_MAJOR,
        ANY_MINOR,
    );

    app_wrapper.register_subscription_status_handler_fn_ptr_safe(
        vsomeip::ANY_SERVICE,
        vsomeip::ANY_INSTANCE,
        vsomeip::ANY_EVENTGROUP,
        vsomeip::ANY_EVENT,
        SubscriptionStatusHandlerFnPtr(on_subscription_status),
        true,
    );

    app_wrapper.register_message_handler_fn_ptr_safe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENT_ID,
        MessageHandlerFnPtr(on_message),
    );

    app_wrapper.get_pinned().request_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_INTERFACE_VERSION,
        ANY_MINOR,
    );

    app_wrapper.request_single_event_safe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENT_ID,
        SAMPLE_EVENTGROUP_ID,
    );

    app_wrapper.get_pinned().subscribe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENTGROUP_ID,
        SAMPLE_INTERFACE_VERSION,
        SAMPLE_EVENT_ID,
    );

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c signal");
    Ok(())
}
