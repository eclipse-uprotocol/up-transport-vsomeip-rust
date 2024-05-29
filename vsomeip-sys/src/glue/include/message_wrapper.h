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
 * \brief A C++ wrapper around an `std::shared_ptr<message>`
 *
 * Need as a helpful shim to work around the current challenges of interop between Rust
 * and lifetimes of `std::shared_ptr<>` in C++
 *
 */
class MessageWrapper {
public:
    explicit MessageWrapper(std::shared_ptr<vsomeip_v3::message> ptr) : ptr_(std::move(ptr)) {}

    vsomeip_v3::message* get_mut() const {
        return ptr_.get();
    }

    std::shared_ptr<vsomeip_v3::message> get_shared_ptr() const {
        return ptr_;
    }

private:
    std::shared_ptr<vsomeip_v3::message> ptr_;
};

/**
 *
 * \brief Allows us to wrap a `std::shared_ptr<vsomeip_v3::message>` into a
 *        `std::unique_ptr<MessageWrapper>`
 *
 * We can then use the `std::unique<MessageWrapper>` with the `get_pinned_message()`
 * function to call methods on a `vsomeip_v3::message` which mutate it
 *
 */
std::unique_ptr<MessageWrapper> make_message_wrapper(std::shared_ptr<vsomeip_v3::message> ptr) {
    return std::make_unique<MessageWrapper>(std::move(ptr));
}

/**
 *
 * \brief Allows us to upcast a `vsomeip_v3::message` into a `vsomeip_v3::message_base`
 *        so that we are able to call virtual methods belonging to `vsomeip_v3::message_base`
 *
 * Required because cxx is currently unable to allow us to call virtual methods on a derived class
 *
 */
inline vsomeip_v3::message_base& upcast(vsomeip_v3::message& derived) {
    return derived;
}

} // namespace glue
