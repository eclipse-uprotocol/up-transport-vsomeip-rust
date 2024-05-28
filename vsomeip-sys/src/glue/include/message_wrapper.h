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

std::unique_ptr<MessageWrapper> make_message_wrapper(std::shared_ptr<vsomeip_v3::message> ptr) {
    return std::make_unique<MessageWrapper>(std::move(ptr));
}

inline vsomeip_v3::message_base& upcast(vsomeip_v3::message& derived) {
    return derived;
}

} // namespace glue
