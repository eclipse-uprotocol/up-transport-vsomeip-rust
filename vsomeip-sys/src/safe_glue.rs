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

use crate::cxx_bridge::handler_registration::register_availability_handler_fn_ptr;
use crate::cxx_bridge::handler_registration::register_message_handler_fn_ptr;
use crate::extern_callback_wrappers::{AvailabilityHandlerFnPtr, MessageHandlerFnPtr};
use crate::ffi::glue::{get_payload_raw, set_payload_raw};
use crate::glue::upcast;
use crate::glue::{ApplicationWrapper, MessageWrapper, PayloadWrapper, RuntimeWrapper};
use crate::unsafe_fns::create_payload_wrapper;
use crate::vsomeip::message_base;
use crate::vsomeip::{application, message, payload, runtime};
use cxx::UniquePtr;
use std::pin::Pin;
use std::slice;

pub fn get_pinned_runtime(wrapper: &RuntimeWrapper) -> Pin<&mut runtime> {
    unsafe { Pin::new_unchecked(wrapper.get_mut().as_mut().unwrap()) }
}

pub fn get_pinned_application(wrapper: &ApplicationWrapper) -> Pin<&mut application> {
    unsafe { Pin::new_unchecked(wrapper.get_mut().as_mut().unwrap()) }
}

pub fn get_pinned_message(wrapper: &MessageWrapper) -> Pin<&mut message> {
    unsafe { Pin::new_unchecked(wrapper.get_mut().as_mut().unwrap()) }
}

pub fn get_message(wrapper: &MessageWrapper) -> Pin<&mut message> {
    unsafe {
        let msg_ptr: *mut message = wrapper.get_mut();
        if msg_ptr.is_null() {
            panic!("msg_ptr is null");
        }
        Pin::new_unchecked(msg_ptr.as_mut().unwrap())
    }
}

pub fn get_pinned_message_base(wrapper: &MessageWrapper) -> Pin<&mut message_base> {
    unsafe {
        let msg_ptr: *mut message = wrapper.get_mut();
        if msg_ptr.is_null() {
            panic!("msg_ptr is null");
        }

        // Convert the raw pointer to a mutable reference
        let msg_ref: &mut message = &mut *msg_ptr;

        // Pin the mutable reference
        let pinned_msg_ref: Pin<&mut message> = Pin::new_unchecked(msg_ref);

        // Use the upcast function to get a pinned mutable reference to message_base
        let pinned_base_ref: Pin<&mut message_base> = upcast(pinned_msg_ref);

        pinned_base_ref
    }
}

pub fn get_pinned_payload(wrapper: &PayloadWrapper) -> Pin<&mut payload> {
    unsafe { Pin::new_unchecked(wrapper.get_mut().as_mut().unwrap()) }
}

pub fn set_data_safe(payload: Pin<&mut payload>, _data: &[u8]) {
    // Get the length of the data
    let length = _data.len() as u32;

    // Get a pointer to the data
    let data_ptr = _data.as_ptr();

    unsafe {
        payload.set_data(data_ptr, length);
    }
}

pub fn get_data_safe(payload_wrapper: &PayloadWrapper) -> Vec<u8> {
    let length = get_pinned_payload(payload_wrapper).get_length();
    let data_ptr = get_pinned_payload(payload_wrapper).get_data();

    // Convert the raw pointer and length to a slice
    let data_slice: &[u8] = unsafe { slice::from_raw_parts(data_ptr, length as usize) };

    // Convert the slice to a Vec
    let data_vec: Vec<u8> = data_slice.to_vec();

    data_vec
}

pub fn set_message_payload(
    message_wrapper: &mut UniquePtr<MessageWrapper>,
    payload_wrapper: &mut UniquePtr<PayloadWrapper>,
) {
    unsafe {
        let message_pin = Pin::new_unchecked(&mut *message_wrapper);
        let payload_pin = Pin::new_unchecked(&mut *payload_wrapper);
        let message_ptr = MessageWrapper::get_mut(&message_pin);
        let payload_ptr = PayloadWrapper::get_mut(&payload_pin);
        set_payload_raw(message_ptr, payload_ptr);
    }
}

pub fn get_message_payload(
    message_wrapper: &mut UniquePtr<MessageWrapper>,
) -> UniquePtr<PayloadWrapper> {
    unsafe {
        if message_wrapper.is_null() {
            eprintln!("message_wrapper is null");
            return cxx::UniquePtr::null();
        }

        let message_pin = Pin::new_unchecked(message_wrapper.as_mut().unwrap());
        let message_ptr = MessageWrapper::get_mut(&message_pin) as *const message;

        if (message_ptr as *const ()).is_null() {
            eprintln!("message_ptr is null");
            return UniquePtr::null();
        }

        let payload_ptr = get_payload_raw(message_ptr);

        if (payload_ptr as *const ()).is_null() {
            eprintln!("payload_ptr is null");
            return UniquePtr::null();
        }

        println!("get_message_payload: payload_ptr = {:?}", payload_ptr);

        // Use the intermediate function to create a UniquePtr<PayloadWrapper>
        let payload_wrapper = create_payload_wrapper(payload_ptr);

        if payload_wrapper.is_null() {
            eprintln!("Failed to create UniquePtr<PayloadWrapper>");
        } else {
            println!("Successfully created UniquePtr<PayloadWrapper>");
        }

        payload_wrapper
    }
}

pub fn register_message_handler_fn_ptr_safe(
    application_wrapper: &mut UniquePtr<ApplicationWrapper>,
    _service: u16,
    _instance: u16,
    _method: u16,
    _fn_ptr_handler: MessageHandlerFnPtr,
) {
    unsafe {
        let application_wrapper_ptr = application_wrapper.pin_mut().get_self();
        register_message_handler_fn_ptr(
            application_wrapper_ptr,
            _service,
            _instance,
            _method,
            _fn_ptr_handler,
        );
    }
}

pub fn register_availability_handler_fn_ptr_safe(
    application_wrapper: &mut UniquePtr<ApplicationWrapper>,
    _service: u16,
    _instance: u16,
    _fn_ptr_handler: AvailabilityHandlerFnPtr,
    _major_version: u8,
    _minor_version: u32,
) {
    unsafe {
        let application_wrapper_ptr = application_wrapper.pin_mut().get_self();
        register_availability_handler_fn_ptr(
            application_wrapper_ptr,
            _service,
            _instance,
            _fn_ptr_handler,
            _major_version,
            _minor_version,
        );
    }
}