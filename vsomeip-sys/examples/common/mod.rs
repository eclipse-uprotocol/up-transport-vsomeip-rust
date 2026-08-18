/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
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

use std::sync::{Arc, OnceLock};

use cxx::let_cxx_string;
use tokio::sync::Notify;
use tracing::info;
use vsomeip_sys::{
    glue::{make_application_wrapper, make_runtime_wrapper, AvailableStateHandlerFnPtr},
    vsomeip::{self, runtime},
};

pub(crate) const SAMPLE_SERVICE_ID: u16 = 0x1234;
pub(crate) const SAMPLE_INSTANCE_ID: u16 = 0x0001;
pub(crate) const SAMPLE_INTERFACE_VERSION: u8 = 0x01;

#[allow(dead_code)]
pub(crate) const SAMPLE_EVENTGROUP_ID: u16 = 0x4465;
#[allow(dead_code)]
pub(crate) const SAMPLE_EVENT_ID: u16 = 0x4465;

#[allow(dead_code)]
pub(crate) const SAMPLE_METHOD_ID: u16 = 0x0421;

static CLIENT_REGISTERED: OnceLock<Arc<Notify>> = OnceLock::new();

/// This handler is invoked by the vsomeip runtime when the state of the application changes.
///
/// In this example, we are only interested in the ST_REGISTERED state, which indicates that
/// the application has successfully registered with the vsomeip runtime and is ready to send
/// and receive messages.
extern "C" fn state_handler(state: vsomeip::state_type_e) {
    match state {
        vsomeip::state_type_e::ST_REGISTERED => {
            info!("client is now registered");
            // Notify the main thread that the client is registered and ready to send messages
            // Sadly, there is no way to pass a context pointer to the state handler,
            // so we have to use a static variable
            if let Some(n) = CLIENT_REGISTERED.get() {
                n.notify_one();
            }
        }
        vsomeip::state_type_e::ST_DEREGISTERED => {
            info!("client is now deregistered");
        }
    }
}

pub(crate) fn start_app(name: &str) -> Arc<Notify> {
    let result = Arc::new(Notify::new());
    let _ = CLIENT_REGISTERED.set(result.clone());
    let app_name = name.to_string();
    std::thread::spawn(move || {
        let runtime = runtime::get();
        let runtime_wrapper = make_runtime_wrapper(runtime);

        let_cxx_string!(application_name = app_name);
        let app_wrapper = make_application_wrapper(
            runtime_wrapper
                .get_pinned()
                .create_application(&application_name),
        )
        .unwrap_or_else(|| {
            panic!("failed to create application [{application_name}]");
        });
        app_wrapper.get_pinned().init();
        app_wrapper.register_state_handler_fn_ptr_safe(AvailableStateHandlerFnPtr(state_handler));
        app_wrapper.get_pinned().start();
    });
    result
}
