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

use crate::determine_message_type::{
    determine_registration_type, determine_send_type, RegistrationType,
};
use crate::storage::application_registry::ApplicationRegistry;
use crate::storage::message_handler_registry::MessageHandlerRegistry;
use crate::transport_engine::TransportCommand;
use crate::transport_engine::UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL;
use crate::UPTransportVsomeip;
use async_trait::async_trait;
use log::trace;
use std::sync::Arc;
use tokio::sync::oneshot;
use up_rust::{
    ComparableListener, LocalUriProvider, UAttributesValidators, UCode, UListener, UMessage,
    UStatus, UTransport, UUri,
};

#[async_trait]
impl UTransport for UPTransportVsomeip {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        let attributes = message.attributes.as_ref().ok_or(UStatus::fail_with_code(
            UCode::INVALID_ARGUMENT,
            "Missing uAttributes",
        ))?;

        // Validate UAttributes before conversion.
        UAttributesValidators::get_validator_for_attributes(attributes)
            .validate(attributes)
            .map_err(|e| {
                UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    format!("Invalid uAttributes, err: {e:?}"),
                )
            })?;

        trace!("Sending message with attributes: {:?}", attributes);

        let Some(source_filter) = message.attributes.source.as_ref() else {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "UMessage provided with no source",
            ));
        };

        let sink_filter = message.attributes.sink.as_ref();
        let message_type = determine_send_type(source_filter, &sink_filter.cloned())?;
        trace!("inside send(), message_type: {message_type:?}");
        let app_name_res = {
            if let Some(app_name) = self
                .storage
                .get_app_name_for_client_id(message_type.client_id())
            {
                Ok(app_name)
            } else {
                let client_id = message_type.client_id();
                let app_name = format!("{}", message_type.client_id());
                self.initialize_vsomeip_app(client_id, app_name).await
            }
        };

        let Ok(app_name) = app_name_res else {
            return Err(app_name_res.err().unwrap());
        };

        self.register_for_returning_response_if_point_to_point_listener_and_sending_request(
            source_filter,
            sink_filter,
            message_type.clone(),
        )
        .await?;

        let (tx, rx) = oneshot::channel();
        let send_to_engine_res = Self::send_to_engine_with_status(
            &self.engine.transport_command_sender,
            TransportCommand::Send(
                message,
                message_type,
                app_name,
                self.storage.clone(),
                self.storage.clone(),
                tx,
            ),
        )
        .await;
        if let Err(err) = send_to_engine_res {
            panic!("engine has stopped! unable to proceed! with err: {err:?}");
        }
        Self::await_engine(UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL, rx).await
    }

    async fn register_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        let registration_type = determine_registration_type(
            source_filter,
            &sink_filter.cloned(),
            self.storage.get_ue_id(),
        )?;

        trace!("registration_type: {registration_type:?}");

        if registration_type == RegistrationType::AllPointToPoint(0xFFFF) {
            return self.register_point_to_point_listener(&listener).await;
        }

        let app_name_res = {
            if let Some(app_name) = self
                .storage
                .get_app_name_for_client_id(registration_type.client_id())
            {
                Ok(app_name)
            } else {
                let app_name = format!("{}", registration_type.client_id());
                let client_id = registration_type.client_id();
                self.initialize_vsomeip_app(client_id, app_name).await
            }
        };

        let Ok(app_name) = app_name_res else {
            return Err(app_name_res.err().unwrap());
        };

        let comp_listener = ComparableListener::new(listener);
        let listener_config = (source_filter.clone(), sink_filter.cloned(), comp_listener);
        let Ok(msg_handler) = self.storage.get_message_handler(
            registration_type.client_id(),
            self.storage.clone(),
            listener_config,
        ) else {
            return Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                "Unable to get message handler for register_listener",
            ));
        };

        let (tx, rx) = oneshot::channel();
        let send_to_engine_res = Self::send_to_engine_with_status(
            &self.engine.transport_command_sender,
            TransportCommand::RegisterListener(
                source_filter.clone(),
                sink_filter.cloned(),
                registration_type,
                msg_handler,
                app_name,
                self.storage.clone(),
                tx,
            ),
        )
        .await;
        if let Err(err) = send_to_engine_res {
            panic!("engine has stopped! unable to proceed! err: {err}");
        }

        Self::await_engine("register", rx).await
    }

    async fn unregister_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        self.unregister_listener(source_filter, sink_filter, listener)
    }

    async fn receive(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<UMessage, UStatus> {
        Err(UStatus::fail_with_code(
            UCode::UNIMPLEMENTED,
            "This method is not implemented for vsomeip. Use register_listener instead.",
        ))
    }
}

impl LocalUriProvider for UPTransportVsomeip {
    fn get_authority(&self) -> String {
        self.storage.get_uri().authority_name
    }
    fn get_resource_uri(&self, resource_id: u16) -> UUri {
        let mut resource_uri = self.storage.get_uri();
        resource_uri.resource_id = resource_id as u32;
        resource_uri
    }
    fn get_source_uri(&self) -> UUri {
        self.storage.get_uri()
    }
}
