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

#include "../include/application_wrapper.h"

namespace glue {

std::unique_ptr<ApplicationWrapper> make_application_wrapper(std::shared_ptr<vsomeip_v3::application> ptr) {
    return std::make_unique<ApplicationWrapper>(std::move(ptr));
}

} // namespace glue
