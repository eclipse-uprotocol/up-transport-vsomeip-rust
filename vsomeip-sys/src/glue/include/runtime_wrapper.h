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

/**
 *
 * \brief A C++ wrapper around an `std::shared_ptr<runtime>`
 *
 * Need as a helpful shim to work around the current challenges of interop between Rust
 * and lifetimes of `std::shared_ptr<>` in C++
 *
 */
class RuntimeWrapper {
public:
    explicit RuntimeWrapper(std::shared_ptr<vsomeip_v3::runtime> ptr) : ptr_(std::move(ptr)) {}

    vsomeip_v3::runtime* get_mut() const {
        return ptr_.get();
    }

private:
    std::shared_ptr<vsomeip_v3::runtime> ptr_;
};

/**
 *
 * \brief Allows us to wrap a `std::shared_ptr<vsomeip_v3::runtime>` into a
 *        `std::unique_ptr<RuntimeWrapper>`
 *
 * We can then use the `std::unique<RuntimeWrapper>` with the `get_pinned_runtime()`
 * function to call methods on a `vsomeip_v3::runtime` which mutate it
 *
 */
std::unique_ptr<RuntimeWrapper> make_runtime_wrapper(std::shared_ptr<vsomeip_v3::runtime> ptr) {
    return std::make_unique<RuntimeWrapper>(std::move(ptr));
}

} // namespace glue
