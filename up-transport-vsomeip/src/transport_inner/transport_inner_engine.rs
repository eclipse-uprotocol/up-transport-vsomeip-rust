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

use crate::determine_message_type::RegistrationType;
use crate::message_conversions::UMessageToVsomeipMessage;
use crate::storage::application_state_availability_handler_registry::ApplicationStateAvailabilityHandlerRegistry;
use crate::storage::rpc_correlation::RpcCorrelationRegistry;
use crate::storage::vsomeip_offered_requested::VsomeipOfferedRequestedRegistry;
use crate::transport_inner::{
    UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP, UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, UP_CLIENT_VSOMEIP_FN_TAG_START_APP,
    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL, UP_CLIENT_VSOMEIP_TAG,
};
use crate::utils::{split_u32_to_u16, split_u32_to_u8};
use crate::{ApplicationName, ClientId, UeId};
use cxx::{let_cxx_string, UniquePtr};
use log::{error, info, trace};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use up_rust::{UCode, UMessage, UMessageType, UStatus, UUri};
use vsomeip_sys::glue::{
    make_application_wrapper, make_payload_wrapper, make_runtime_wrapper, ApplicationWrapper,
    MessageHandlerFnPtr, RuntimeWrapper,
};
use vsomeip_sys::vsomeip;

pub enum TransportCommand {
    // Primary purpose of a UTransport
    RegisterListener(
        UUri,
        Option<UUri>,
        RegistrationType,
        MessageHandlerFnPtr,
        ApplicationName,
        Arc<dyn VsomeipOfferedRequestedRegistry>,
        oneshot::Sender<Result<(), UStatus>>,
    ),
    UnregisterListener(
        UUri,
        Option<UUri>,
        RegistrationType,
        ApplicationName,
        // TODO: Include Arc<dyn VsomeipOfferedRequestedRegistry> here to unregister?
        oneshot::Sender<Result<(), UStatus>>,
    ),
    Send(
        UMessage,
        RegistrationType,
        ApplicationName,
        Arc<dyn RpcCorrelationRegistry>,
        Arc<dyn VsomeipOfferedRequestedRegistry>,
        oneshot::Sender<Result<(), UStatus>>,
    ),
    // Additional helpful commands
    StartVsomeipApp(
        ClientId,
        ApplicationName,
        Arc<dyn ApplicationStateAvailabilityHandlerRegistry>,
        oneshot::Sender<Result<(), UStatus>>,
    ),
    StopVsomeipApp(
        ClientId,
        ApplicationName,
        oneshot::Sender<Result<(), UStatus>>,
    ),
}

pub struct UPTransportVsomeipInnerEngine {
    pub(crate) transport_command_sender: Sender<TransportCommand>,
}

impl UPTransportVsomeipInnerEngine {
    pub fn new(ue_id: UeId, config_path: Option<&Path>) -> Self {
        let (tx, rx) = channel(10000);

        Self::start_event_loop(ue_id, rx, config_path);

        Self {
            transport_command_sender: tx,
        }
    }

    fn start_event_loop(
        ue_id: UeId,
        rx_to_event_loop: Receiver<TransportCommand>,
        config_path: Option<&Path>,
    ) {
        let config_path: Option<PathBuf> = config_path.map(|p| p.to_path_buf());

        thread::spawn(move || {
            trace!("On dedicated thread");

            // Create a new single-threaded runtime
            let runtime = Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime");

            runtime.block_on(async move {
                trace!("Within blocked runtime");
                Self::event_loop(ue_id, rx_to_event_loop, config_path).await;
                info!("Broke out of loop! You probably dropped the UPClientVsomeip");
            });
            trace!("Parking dedicated thread");
            thread::park();
            trace!("Made past dedicated thread park");
        });
        trace!("Past thread spawn");
    }

