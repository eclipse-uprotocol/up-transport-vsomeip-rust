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

use crate::{EventId, InstanceId, MethodId, ServiceId};
use std::collections::HashSet;
use std::sync::RwLock;

type OfferedServices = HashSet<(ServiceId, InstanceId, MethodId)>;
type RequestedServices = HashSet<(ServiceId, InstanceId, MethodId)>;
type OfferedEvents = HashSet<(ServiceId, InstanceId, EventId)>;
type RequestedEvents = HashSet<(ServiceId, InstanceId, MethodId)>;

pub trait VsomeipOfferedRequestedRegistry: Send + Sync {
    /// Check if a vsomeip service has been offered
    fn is_service_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool;

    /// Insert vsomeip offered service metadata
    fn insert_service_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool;

    /// Check if vsomeip service has been requested
    fn is_service_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool;

    /// Insert vsomeip requested service metadata
    fn insert_service_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool;

    /// Check if event has been offered
    fn is_event_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool;

    /// Insert vsomeip offered event metadata
    fn insert_event_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool;

    /// Check if vsomeip event has been requested
    fn is_event_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool;

    /// Insert vsomeip requested event metadata
    fn insert_event_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool;
}

/// vsomeip services, events: offered and requested
pub struct InMemoryVsomeipOfferedRequestedRegistry {
    offered_services: RwLock<OfferedServices>,
    requested_services: RwLock<RequestedServices>,
    offered_events: RwLock<OfferedEvents>,
    requested_events: RwLock<RequestedEvents>,
}

impl InMemoryVsomeipOfferedRequestedRegistry {
    /// Create a [InMemoryVsomeipOfferedRequestedRegistry]
    pub fn new() -> Self {
        Self {
            offered_services: RwLock::new(HashSet::new()),
            requested_services: RwLock::new(HashSet::new()),
            offered_events: RwLock::new(HashSet::new()),
            requested_events: RwLock::new(HashSet::new()),
        }
    }

    /// Check if a vsomeip service has been offered
    pub fn is_service_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.offered_services
            .read()
            .unwrap()
            .contains(&(service_id, instance_id, method_id))
    }

    /// Insert vsomeip offered service metadata
    pub fn insert_service_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.offered_services
            .write()
            .unwrap()
            .insert((service_id, instance_id, method_id))
    }

    /// Check if vsomeip service has been requested
    pub fn is_service_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.requested_services
            .read()
            .unwrap()
            .contains(&(service_id, instance_id, method_id))
    }

    /// Insert vsomeip requested service metadata
    pub fn insert_service_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.requested_services
            .write()
            .unwrap()
            .insert((service_id, instance_id, method_id))
    }

    /// Check if event has been offered
    pub fn is_event_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.offered_events
            .read()
            .unwrap()
            .contains(&(service_id, instance_id, event_id))
    }

    /// Insert vsomeip offered event metadata
    pub fn insert_event_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.offered_events
            .write()
            .unwrap()
            .insert((service_id, instance_id, event_id))
    }

    /// Check if vsomeip event has been requested
    pub fn is_event_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.requested_events
            .read()
            .unwrap()
            .contains(&(service_id, instance_id, event_id))
    }

    /// Insert vsomeip requested event metadata
    pub fn insert_event_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.requested_events
            .write()
            .unwrap()
            .insert((service_id, instance_id, event_id))
    }
}

// TODO: Add unit tests
