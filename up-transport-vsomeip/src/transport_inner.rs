/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

pub mod transport_inner_engine;
pub mod transport_inner_handle;

pub const UP_CLIENT_VSOMEIP_TAG: &str = "UPClientVsomeipInner";
pub const UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP: &str = "app_event_loop";
pub const UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL: &str = "register_listener_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL: &str =
    "unregister_listener_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL: &str = "send_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL: &str =
    "initialize_new_app_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_START_APP: &str = "start_app";
pub const UP_CLIENT_VSOMEIP_FN_TAG_STOP_APP: &str = "stop_app";

const INTERNAL_FUNCTION_TIMEOUT: u64 = 3;
