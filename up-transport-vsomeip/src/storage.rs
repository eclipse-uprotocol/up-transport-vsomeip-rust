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

pub mod application_registry;
pub mod application_state_availability_handler_registry;
pub mod message_handler_registry;
pub mod rpc_correlation;
pub mod vsomeip_offered_requested;

use crate::storage::application_registry::ApplicationRegistry;
use crate::storage::message_handler_registry::{
    ClientUsage, GetMessageHandlerError, MessageHandlerRegistry,
};
use crate::storage::rpc_correlation::RpcCorrelationRegistry;
use crate::storage::vsomeip_offered_requested::VsomeipOfferedRequestedRegistry;
use crate::storage::{
    application_registry::InMemoryApplicationRegistry,
    application_state_availability_handler_registry::{
        ApplicationStateAvailabilityHandlerRegistry,
        InMemoryApplicationStateAvailabilityHandlerRegistry,
    },
    message_handler_registry::InMemoryMessageHandlerRegistry,
    rpc_correlation::InMemoryRpcCorrelationRegistry,
    vsomeip_offered_requested::InMemoryVsomeipOfferedRequestedRegistry,
};
use crate::{
    ApplicationName, AuthorityName, ClientId, EventId, InstanceId, MethodId, ServiceId, SessionId,
    SomeIpRequestId, UProtocolReqId, UeId,
};
use crossbeam_channel::Receiver;
use std::sync::Arc;
use tokio::runtime::Handle;
use up_rust::{ComparableListener, UListener, UStatus, UUri};
use vsomeip_sys::glue::{AvailableStateHandlerFnPtr, MessageHandlerFnPtr};
use vsomeip_sys::vsomeip;

pub struct UPTransportVsomeipStorage {
    ue_id: UeId,
    local_authority: AuthorityName,
    remote_authority: AuthorityName,
    runtime_handle: Handle,
    message_handler_registry: Arc<InMemoryMessageHandlerRegistry>,
    application_state_handler_registry: Arc<InMemoryApplicationStateAvailabilityHandlerRegistry>,
    application_registry: Arc<InMemoryApplicationRegistry>,
    rpc_correlation: Arc<InMemoryRpcCorrelationRegistry>,
    vsomeip_offered_requested: Arc<InMemoryVsomeipOfferedRequestedRegistry>,
}

impl UPTransportVsomeipStorage {
    pub fn new(
        local_authority: AuthorityName,
        remote_authority: AuthorityName,
        ue_id: UeId,
        runtime_handle: Handle,
    ) -> Self {
        let application_state_handler_registry =
            InMemoryApplicationStateAvailabilityHandlerRegistry::new_trait_obj();

        Self {
            ue_id,
            local_authority,
            remote_authority,
            runtime_handle,
            message_handler_registry: Arc::new(InMemoryMessageHandlerRegistry::new()),
            application_state_handler_registry,
            application_registry: Arc::new(InMemoryApplicationRegistry::new()),
            rpc_correlation: Arc::new(InMemoryRpcCorrelationRegistry::new()),
            vsomeip_offered_requested: Arc::new(InMemoryVsomeipOfferedRequestedRegistry::new()),
        }
    }

    pub fn get_runtime_handle(&self) -> Handle {
        self.runtime_handle.clone()
    }
    pub fn get_local_authority(&self) -> AuthorityName {
        self.local_authority.clone()
    }

    pub fn get_remote_authority(&self) -> AuthorityName {
        self.remote_authority.clone()
    }

    pub fn get_ue_id(&self) -> UeId {
        self.ue_id
    }
}

impl ApplicationStateAvailabilityHandlerRegistry for UPTransportVsomeipStorage {
    fn get_application_state_availability_handler(
        &self,
        state_handler_id: usize,
    ) -> (AvailableStateHandlerFnPtr, Receiver<vsomeip::state_type_e>) {
        self.application_state_handler_registry
            .get_state_handler(state_handler_id)
    }

    fn free_application_state_availability_handler_id(
        &self,
        state_handler_id: usize,
    ) -> Result<(), UStatus> {
        self.application_state_handler_registry
            .free_state_handler_id(state_handler_id)
    }

    fn find_application_state_availability_handler_id(&self) -> Result<usize, UStatus> {
        self.application_state_handler_registry
            .find_available_state_handler_id()
    }
}

