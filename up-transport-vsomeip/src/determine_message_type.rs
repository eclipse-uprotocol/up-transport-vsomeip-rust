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

        // Log each condition separately to identify which one(s) are failing
        let source_wildcard_authority = !source_filter.has_wildcard_authority();
        trace!("source_non_wildcard_authority: {source_wildcard_authority}");

        let source_wildcard_entity_type = source_filter.has_wildcard_entity_type();
        trace!("source_wildcard_entity_type: {source_wildcard_entity_type}");

        let source_wildcard_entity_instance = source_filter.has_wildcard_entity_instance();
        trace!("source_wildcard_entity_instance: {source_wildcard_entity_instance}");

        let source_wildcard_version = source_filter.has_wildcard_version();
        trace!("source_wildcard_version: {source_wildcard_version}");

        let source_wildcard_resource_id = source_filter.has_wildcard_resource_id();
        trace!("source_wildcard_resource_id: {source_wildcard_resource_id}");

        let sink_non_wildcard_authority = !sink_filter.has_wildcard_authority();
        trace!("sink_non_wildcard_authority: {sink_non_wildcard_authority}");

        let sink_wildcard_entity_type = sink_filter.has_wildcard_entity_type();
        trace!("sink_wildcard_entity_type: {sink_wildcard_entity_type}");

        let sink_wildcard_entity_instance = sink_filter.has_wildcard_entity_instance();
        trace!("sink_wildcard_entity_instance: {sink_wildcard_entity_instance}");

        let sink_wildcard_version = sink_filter.has_wildcard_version();
        trace!("sink_wildcard_version: {sink_wildcard_version}");

        let sink_wildcard_resource_id = sink_filter.has_wildcard_resource_id();
        trace!("sink_wildcard_resource_id: {sink_wildcard_resource_id}");

        let streamer_use_case = {
            source_wildcard_authority
                && source_wildcard_entity_type
                && source_wildcard_entity_instance
                && source_wildcard_version
                && source_wildcard_resource_id
                && sink_non_wildcard_authority
                && sink_wildcard_entity_type
                && sink_wildcard_entity_instance
                && sink_wildcard_version
                && sink_wildcard_resource_id
        };
        trace!("streamer_use_case final result: {streamer_use_case}");

        if streamer_use_case {
            trace!("Matched streamer use case - returning AllPointToPoint");
            return Ok(RegistrationType::AllPointToPoint);
        }

        // Log which case we're falling through to
        if sink_filter.resource_id == 0 {
            trace!("sink_filter.resource_id == 0 - returning Response");
            Ok(RegistrationType::Response)
        } else {
            trace!("sink_filter.resource_id != 0 - returning Request");
            Ok(RegistrationType::Request)
        }
    } else {
        trace!("No sink_filter provided - returning Publish");
        Ok(RegistrationType::Publish)
    }
}
