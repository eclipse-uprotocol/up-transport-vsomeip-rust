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

use crate::transport::{
    CLIENT_ID_APP_MAPPING, CLIENT_ID_SESSION_ID_TRACKING, FREE_LISTENER_IDS, LISTENER_ID_MAP,
};
use crate::{ApplicationName, ClientId, RegistrationType, RequestId, SessionId, UeId};
use log::{info, trace};
use up_rust::{ComparableListener, UCode, UStatus, UUri};

pub(crate) async fn free_listener_id(listener_id: usize) -> UStatus {
    info!("listener_id was not used since we already have registered for this");
    let mut free_ids = FREE_LISTENER_IDS.write().await;
    free_ids.insert(listener_id);
    UStatus::fail_with_code(
        UCode::ALREADY_EXISTS,
        "Already have registered with this source, sink and listener",
    )
}

pub(crate) async fn insert_into_listener_id_map(
    key: (UUri, Option<UUri>, ComparableListener),
    listener_id: usize,
) -> bool {
    let mut id_map = LISTENER_ID_MAP.write().await;
    if id_map.insert(key, listener_id).is_some() {
        trace!(
            "Not inserted into LISTENER_ID_MAP since wa already have registered for this Request"
        );
        false
    } else {
        trace!("Inserted into LISTENER_ID_MAP");
        true
    }
}

pub(crate) async fn find_available_listener_id() -> Result<usize, UStatus> {
    let mut free_ids = FREE_LISTENER_IDS.write().await;
    if let Some(&id) = free_ids.iter().next() {
        free_ids.remove(&id);
        Ok(id)
    } else {
        Err(UStatus::fail_with_code(
            UCode::RESOURCE_EXHAUSTED,
            "No more extern C fns available",
        ))
    }
}

pub(crate) async fn find_app_name(client_id: ClientId) -> Result<ApplicationName, UStatus> {
    let client_id_app_mapping = CLIENT_ID_APP_MAPPING.read().await;
    if let Some(app_name) = client_id_app_mapping.get(&client_id) {
        Ok(app_name.clone())
    } else {
        Err(UStatus::fail_with_code(
            UCode::NOT_FOUND,
            format!("There was no app_name found for client_id: {}", client_id),
        ))
    }
}

pub(crate) fn split_u32_to_u16(value: u32) -> (u16, u16) {
    let most_significant_bits = (value >> 16) as u16;
    let least_significant_bits = (value & 0xFFFF) as u16;
    (most_significant_bits, least_significant_bits)
}

pub(crate) fn split_u32_to_u8(value: u32) -> (u8, u8, u8, u8) {
    let byte1 = (value >> 24) as u8;
    let byte2 = (value >> 16 & 0xFF) as u8;
    let byte3 = (value >> 8 & 0xFF) as u8;
    let byte4 = (value & 0xFF) as u8;
    (byte1, byte2, byte3, byte4)
}

pub(crate) async fn retrieve_session_id(client_id: ClientId) -> SessionId {
    let mut client_id_session_id_tracking = CLIENT_ID_SESSION_ID_TRACKING.write().await;

    let current_sesion_id = client_id_session_id_tracking.entry(client_id).or_insert(1);
    let returned_session_id = *current_sesion_id;
    *current_sesion_id += 1;
    returned_session_id
}

pub(crate) fn create_request_id(client_id: ClientId, session_id: SessionId) -> RequestId {
    ((client_id as u32) << 16) | (session_id as u32)
}

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

pub(crate) fn determine_message_type(
    source_filter: &UUri,
    sink_filter: &Option<UUri>,
) -> Result<RegistrationType, UStatus> {
    determine_type(source_filter, sink_filter, None, DeterminationType::Message)
}

enum DeterminationType {
    Register,
    Message,
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
            source_filter.authority_name == "*" // TODO: Is this good enough? Maybe have configurable in UPClientVsomeip?
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

        let client_id = {
            match determination_type {
                DeterminationType::Register => sink_filter.ue_id as ClientId,
                DeterminationType::Message => source_filter.ue_id as ClientId,
            }
        };

        if sink_filter.resource_id == 0 {
            Ok(RegistrationType::Response(client_id))
        } else {
            Ok(RegistrationType::Request(client_id))
        }
    } else {
        let client_id = {
            match determination_type {
                DeterminationType::Register => {
                    my_ue_id.expect("Should have been a own ue_id available in this path")
                }
                DeterminationType::Message => source_filter.ue_id as ClientId,
            }
        };
        Ok(RegistrationType::Publish(client_id))
    }
}