impl RpcCorrelationRegistry for UPTransportVsomeipStorage {
    fn retrieve_session_id(&self, client_id: ClientId) -> SessionId {
        self.rpc_correlation.retrieve_session_id(client_id)
    }

    fn insert_ue_request_correlation(
        &self,
        someip_request_id: SomeIpRequestId,
        uprotocol_req_id: &UProtocolReqId,
    ) -> Result<(), UStatus> {
        self.rpc_correlation
            .insert_ue_request_correlation(someip_request_id, uprotocol_req_id)
    }

    fn remove_ue_request_correlation(
        &self,
        someip_request_id: SomeIpRequestId,
    ) -> Result<UProtocolReqId, UStatus> {
        self.rpc_correlation
            .remove_ue_request_correlation(someip_request_id)
    }

    fn insert_me_request_correlation(
        &self,
        uprotocol_req_id: UProtocolReqId,
        someip_request_id: SomeIpRequestId,
    ) -> Result<(), UStatus> {
        self.rpc_correlation
            .insert_me_request_correlation(uprotocol_req_id, someip_request_id)
    }

    fn remove_me_request_correlation(
        &self,
        uprotocol_req_id: &UProtocolReqId,
    ) -> Result<SomeIpRequestId, UStatus> {
        self.rpc_correlation
            .remove_me_request_correlation(uprotocol_req_id)
    }
}

impl VsomeipOfferedRequestedRegistry for UPTransportVsomeipStorage {
    fn is_service_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.vsomeip_offered_requested
            .is_service_offered(service_id, instance_id, method_id)
    }

    fn insert_service_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.vsomeip_offered_requested
            .insert_service_offered(service_id, instance_id, method_id)
    }

    fn is_service_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.vsomeip_offered_requested
            .is_service_requested(service_id, instance_id, method_id)
    }

    fn insert_service_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        method_id: MethodId,
    ) -> bool {
        self.vsomeip_offered_requested
            .insert_service_requested(service_id, instance_id, method_id)
    }

    fn is_event_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.vsomeip_offered_requested
            .is_event_offered(service_id, instance_id, event_id)
    }

    fn insert_event_offered(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.vsomeip_offered_requested
            .insert_event_offered(service_id, instance_id, event_id)
    }

    fn is_event_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.vsomeip_offered_requested
            .is_event_requested(service_id, instance_id, event_id)
    }

    fn insert_event_requested(
        &self,
        service_id: ServiceId,
        instance_id: InstanceId,
        event_id: EventId,
    ) -> bool {
        self.vsomeip_offered_requested
            .insert_event_requested(service_id, instance_id, event_id)
    }
}

impl ApplicationRegistry for UPTransportVsomeipStorage {
    fn get_app_name_for_client_id(&self, client_id: ClientId) -> Option<ApplicationName> {
        self.application_registry
            .get_app_name_for_client_id(client_id)
    }

    fn insert_client_and_app_name(
        &self,
        client_id: ClientId,
        app_name: ApplicationName,
    ) -> Result<(), UStatus> {
        self.application_registry
            .insert_client_and_app_name(client_id, app_name)
    }

    fn remove_app_name_for_client_id(&self, client_id: ClientId) -> Option<ApplicationName> {
        self.application_registry
            .remove_app_name_for_client_id(client_id)
    }
}

impl MessageHandlerRegistry for UPTransportVsomeipStorage {
    fn get_message_handler(
        &self,
        client_id: ClientId,
        transport_storage: Arc<UPTransportVsomeipStorage>,
        listener_config: (UUri, Option<UUri>, ComparableListener),
    ) -> Result<MessageHandlerFnPtr, GetMessageHandlerError> {
        self.message_handler_registry.get_message_handler(
            client_id,
            transport_storage,
            listener_config,
        )
    }

    fn release_message_handler(
        &self,
        listener_config: (UUri, Option<UUri>, ComparableListener),
    ) -> Result<ClientUsage, UStatus> {
        self.message_handler_registry
            .release_message_handler(listener_config)
    }

    fn get_all_listener_configs(&self) -> Vec<(UUri, Option<UUri>, ComparableListener)> {
        self.message_handler_registry.get_all_listener_configs()
    }

    fn get_listener_for_message_handler_id(
        &self,
        message_handler_id: usize,
    ) -> Option<Arc<dyn UListener>> {
        self.message_handler_registry
            .get_listener_for_message_handler_id(message_handler_id)
    }
}
