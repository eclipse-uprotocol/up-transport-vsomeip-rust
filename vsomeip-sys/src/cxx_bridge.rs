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

#[cxx::bridge(namespace = "glue")]
pub mod handler_registration {
    unsafe extern "C++" {
        include!("vsomeip/vsomeip.hpp");
        include!("include/application_wrapper.h");
        include!("application_registrations.h");

        type message_handler_fn_ptr = crate::extern_callback_wrappers::MessageHandlerFnPtr;
        type ApplicationWrapper = crate::ffi::glue::ApplicationWrapper;

        pub unsafe fn register_message_handler_fn_ptr(
            _application: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _method: u16,
            _fn_ptr_handler: message_handler_fn_ptr,
        );

        type availability_handler_fn_ptr =
            crate::extern_callback_wrappers::AvailabilityHandlerFnPtr;

        pub unsafe fn register_availability_handler_fn_ptr(
            _application: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _fn_ptr_handler: availability_handler_fn_ptr,
            _major: u8,
            _minor: u32,
        );
    }
}

// autocxx fails to generate bindings to these functions, so we write the bindings for them
// by hand and inject them into the vsomeip_v3 namespace
#[cxx::bridge(namespace = "vsomeip_v3")]
mod autocxx_failed {
    unsafe extern "C++" {
        include!("vsomeip/vsomeip.hpp");

        type payload = crate::vsomeip::payload;

        /// # Safety
        ///
        /// We are simply creating a binding here for one that autocxx failed to generate
        pub unsafe fn set_data(self: Pin<&mut payload>, _data: *const u8, _length: u32);

        pub fn get_data(self: &payload) -> *const u8;

        pub fn get_length(self: &payload) -> u32;
    }
}
