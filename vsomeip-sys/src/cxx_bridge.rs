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

/// autocxx fails to generate bindings currently to functions which contain usage
/// of C++ std::function, so we manually generate these separately using cxx
#[cxx::bridge(namespace = "glue")]
pub mod handler_registration {
    unsafe extern "C++" {
        include!("vsomeip/vsomeip.hpp");
        include!("include/application_wrapper.h");
        include!("application_registrations.h");

        pub unsafe fn request_single_event(
            _application_wrapper: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _notifier: u16,
            _eventgroup: u16,
        );

        pub unsafe fn offer_single_event(
            _application_wrapper: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _notifier: u16,
            _eventgroup: u16,
        );

        type message_handler_fn_ptr = crate::extern_callback_wrappers::MessageHandlerFnPtr;
        type ApplicationWrapper = crate::ffi::glue::ApplicationWrapper;

        /// Registers a message handler
        ///
        /// # Rationale
        ///
        /// This function exists as a workaround, since the vsomeip API for
        /// application::register_message_handler() takes a std::function
        /// which is not supported by autocxx or cxx
        ///
        /// So we instead use this C++ glue code which accepts a function pointer
        /// and creates an std::function with which to then call application::register_message_handler()
        ///
        /// # Parameters
        ///
        /// * _application_wrapper - An [ApplicationWrapper]
        /// * _service - A SOME/IP [service_t](crate::vsomeip::service_t), i.e service ID
        /// * _instance - A SOME/IP [instance_t](crate::vsomeip::instance_t), i.e instance ID
        /// * _method - A SOME/IP [method_t](crate::vsomeip::method_t), i.e method/event ID
        /// * _fn_ptr_handler - A [MessageHandlerFnPtr](crate::extern_callback_wrappers::MessageHandlerFnPtr)
        pub unsafe fn register_message_handler_fn_ptr(
            _application_wrapper: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _method: u16,
            _fn_ptr_handler: message_handler_fn_ptr,
        );

        type availability_handler_fn_ptr =
            crate::extern_callback_wrappers::AvailabilityHandlerFnPtr;

        /// Registers an availability handler
        ///
        /// # Rationale
        ///
        /// This function exists as a workaround, since the vsomeip API for
        /// application::register_availability_handler() takes a std::function
        /// which is not supported by autocxx or cxx
        ///
        /// So we instead use this C++ glue code which accepts a function pointer
        /// and creates a lambda with which to then call application::register_availablity_handler()
        ///
        /// # Parameters
        ///
        /// * _application_wrapper - An [ApplicationWrapper]
        /// * _service - A SOME/IP [service_t](crate::vsomeip::service_t), i.e service ID
        /// * _instance - A SOME/IP [instance_t](crate::vsomeip::instance_t), i.e instance ID
        /// * _fn_ptr_handler - A [AvailabilityHandlerFnPtr](crate::extern_callback_wrappers::AvailabilityHandlerFnPtr)
        /// * _major - A SOME/IP [major_version_t](crate::vsomeip::major_version_t), i.e the major version
        /// * _minor - A SOME/IP [minor_version_t](crate::vsomeip::minor_version_t), i.e the major version
        pub unsafe fn register_availability_handler_fn_ptr(
            _application_wrapper: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _fn_ptr_handler: availability_handler_fn_ptr,
            _major: u8,
            _minor: u32,
        );

        type subscription_status_handler_fn_ptr =
            crate::extern_callback_wrappers::SubscriptionStatusHandlerFnPtr;

        /// Registers a subscription status handler
        ///
        /// # Rationale
        ///
        /// This function exists as a workaround, since the vsomeip API for
        /// application::register_subscription_status_handler() takes a std::function
        /// which is not supported by autocxx or cxx
        ///
        /// So we instead use this C++ glue code which accepts a function pointer
        /// and creates a lambda with which to then call application::register_subscription_status_handler()
        ///
        /// # Parameters
        ///
        /// * _application_wrapper - An [crate::cxx_bridge::handler_registration::ApplicationWrapper]
        /// * _service - A SOME/IP [service_t](crate::vsomeip::service_t), i.e service ID
        /// * _instance - A SOME/IP [instance_t](crate::vsomeip::instance_t), i.e instance ID
        /// * _eventgroup - A SOME/IP [eventgroup_t](crate::vsomeip::eventgroup_t)
        /// * _event - A SOME/IP [event_t](crate::vsomeip::event_t)
        /// * _fn_ptr_handler - A [SubscriptionStatusHandlerFnPtr](crate::extern_callback_wrappers::SubscriptionStatusHandlerFnPtr)
        /// * _is_selective - If true the callback is called even when there's an error registering the subscription
        pub unsafe fn register_subscription_status_handler_fn_ptr(
            _application_wrapper: *mut ApplicationWrapper,
            _service: u16,
            _instance: u16,
            _eventgroup: u16,
            _event: u16,
            _fn_ptr_handler: subscription_status_handler_fn_ptr,
            _is_selective: bool,
        );

        type state_handler_fn_ptr = crate::extern_callback_wrappers::AvailableStateHandlerFnPtr;

        pub unsafe fn register_state_handler_fn_ptr(
            application_wrapper: *mut ApplicationWrapper,
            _fn_ptr_handler: state_handler_fn_ptr,
        );
    }
}

/// autocxx fails to generate bindings to these functions, so we write the bindings for them
/// by hand and inject them into the vsomeip_v3 namespace
#[cxx::bridge(namespace = "vsomeip_v3")]
mod autocxx_failed {
    unsafe extern "C++" {
        include!("vsomeip/vsomeip.hpp");

        type payload = crate::vsomeip::payload;

        /// # Safety
        ///
        /// We are simply creating a binding here for one that autocxx failed to generate
        #[allow(clippy::missing_safety_doc)]
        pub(crate) unsafe fn set_data(self: Pin<&mut payload>, _data: *const u8, _length: u32);

        pub fn get_data(self: &payload) -> *const u8;

        pub fn get_length(self: &payload) -> u32;
    }
}
