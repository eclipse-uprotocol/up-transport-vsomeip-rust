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

use crate::vsomeip;
use cxx::{type_id, ExternType, SharedPtr};

/// A Rust wrapper around the extern "C" fn used when registering an [availability_handler_t](crate::vsomeip::availability_handler_t)
///
/// # Rationale
///
/// We want the ability to think at a higher level and not need to consider the underlying
/// extern "C" fn, so we wrap this here in a Rust struct
#[repr(transparent)]
pub struct AvailabilityHandlerFnPtr(
    pub  extern "C" fn(
        service: crate::ffi::vsomeip_v3::service_t,
        instance: crate::ffi::vsomeip_v3::instance_t,
        availability: bool,
    ),
);

unsafe impl ExternType for AvailabilityHandlerFnPtr {
    type Id = type_id!("glue::availability_handler_fn_ptr");
    type Kind = cxx::kind::Trivial;
}

/// A Rust wrapper around the extern "C" fn used when registering a [message_handler_t](crate::vsomeip::message_handler_t)
///
/// # Rationale
///
/// We want the ability to think at a higher level and not need to consider the underlying
/// extern "C" fn, so we wrap this here in a Rust struct
#[repr(transparent)]
pub struct MessageHandlerFnPtr(pub extern "C" fn(&SharedPtr<vsomeip::message>));

unsafe impl ExternType for MessageHandlerFnPtr {
    type Id = type_id!("glue::message_handler_fn_ptr");
    type Kind = cxx::kind::Trivial;
}

/// A Rust wrapper around the extern "C" fn used when registering a [subscription_status_handler_t](vsomeip::subscription_status_handler_t)
///
/// # Rationale
///
/// We want the ability to think at a higher level and not need to consider the underlying
/// extern "C" fn, so we wrap this here in a Rust struct
#[repr(transparent)]
pub struct SubscriptionStatusHandlerFnPtr(
    pub  extern "C" fn(
        service: crate::ffi::vsomeip_v3::service_t,
        instance: crate::ffi::vsomeip_v3::instance_t,
        eventgroup: crate::ffi::vsomeip_v3::eventgroup_t,
        event: crate::ffi::vsomeip_v3::event_t,
        status: u16, // TODO: This should really be an enum with a repr of u16: 0x00: OK or 0x7 Not OK
    ),
);

unsafe impl ExternType for SubscriptionStatusHandlerFnPtr {
    type Id = type_id!("glue::subscription_status_handler_fn_ptr");
    type Kind = cxx::kind::Trivial;
}
