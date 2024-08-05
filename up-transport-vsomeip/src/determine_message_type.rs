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

use crate::{ClientId, UeId};
use up_rust::{UCode, UStatus, UUri};

/// Registration type containing the [ClientId] of the [vsomeip_sys::vsomeip::application]
/// which should be used for this message
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RegistrationType {
    Publish(ClientId),
    Request(ClientId),
    Response(ClientId),
    AllPointToPoint(ClientId),
}

impl RegistrationType {
    /// Get the [ClientId] of the [vsomeip_sys::vsomeip::application]
    /// which should be used for this message
    pub fn client_id(&self) -> ClientId {
        match self {
            RegistrationType::Publish(client_id) => *client_id,
            RegistrationType::Request(client_id) => *client_id,
            RegistrationType::Response(client_id) => *client_id,
            RegistrationType::AllPointToPoint(client_id) => *client_id,
        }
    }
}

/// infer the type of message desired based on the filters provided in a registration / unregistration
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

/// infer the type of message desired based on the source and sink contained in a message
pub(crate) fn determine_send_type(
    source_filter: &UUri,
    sink_filter: &Option<UUri>,
) -> Result<RegistrationType, UStatus> {
    determine_type(source_filter, sink_filter, None, DeterminationType::Send)
}

/// Whether the [determine_type] function should treat this as a registration / unregistration or send
enum DeterminationType {
    Register,
    Send,
}

/// Determines [RegistrationType] of a source and sink filter [UUri]
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
            DeterminationType::Register => match my_ue_id {
                None => {
                    return Err(UStatus::fail_with_code(
                        UCode::INTERNAL,
                        "Should have been an own ue_id available in this path",
                    ));
                }
                Some(ue_id) => ue_id,
            },
            DeterminationType::Send => source_filter.ue_id,
        };
        Ok(RegistrationType::Publish(client_id as ClientId))
    }
}

// TODO: Add unit tests
