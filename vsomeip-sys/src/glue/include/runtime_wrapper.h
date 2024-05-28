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

class RuntimeWrapper {
public:
    explicit RuntimeWrapper(std::shared_ptr<vsomeip_v3::runtime> ptr) : ptr_(std::move(ptr)) {}

    vsomeip_v3::runtime* get_mut() const {
        return ptr_.get();
    }

private:
    std::shared_ptr<vsomeip_v3::runtime> ptr_;
};

std::unique_ptr<RuntimeWrapper> make_runtime_wrapper(std::shared_ptr<vsomeip_v3::runtime> ptr) {
    return std::make_unique<RuntimeWrapper>(std::move(ptr));
}

} // namespace glue
