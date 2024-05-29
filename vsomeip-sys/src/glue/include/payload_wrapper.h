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

#pragma once

#include <memory>
#include <iostream>
#include "vsomeip/vsomeip.hpp"  // Adjust the path as necessary to include the runtime type

namespace glue {

/**
 *
 * \brief A C++ wrapper around an `std::shared_ptr<payload>`
 *
 * Need as a helpful shim to work around the current challenges of interop between Rust
 * and lifetimes of `std::shared_ptr<>` in C++
 *
 */
class PayloadWrapper {
public:
    explicit PayloadWrapper(std::shared_ptr<vsomeip_v3::payload> ptr) : ptr_(std::move(ptr)) {}

    vsomeip_v3::payload* get_mut() const {
        return ptr_.get();
    }

    std::shared_ptr<vsomeip_v3::payload> get_shared_ptr() const {
        return ptr_;
    }

private:
    std::shared_ptr<vsomeip_v3::payload> ptr_;
};

/**
 *
 * \brief Allows us to wrap a `std::shared_ptr<vsomeip_v3::payload>` into a
 *        `std::unique_ptr<PayloadWrapper>`
 *
 * We can then use the `std::unique<PayloadWrapper>` with the `get_pinned_payload()`
 * function to call methods on a `vsomeip_v3::payload` which mutate it
 *
 */
std::unique_ptr<PayloadWrapper> make_payload_wrapper(std::shared_ptr<vsomeip_v3::payload> ptr) {
    return std::make_unique<PayloadWrapper>(std::move(ptr));
}

/**
 *
 * \brief Allows us to set a `messsage` with the content of a `payload`
 *
 * A glue function to allow us to wrap this with a safe variant within Rust.
 *
 */
void set_payload_raw(vsomeip_v3::message* message_ptr, const vsomeip_v3::payload* payload_ptr) {
    std::shared_ptr<vsomeip_v3::payload> sptr(const_cast<vsomeip_v3::payload*>(payload_ptr), [](vsomeip_v3::payload*){});
    message_ptr->set_payload(sptr);
}

/**
 *
 * \brief Allows us to get a `payload` from a `message`
 *
 * A glue function to allow us to wrap this with a safe variant within Rust.
 *
 */
const vsomeip_v3::payload* get_payload_raw(const vsomeip_v3::message* self) {
    auto sp = self->get_payload();
    return sp.get();
}

/**
 *
 * \brief Allows us create a `std::shared_ptr<vsomeip_v3::payload>` from a pointer to a
 * vsomeip_v3::payload
 *
 * A glue function used to work around the fact that within Rust we're manipulating
 * a `std::unique_ptr<MessageWrapper>` and a `std::unique_ptr<PayloadWrapper>`
 */
std::shared_ptr<vsomeip_v3::payload> clone_payload(const vsomeip_v3::payload* payload_ptr) {
    return std::shared_ptr<vsomeip_v3::payload>(const_cast<vsomeip_v3::payload*>(payload_ptr), [](vsomeip_v3::payload*){});
}

/**
 *
 * \brief Allows us create a `std::unique_ptr<vsomeip_v3::payload>` from a pointer to a
 * vsomeip_v3::payload
 *
 * A glue function used to work around the fact that within Rust we're manipulating
 * a `std::unique_ptr<MessageWrapper>` and a `std::unique_ptr<PayloadWrapper>`
 */
std::unique_ptr<PayloadWrapper> create_payload_wrapper(const vsomeip_v3::payload* payload_ptr) {
    std::shared_ptr<vsomeip_v3::payload> sptr = clone_payload(payload_ptr);
    return std::make_unique<PayloadWrapper>(sptr);
}

} // namespace glue
