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

#[repr(transparent)]
pub struct MessageHandlerFnPtr(pub extern "C" fn(&SharedPtr<vsomeip::message>));

unsafe impl ExternType for MessageHandlerFnPtr {
    type Id = type_id!("glue::message_handler_fn_ptr");
    type Kind = cxx::kind::Trivial;
}
