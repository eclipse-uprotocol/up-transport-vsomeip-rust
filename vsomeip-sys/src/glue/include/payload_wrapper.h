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

std::unique_ptr<PayloadWrapper> make_payload_wrapper(std::shared_ptr<vsomeip_v3::payload> ptr) {
    return std::make_unique<PayloadWrapper>(std::move(ptr));
}

void set_payload_raw(vsomeip_v3::message* message_ptr, const vsomeip_v3::payload* payload_ptr) {
    std::shared_ptr<vsomeip_v3::payload> sptr(const_cast<vsomeip_v3::payload*>(payload_ptr), [](vsomeip_v3::payload*){});
    message_ptr->set_payload(sptr);
}

const vsomeip_v3::payload* get_payload_raw(const vsomeip_v3::message* self) {
    auto sp = self->get_payload();
    return sp.get();
}

std::shared_ptr<vsomeip_v3::payload> clone_payload(const vsomeip_v3::payload* payload_ptr) {
    return std::shared_ptr<vsomeip_v3::payload>(const_cast<vsomeip_v3::payload*>(payload_ptr), [](vsomeip_v3::payload*){});
}

std::unique_ptr<PayloadWrapper> create_payload_wrapper(const vsomeip_v3::payload* payload_ptr) {
    std::shared_ptr<vsomeip_v3::payload> sptr = clone_payload(payload_ptr);
    return std::make_unique<PayloadWrapper>(sptr);
}

} // namespace glue
