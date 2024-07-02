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

use crate::{ClientId, RegistrationType, UeId};
use up_rust::{UStatus, UUri};

// infer the type of message desired based on the filters provided
pub(crate) fn determine_registration_type(
    source_filter: &UUri,
    sink_filter: &Option<UUri>,
    my_ue_id: UeId,
) -> Result<RegistrationType, UStatus> {
    determine_type(
        source_filter,
        sink_filter,
        Some(my_ue_id),
        DeterminationType::Register,
    )
}

pub(crate) fn determine_send_type(
    source_filter: &UUri,
    sink_filter: &Option<UUri>,
) -> Result<RegistrationType, UStatus> {
    determine_type(source_filter, sink_filter, None, DeterminationType::Send)
}

enum DeterminationType {
    Register,
    Send,
}

fn determine_type(
    source_filter: &UUri,
    sink_filter: &Option<UUri>,
    my_ue_id: Option<UeId>,
    determination_type: DeterminationType,
) -> Result<RegistrationType, UStatus> {
    if let Some(sink_filter) = &sink_filter {
        // determine if we're in the uStreamer use-case of capturing all point-to-point messages
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
            return Ok(RegistrationType::AllPointToPoint(0xFFFF));
        }

        let client_id = match determination_type {
            DeterminationType::Register => sink_filter.ue_id as ClientId,
            DeterminationType::Send => source_filter.ue_id as ClientId,
        };

        if sink_filter.resource_id == 0 {
            Ok(RegistrationType::Response(client_id))
        } else {
            Ok(RegistrationType::Request(client_id))
        }
    } else {
        let client_id = match determination_type {
            DeterminationType::Register => {
                my_ue_id.expect("Should have been an own ue_id available in this path")
            }
            DeterminationType::Send => source_filter.ue_id as ClientId,
        };
        Ok(RegistrationType::Publish(client_id))
    }
}

// TODO: Add unit tests
