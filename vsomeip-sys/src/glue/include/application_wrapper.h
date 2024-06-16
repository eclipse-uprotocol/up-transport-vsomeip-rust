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
#include "vsomeip/vsomeip.hpp"  // Adjust the path as necessary to include the runtime type

namespace glue {

using message_handler_fn_ptr = void (*)(const std::shared_ptr< vsomeip_v3::message > &);
using availability_handler_fn_ptr = void (*)(vsomeip_v3::service_t, vsomeip_v3::instance_t, bool);
using subscription_status_handler_fn_ptr = void (*)(const vsomeip_v3::service_t, const vsomeip_v3::instance_t, const vsomeip_v3::eventgroup_t,
                                                    const vsomeip_v3::event_t, const uint16_t);

/**
 *
 * \brief A C++ wrapper around an `std::shared_ptr<application>`
 *
 * Need as a helpful shim to work around the current challenges of interop between Rust
 * and lifetimes of `std::shared_ptr<>` in C++
 *
 */
class ApplicationWrapper {
public:
    explicit ApplicationWrapper(std::shared_ptr<vsomeip_v3::application> ptr) : ptr_(std::move(ptr)) {}

    vsomeip_v3::application* get_mut() const {
        return ptr_.get();
    }

    std::shared_ptr<vsomeip_v3::application> get_shared_ptr() const {
        return ptr_;
    }

    ApplicationWrapper* get_self() {
        return this;
    }

private:
    std::shared_ptr<vsomeip_v3::application> ptr_;
};

/**
 *
 * \brief Allows us to wrap a `std::shared_ptr<vsomeip_v3::application>` into a
 *        `std::unique_ptr<ApplicationWrapper>`
 *
 * We can then use the `std::unique<ApplicationWrapper>` with the `get_pinned_application()`
 * function to call methods on a `vsomeip_v3::application` which mutate it
 *
 */
std::unique_ptr<ApplicationWrapper> make_application_wrapper(std::shared_ptr<vsomeip_v3::application> ptr);

} // namespace glue
