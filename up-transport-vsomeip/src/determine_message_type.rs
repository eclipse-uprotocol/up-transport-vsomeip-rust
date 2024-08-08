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

use log::trace;
use up_rust::{UStatus, UUri};

/// Registration type containing the [ClientId] of the [vsomeip_sys::vsomeip::application]
/// which should be used for this message
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RegistrationType {
    Publish,
    Request,
    Response,
    AllPointToPoint,
}

/// Determines [RegistrationType] of a source and sink filter [UUri]
pub fn determine_type(
    source_filter: &UUri,
    sink_filter: &Option<UUri>,
) -> Result<RegistrationType, UStatus> {
    if let Some(sink_filter) = &sink_filter {
        // determine if we're in the uStreamer use-case of capturing all point-to-point messages
        trace!("source_filter: {source_filter:?}");
        trace!("sink_filter: {sink_filter:?}");
        let streamer_use_case = {
            source_filter.authority_name == "*"
                && source_filter.ue_id == 0x0000_FFFF
                && source_filter.ue_version_major == 0xFF
                && source_filter.resource_id == 0xFFFF
                && sink_filter.authority_name != "*"
                && sink_filter.ue_id == 0x0000_FFFF
                && sink_filter.ue_version_major == 0xFF
                && sink_filter.resource_id == 0xFFFF
        };

        if streamer_use_case {
            return Ok(RegistrationType::AllPointToPoint);
        }

        if sink_filter.resource_id == 0 {
            Ok(RegistrationType::Response)
        } else {
            Ok(RegistrationType::Request)
        }
    } else {
        Ok(RegistrationType::Publish)
    }
}

// TODO: Add unit tests