    fn create_app(
        app_name: &ApplicationName,
        config_path: Option<&Path>,
        application_state_availability_handler_registry: Arc<
            dyn ApplicationStateAvailabilityHandlerRegistry,
        >,
    ) -> Result<(), UStatus> {
        let app_name = app_name.to_string();
        let config_path = config_path.map(|p| p.to_path_buf());
        let runtime_wrapper = make_runtime_wrapper(vsomeip::runtime::get());

        // Check whether application exists beforehand
        let_cxx_string!(app_name_cxx = app_name.clone());
        let application_wrapper =
            make_application_wrapper(runtime_wrapper.get_pinned().get_application(&app_name_cxx));
        if application_wrapper.is_some() {
            return Err(UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                format!("vsomeip app already exists with app_name: {app_name}"),
            ));
        }

        let state_handler_id = application_state_availability_handler_registry
            .find_application_state_availability_handler_id()?;
        let (available_state_handler_fn_ptr, receiver) =
            application_state_availability_handler_registry
                .get_application_state_availability_handler(state_handler_id);

        let app_name_init = app_name.to_string();
        let config_path = config_path.map(|p| p.to_path_buf());
        let_cxx_string!(app_name_cxx = app_name_init.clone());
        let application_wrapper = {
            if let Some(config_path) = config_path {
                let config_path_str = config_path.display().to_string();
                let_cxx_string!(config_path_cxx_str = config_path_str);
                make_application_wrapper(
                    runtime_wrapper
                        .get_pinned()
                        .create_application1(&app_name_cxx, &config_path_cxx_str),
                )
            } else {
                make_application_wrapper(
                    runtime_wrapper
                        .get_pinned()
                        .create_application(&app_name_cxx),
                )
            }
        };
        let Some(application_wrapper) = application_wrapper else {
            return Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                "Unable to create vsomeip application",
            ));
        };
        application_wrapper.get_pinned().init();
        (*application_wrapper).register_state_handler_fn_ptr_safe(available_state_handler_fn_ptr);

        let app_name_start = app_name.to_string();
        thread::spawn(move || {
            let_cxx_string!(app_name_cxx = app_name_start.clone());
            let runtime_wrapper = make_runtime_wrapper(vsomeip::runtime::get());

            trace!(
                "{}:{} - Within start_app, spawned dedicated thread to park app",
                UP_CLIENT_VSOMEIP_TAG,
                UP_CLIENT_VSOMEIP_FN_TAG_START_APP
            );

            let Some(application_wrapper) = make_application_wrapper(
                runtime_wrapper.get_pinned().get_application(&app_name_cxx),
            ) else {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("No app found for app_name: {}", app_name_start),
                ));
            };

            let client_id = application_wrapper.get_pinned().get_client();
            trace!("start_app: after init'ing app we see its client_id: {client_id}");
            application_wrapper.get_pinned().start();

            Ok(())
        });

        trace!(
            "{}:{} - Made it past creating app and starting it on dedicated thread",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_START_APP
        );

        match receiver.recv_timeout(Duration::from_millis(50)) {
            Err(err) => {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Timed out on waiting for application to start: {err:?}"),
                ));
            }
            Ok(app_state) => {
                info!("app_state: {app_state:?}");

                application_wrapper.get_pinned().unregister_state_handler();
                application_state_availability_handler_registry
                    .free_application_state_availability_handler_id(state_handler_id)?;
            }
        }

        Ok(())
    }

    async fn event_loop(
        ue_id: UeId,
        mut rx_to_event_loop: Receiver<TransportCommand>,
        config_path: Option<PathBuf>,
    ) {
        let runtime_wrapper = make_runtime_wrapper(vsomeip::runtime::get());

        trace!("Entering command loop");
        while let Some(cmd) = rx_to_event_loop.recv().await {
            trace!("Received TransportCommand");

            match cmd {
                TransportCommand::RegisterListener(
                    src,
                    sink,
                    registration_type,
                    msg_handler,
                    app_name,
                    vsomeip_offered_requested_registry,
                    return_channel,
                ) => {
                    trace!(
                        "{}:{} - Attempting to register listener: src: {src:?} sink: {sink:?}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                    );

                    trace!("registration_type: {registration_type:?}");

                    let_cxx_string!(app_name_cxx = app_name.clone());

                    let application_wrapper = make_application_wrapper(
                        runtime_wrapper.get_pinned().get_application(&app_name_cxx),
                    );
                    let Some(mut application_wrapper) = application_wrapper else {
                        let err = Err(UStatus::fail_with_code(
                            UCode::INTERNAL,
                            format!("Application does not exist for app_name: {app_name}"),
                        ));
                        Self::return_oneshot_result(err, return_channel).await;
                        continue;
                    };

                    let res = Self::register_listener_internal(
                        src,
                        sink,
                        registration_type,
                        msg_handler,
                        vsomeip_offered_requested_registry,
                        &mut application_wrapper,
                        &runtime_wrapper,
                    )
                    .await;
                    trace!("Sending back results of registration");
                    Self::return_oneshot_result(res, return_channel).await;
                    trace!("Sent back results of registration");
                }
                TransportCommand::UnregisterListener(
                    src,
                    sink,
                    registration_type,
                    app_name,
                    return_channel,
                ) => {
                    let_cxx_string!(app_name_cxx = app_name.clone());

                    let application_wrapper = make_application_wrapper(
                        runtime_wrapper.get_pinned().get_application(&app_name_cxx),
                    );
                    let Some(application_wrapper) = application_wrapper else {
                        let err = Err(UStatus::fail_with_code(
                            UCode::INTERNAL,
                            format!("Application does not exist for app_name: {app_name}"),
                        ));
                        Self::return_oneshot_result(err, return_channel).await;
                        continue;
                    };

                    let res = Self::unregister_listener_internal(
                        src,
                        sink,
                        registration_type,
                        &application_wrapper,
                        &runtime_wrapper,
                    )
                    .await;
                    trace!("after unregister_listener_internal returns");
                    Self::return_oneshot_result(res.clone(), return_channel).await;
                    trace!("after returning oneshot result of res: {res:?}");
                }
                TransportCommand::Send(
                    umsg,
                    message_type,
                    app_name,
                    rpc_correlation_registry,
                    vsomeip_offered_requested_registry,
                    return_channel,
                ) => {
                    trace!(
                        "{}:{} - Attempting to send UMessage: {:?}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                        umsg
                    );

                    trace!(
                        "inside TransportCommand::Send dispatch, message_type: {message_type:?}"
                    );

                    let_cxx_string!(app_name_cxx = app_name.clone());

                    let application_wrapper = make_application_wrapper(
                        runtime_wrapper.get_pinned().get_application(&app_name_cxx),
                    );
                    let Some(mut application_wrapper) = application_wrapper else {
                        let err = format!(
                            "No application exists for {app_name} under client_id: {}",
                            message_type.client_id()
                        );
                        Self::return_oneshot_result(
                            Err(UStatus::fail_with_code(UCode::INTERNAL, err)),
                            return_channel,
                        )
                        .await;
                        continue;
                    };

                    let app_client_id = application_wrapper.get_pinned().get_client();
                    trace!("Application existed for {app_name}, listed under client_id: {}, with app_client_id: {app_client_id}", message_type.client_id());

                    let res = Self::send_internal(
                        umsg,
                        rpc_correlation_registry,
                        vsomeip_offered_requested_registry,
                        &mut application_wrapper,
                        &runtime_wrapper,
                    )
                    .await;
                    Self::return_oneshot_result(res, return_channel).await;
                }
                TransportCommand::StartVsomeipApp(
                    client_id,
                    app_name,
                    application_state_availability_handler_registry,
                    return_channel,
                ) => {
                    trace!(
                        "{}:{} - Attempting to initialize new app for client_id: {} app_name: {}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                        client_id,
                        app_name
                    );
                    let new_app_res = Self::start_vsomeip_app_internal(
                        client_id,
                        app_name.clone(),
                        application_state_availability_handler_registry,
                        config_path.clone(),
                    )
                    .await;
                    trace!(
                        "{}:{} - After attempt to create new app for client_id: {} app_name: {}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                        client_id,
                        app_name
                    );

                    Self::return_oneshot_result(new_app_res, return_channel).await;
                }
                TransportCommand::StopVsomeipApp(_client_id, app_name, return_channel) => {
                    let stop_res =
                        Self::stop_vsomeip_app_internal(app_name, &runtime_wrapper).await;
                    Self::return_oneshot_result(stop_res, return_channel).await;
                }
            }
            trace!("Hit bottom of event loop, ue_id: {ue_id}");
        }
    }

    async fn register_listener_internal(
        source_filter: UUri,
        sink_filter: Option<UUri>,
        registration_type: RegistrationType,
        msg_handler: MessageHandlerFnPtr,
        vsomeip_offered_requested_registry: Arc<dyn VsomeipOfferedRequestedRegistry>,
        application_wrapper: &mut UniquePtr<ApplicationWrapper>,
        _runtime_wrapper: &UniquePtr<RuntimeWrapper>,
    ) -> Result<(), UStatus> {
        trace!(
            "{}:{} - Attempting to register: source_filter: {:?} & sink_filter: {:?}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
            source_filter,
            sink_filter
        );

        match registration_type {
            RegistrationType::Publish(_) => {
                trace!(
                    "{}:{} - Registering for Publish style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );
                let (_, service_id) = split_u32_to_u16(source_filter.ue_id);
                // let instance_id = vsomeip::ANY_INSTANCE; // TODO: Set this to 1? To ANY_INSTANCE?
                let instance_id = 1;
                let (_, event_id) = split_u32_to_u16(source_filter.resource_id);

                trace!(
                    "{}:{} - register_message_handler: service: {} instance: {} method: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                    service_id,
                    instance_id,
                    event_id
                );

                if !vsomeip_offered_requested_registry.is_event_requested(
                    service_id,
                    instance_id,
                    event_id,
                ) {
                    application_wrapper.get_pinned().request_service(
                        service_id,
                        instance_id,
                        vsomeip::ANY_MAJOR,
                        vsomeip::ANY_MINOR,
                    );
                    (*application_wrapper).request_single_event_safe(
                        service_id,
                        instance_id,
                        event_id,
                        event_id,
                    );
                    application_wrapper.get_pinned().subscribe(
                        service_id,
                        instance_id,
                        event_id,
                        vsomeip::ANY_MAJOR,
                        event_id,
                    );
                    vsomeip_offered_requested_registry.insert_event_requested(
                        service_id,
                        instance_id,
                        event_id,
                    );
                }

                (*application_wrapper).register_message_handler_fn_ptr_safe(
                    service_id,
                    instance_id,
                    event_id,
                    msg_handler,
                );

                trace!(
                    "{}:{} - Registered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );

                Ok(())
            }
            RegistrationType::Request(_) => {
                trace!(
                    "{}:{} - Registering for Request style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );
                let Some(sink_filter) = sink_filter else {
                    return Err(UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Unable to map source and sink filters to uProtocol message type",
                    ));
                };

                let (_, service_id) = split_u32_to_u16(sink_filter.ue_id);
                let instance_id = 1; // TODO: Set this to 1? To ANY_INSTANCE?
                let (_, method_id) = split_u32_to_u16(sink_filter.resource_id);
                let (_, _, _, major_version) = split_u32_to_u8(sink_filter.ue_version_major);

                trace!(
                    "{}:{} - register_message_handler: service: {} instance: {} method: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                    service_id,
                    instance_id,
                    method_id
                );

                if !vsomeip_offered_requested_registry.is_service_offered(
                    service_id,
                    instance_id,
                    method_id,
                ) {
                    application_wrapper.get_pinned().offer_service(
                        service_id,
                        instance_id,
                        major_version,
                        // vsomeip::ANY_MAJOR,
                        vsomeip::DEFAULT_MINOR,
                    );
                    vsomeip_offered_requested_registry.insert_service_offered(
                        service_id,
                        instance_id,
                        method_id,
                    );
                }

                (*application_wrapper).register_message_handler_fn_ptr_safe(
                    service_id,
                    vsomeip::ANY_INSTANCE,
                    method_id,
                    msg_handler,
                );

                trace!(
                    "{}:{} - Registered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );

                Ok(())
            }
            RegistrationType::Response(_) => {
                trace!(
                    "{}:{} - Registering for Response style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );

                let (_, service_id) = split_u32_to_u16(source_filter.ue_id);
                let instance_id = vsomeip::ANY_INSTANCE; // TODO: Set this to 1? To ANY_INSTANCE?
                let (_, method_id) = split_u32_to_u16(source_filter.resource_id);

                if !vsomeip_offered_requested_registry.is_service_requested(
                    service_id,
                    instance_id,
                    method_id,
                ) {
                    application_wrapper.get_pinned().request_service(
                        service_id,
                        instance_id,
                        vsomeip::ANY_MAJOR,
                        vsomeip::ANY_MINOR,
                    );

                    vsomeip_offered_requested_registry.insert_service_requested(
                        service_id,
                        instance_id,
                        method_id,
                    );
                }

                trace!(
                    "{}:{} - register_message_handler: service: {} instance: {} method: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                    service_id,
                    instance_id,
                    method_id
                );

                (*application_wrapper).register_message_handler_fn_ptr_safe(
                    service_id,
                    instance_id,
                    method_id,
                    msg_handler,
                );

                trace!(
                    "{}:{} - Registered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );

                Ok(())
            }
            RegistrationType::AllPointToPoint(_) => Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                "Should be impossible to register point-to-point internally",
            )),
        }
    }

    async fn unregister_listener_internal(
        source_filter: UUri,
        sink_filter: Option<UUri>,
        registration_type: RegistrationType,
        application_wrapper: &UniquePtr<ApplicationWrapper>,
        _runtime_wrapper: &UniquePtr<RuntimeWrapper>,
    ) -> Result<(), UStatus> {
        trace!(
            "{}:{} - Attempting to unregister: source_filter: {:?} & sink_filter: {:?}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
            source_filter,
            sink_filter
        );

        match registration_type {
            RegistrationType::Publish(_) => {
                trace!(
                    "{}:{} - Unregistering for Publish style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
                );
                let (_, service_id) = split_u32_to_u16(source_filter.ue_id);
                let instance_id = vsomeip::ANY_INSTANCE; // TODO: Set this to 1? To ANY_INSTANCE?
                let (_, method_id) = split_u32_to_u16(source_filter.resource_id);

                application_wrapper.get_pinned().unregister_message_handler(
                    service_id,
                    instance_id,
                    method_id,
                );

                trace!(
                    "{}:{} - Unregistered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
                );
                Ok(())
            }
            RegistrationType::Request(_) => {
                trace!(
                    "{}:{} - Unregistering for Request style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
                );
                let Some(sink_filter) = sink_filter else {
                    return Err(UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Request doesn't contain sink",
                    ));
                };

                let (_, service_id) = split_u32_to_u16(sink_filter.ue_id);
                let instance_id = vsomeip::ANY_INSTANCE; // TODO: Set this to 1? To ANY_INSTANCE?
                let (_, method_id) = split_u32_to_u16(sink_filter.resource_id);

                application_wrapper.get_pinned().unregister_message_handler(
                    service_id,
                    instance_id,
                    method_id,
                );

                trace!(
                    "{}:{} - Unregistered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
                );

                Ok(())
            }
            RegistrationType::Response(_) => {
                trace!(
                    "{}:{} - Unregistering for Response style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
                );
                let Some(sink_filter) = sink_filter else {
                    return Err(UStatus::fail_with_code(
                        UCode::INVALID_ARGUMENT,
                        "Request doesn't contain sink",
                    ));
                };

                let (_, service_id) = split_u32_to_u16(sink_filter.ue_id);
                let instance_id = vsomeip::ANY_INSTANCE; // TODO: Set this to 1? To ANY_INSTANCE?
                let (_, method_id) = split_u32_to_u16(sink_filter.resource_id);

                application_wrapper.get_pinned().unregister_message_handler(
                    service_id,
                    instance_id,
                    method_id,
                );

                trace!(
                    "{}:{} - Unregistered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
                );

                Ok(())
            }
            RegistrationType::AllPointToPoint(_) => Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                "Should be impossible to unregister point-to-point internally",
            )),
        }
    }

    async fn send_internal(
        umsg: UMessage,
        rpc_correlation_registry: Arc<dyn RpcCorrelationRegistry>,
        vsomeip_offered_requested_registry: Arc<dyn VsomeipOfferedRequestedRegistry>,
        application_wrapper: &mut UniquePtr<ApplicationWrapper>,
        runtime_wrapper: &UniquePtr<RuntimeWrapper>,
    ) -> Result<(), UStatus> {
        trace!("send_internal");

        let payload = {
            if let Some(bytes) = umsg.payload.clone() {
                bytes.to_vec()
            } else {
                Vec::new()
            }
        };
        let mut vsomeip_payload =
            make_payload_wrapper(runtime_wrapper.get_pinned().create_payload());
        vsomeip_payload.set_data_safe(&payload);
        let attachable_payload = vsomeip_payload.get_shared_ptr();

        match umsg
            .attributes
            .type_
            .enum_value_or(UMessageType::UMESSAGE_TYPE_UNSPECIFIED)
        {
            UMessageType::UMESSAGE_TYPE_UNSPECIFIED => {
                return Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "Unspecified message type not supported",
                ));
            }
            UMessageType::UMESSAGE_TYPE_NOTIFICATION => {
                return Err(UStatus::fail_with_code(
                    UCode::INVALID_ARGUMENT,
                    "Notification is not supported",
                ));
            }
            UMessageType::UMESSAGE_TYPE_PUBLISH => {
                let (service_id, instance_id, event_id) =
                    UMessageToVsomeipMessage::umsg_publish_to_vsomeip_notification(
                        &umsg,
                        vsomeip_offered_requested_registry,
                        application_wrapper,
                    )
                    .await?;

                application_wrapper.get_pinned().notify(
                    service_id,
                    instance_id,
                    event_id,
                    attachable_payload,
                    true,
                );
            }
            UMessageType::UMESSAGE_TYPE_REQUEST => {
                let vsomeip_msg = UMessageToVsomeipMessage::umsg_request_to_vsomeip_message(
                    &umsg,
                    rpc_correlation_registry,
                    application_wrapper,
                    runtime_wrapper,
                )
                .await?;

                vsomeip_msg.set_message_payload(&mut vsomeip_payload);
                let shared_ptr_message = vsomeip_msg.as_ref().unwrap().get_shared_ptr();
                application_wrapper.get_pinned().send(shared_ptr_message);
            }
            UMessageType::UMESSAGE_TYPE_RESPONSE => {
                let vsomeip_msg = UMessageToVsomeipMessage::umsg_response_to_vsomeip_message(
                    &umsg,
                    rpc_correlation_registry,
                    runtime_wrapper,
                )
                .await?;

                vsomeip_msg.set_message_payload(&mut vsomeip_payload);
                let shared_ptr_message = vsomeip_msg.as_ref().unwrap().get_shared_ptr();
                application_wrapper.get_pinned().send(shared_ptr_message);
            }
        }
        Ok(())
    }

    async fn return_oneshot_result(
        result: Result<(), UStatus>,
        tx: oneshot::Sender<Result<(), UStatus>>,
    ) {
        if let Err(err) = tx.send(result) {
            error!("Unable to return oneshot result back to transport: {err:?}");
        }
    }

    async fn start_vsomeip_app_internal(
        client_id: ClientId,
        app_name: ApplicationName,
        application_state_availability_handler_registry: Arc<
            dyn ApplicationStateAvailabilityHandlerRegistry,
        >,
        config_path: Option<PathBuf>,
    ) -> Result<(), UStatus> {
        trace!(
            "{}:{} - Attempting to initialize new app for client_id: {} app_name: {}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
            client_id,
            app_name
        );

        Self::create_app(
            &app_name,
            config_path.as_deref(),
            application_state_availability_handler_registry,
        )?;
        trace!(
            "{}:{} - After starting app for client_id: {} app_name: {}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
            client_id,
            app_name
        );

        Ok(())
    }

    async fn stop_vsomeip_app_internal(
        app_name: ApplicationName,
        runtime_wrapper: &UniquePtr<RuntimeWrapper>,
    ) -> Result<(), UStatus> {
        trace!("Stopping vsomeip application for app_name: {app_name}");

        let_cxx_string!(app_name_cxx = app_name);

        let Some(app_wrapper) =
            make_application_wrapper(runtime_wrapper.get_pinned().get_application(&app_name_cxx))
        else {
            return Err::<(), UStatus>(UStatus::fail_with_code(
                UCode::INTERNAL,
                "No application exists",
            ));
        };
        app_wrapper.get_pinned().stop();

        Ok(())
    }
}
