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

mod cxx_bridge;

use autocxx::prelude::*; // use all the main autocxx functions

// using autocxx to generate Rust bindings for all these
include_cpp! {
    #include "vsomeip/vsomeip.hpp"
    #include "runtime_wrapper.h"
    #include "application_wrapper.h"
    #include "message_wrapper.h"
    #include "payload_wrapper.h"
    safety!(unsafe) // see details of unsafety policies described in the 'safety' section of the book
    generate!("vsomeip_v3::runtime")
    generate!("vsomeip_v3::application")
    generate!("vsomeip_v3::message_base")
    generate!("vsomeip_v3::message_t")
    generate!("vsomeip_v3::ANY_MAJOR")
    generate!("vsomeip_v3::ANY_MINOR")
    generate!("vsomeip_v3::ANY_INSTANCE")
    generate!("vsomeip_v3::ANY_SERVICE")
    generate!("vsomeip_v3::ANY_METHOD")
    generate!("vsomeip_v3::ANY_EVENT")
    generate!("vsomeip_v3::ANY_EVENTGROUP")
    generate!("glue::RuntimeWrapper")
    generate!("glue::make_runtime_wrapper")
    generate!("glue::ApplicationWrapper")
    generate!("glue::make_application_wrapper")
    generate!("glue::MessageWrapper")
    generate!("glue::make_message_wrapper")
    generate!("glue::upcast")
    generate!("glue::PayloadWrapper")
    generate!("glue::make_payload_wrapper")
    generate!("glue::set_payload_raw")
    generate!("glue::get_payload_raw")
    generate!("glue::create_payload_wrapper")
}

pub mod vsomeip {
    pub use crate::ffi::vsomeip_v3::*;
}

pub mod extern_callback_wrappers;
pub mod safe_glue;

mod unsafe_fns {
    pub use crate::ffi::glue::create_payload_wrapper;
}

pub mod glue {
    pub use crate::ffi::glue::upcast;
    pub use crate::ffi::glue::{
        make_application_wrapper, make_message_wrapper, make_payload_wrapper, make_runtime_wrapper,
        ApplicationWrapper, MessageWrapper, PayloadWrapper, RuntimeWrapper,
    };
}

#[cfg(test)]
mod tests {
    use crate::extern_callback_wrappers::AvailabilityHandlerFnPtr;
    use crate::ffi::vsomeip_v3::runtime;
    use crate::glue::{
        make_application_wrapper, make_message_wrapper, make_payload_wrapper, make_runtime_wrapper,
    };
    use crate::safe_glue::{
        get_data_safe, get_message_payload, get_pinned_application, get_pinned_message_base,
        get_pinned_payload, get_pinned_runtime, register_availability_handler_fn_ptr_safe,
        set_data_safe, set_message_payload,
    };
    use cxx::let_cxx_string;
    use std::time::Duration;

    #[test]
    fn test_make_runtime() {
        let my_runtime = runtime::get();
        let runtime_wrapper = make_runtime_wrapper(my_runtime);

        let_cxx_string!(my_app_str = "my_app");
        let mut app_wrapper = make_application_wrapper(
            get_pinned_runtime(&runtime_wrapper).create_application(&my_app_str),
        );
        get_pinned_application(&app_wrapper).init();

        extern "C" fn callback(
            _service: crate::vsomeip::service_t,
            _instance: crate::vsomeip::instance_t,
            _availability: bool,
        ) {
            println!("hello from Rust!");
        }
        let callback = AvailabilityHandlerFnPtr(callback);
        register_availability_handler_fn_ptr_safe(&mut app_wrapper, 1, 2, callback, 3, 4);
        let request =
            make_message_wrapper(get_pinned_runtime(&runtime_wrapper).create_request(true));

        let reliable = get_pinned_message_base(&request).is_reliable();

        println!("reliable? {reliable}");

        let mut request =
            make_message_wrapper(get_pinned_runtime(&runtime_wrapper).create_request(true));
        get_pinned_message_base(&request).set_service(1);
        get_pinned_message_base(&request).set_instance(2);
        get_pinned_message_base(&request).set_method(3);

        let mut payload_wrapper =
            make_payload_wrapper(get_pinned_runtime(&runtime_wrapper).create_payload());
        let _foo = get_pinned_payload(&payload_wrapper);

        let data: Vec<u8> = vec![1, 2, 3, 4, 5];

        set_data_safe(get_pinned_payload(&payload_wrapper), &data);

        let data_vec = get_data_safe(&payload_wrapper);
        println!("{:?}", data_vec);

        set_message_payload(&mut request, &mut payload_wrapper);

        println!("set_message_payload");

        let loaded_payload = get_message_payload(&mut request);

        println!("get_message_payload");

        let loaded_data_vec = get_data_safe(&loaded_payload);

        println!("loaded_data_vec: {loaded_data_vec:?}");

        std::thread::sleep(Duration::from_millis(2000));
    }
}
