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
use lazy_static::lazy_static;
use std::collections::HashSet;
use tokio::sync::RwLock;

lazy_static! {
    static ref OFFERED_SERVICES: RwLock<HashSet<(ServiceId, InstanceId, MethodId)>> =
        RwLock::new(HashSet::new());
    static ref REQUESTED_SERVICES: RwLock<HashSet<(ServiceId, InstanceId, MethodId)>> =
        RwLock::new(HashSet::new());
    static ref OFFERED_EVENTS: RwLock<HashSet<(ServiceId, InstanceId, EventId)>> =
        RwLock::new(HashSet::new());
    static ref REQUESTED_EVENTS: RwLock<HashSet<(ServiceId, InstanceId, MethodId)>> =
        RwLock::new(HashSet::new());
}

pub(crate) async fn is_service_offered(
    service_id: ServiceId,
    instance_id: InstanceId,
    method_id: MethodId,
) -> bool {
    let offered_services = OFFERED_SERVICES.read().await;
    offered_services.contains(&(service_id, instance_id, method_id))
}

pub(crate) async fn insert_service_offered(
    service_id: ServiceId,
    instance_id: InstanceId,
    method_id: MethodId,
) {
    let mut offered_services = OFFERED_SERVICES.write().await;
    offered_services.insert((service_id, instance_id, method_id));
}

pub(crate) async fn is_service_requested(
    service_id: ServiceId,
    instance_id: InstanceId,
    method_id: MethodId,
) -> bool {
    let requested_services = REQUESTED_SERVICES.read().await;
    requested_services.contains(&(service_id, instance_id, method_id))
}

pub(crate) async fn insert_service_requested(
    service_id: ServiceId,
    instance_id: InstanceId,
    method_id: MethodId,
) {
    let mut requested_services = REQUESTED_SERVICES.write().await;
    requested_services.insert((service_id, instance_id, method_id));
}

pub(crate) async fn is_event_offered(
    service_id: ServiceId,
    instance_id: InstanceId,
    event_id: EventId,
) -> bool {
    let offered_events = OFFERED_EVENTS.read().await;
    offered_events.contains(&(service_id, instance_id, event_id))
}

pub(crate) async fn insert_event_offered(
    service_id: ServiceId,
    instance_id: InstanceId,
    event_id: EventId,
) {
    let mut offered_events = OFFERED_EVENTS.write().await;
    offered_events.insert((service_id, instance_id, event_id));
}

pub(crate) async fn is_event_requested(
    service_id: ServiceId,
    instance_id: InstanceId,
    event_id: EventId,
) -> bool {
    let requested_events = REQUESTED_EVENTS.read().await;
    requested_events.contains(&(service_id, instance_id, event_id))
}

pub(crate) async fn insert_event_requested(
    service_id: ServiceId,
    instance_id: InstanceId,
    event_id: EventId,
) {
    let mut requested_events = REQUESTED_EVENTS.write().await;
    requested_events.insert((service_id, instance_id, event_id));
}

// TODO: Add unit tests
