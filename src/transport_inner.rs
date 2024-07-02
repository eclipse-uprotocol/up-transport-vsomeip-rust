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

use crate::listener_registry::{add_client_id_app_name, find_app_name};
use crate::message_conversions::convert_umsg_to_vsomeip_msg_and_send;
use crate::vsomeip_offered_requested::{
    insert_event_requested, insert_service_offered, insert_service_requested, is_event_requested,
    is_service_offered, is_service_requested,
};
use crate::{split_u32_to_u16, ApplicationName, ClientId, RegistrationType};
use cxx::{let_cxx_string, UniquePtr};
use log::{error, info, trace};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use up_rust::{UCode, UMessage, UMessageType, UStatus, UUri};
use vsomeip_sys::extern_callback_wrappers::MessageHandlerFnPtr;
use vsomeip_sys::glue::{
    make_application_wrapper, make_runtime_wrapper, ApplicationWrapper, RuntimeWrapper,
};
use vsomeip_sys::safe_glue::{
    get_pinned_application, get_pinned_runtime, register_message_handler_fn_ptr_safe,
    request_single_event_safe,
};
use vsomeip_sys::vsomeip;
use vsomeip_sys::vsomeip::{ANY_MAJOR, ANY_MINOR};

pub const UP_CLIENT_VSOMEIP_TAG: &str = "UPClientVsomeipInner";
pub const UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP: &str = "app_event_loop";
pub const UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL: &str = "register_listener_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL: &str =
    "unregister_listener_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL: &str = "send_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL: &str =
    "initialize_new_app_internal";
pub const UP_CLIENT_VSOMEIP_FN_TAG_START_APP: &str = "start_app";

pub enum TransportCommand {
    // Primary purpose of a UTransport
    RegisterListener(
        UUri,
        Option<UUri>,
        RegistrationType,
        MessageHandlerFnPtr,
        oneshot::Sender<Result<(), UStatus>>,
    ),
    UnregisterListener(
        UUri,
        Option<UUri>,
        RegistrationType,
        oneshot::Sender<Result<(), UStatus>>,
    ),
    Send(
        UMessage,
        RegistrationType,
        oneshot::Sender<Result<(), UStatus>>,
    ),
    // Additional helpful commands
    InitializeNewApp(
        ClientId,
        ApplicationName,
        oneshot::Sender<Result<(), UStatus>>,
    ),
}

pub(crate) struct UPTransportVsomeipInner {
    pub(crate) transport_command_sender: Sender<TransportCommand>,
}

impl UPTransportVsomeipInner {
    pub fn new(config_path: Option<&Path>) -> Self {
        let (tx, rx) = channel(10000);

        Self::start_event_loop(rx, config_path);

        Self {
            transport_command_sender: tx,
        }
    }

    fn start_event_loop(rx_to_event_loop: Receiver<TransportCommand>, config_path: Option<&Path>) {
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
                Self::event_loop(rx_to_event_loop, config_path).await;
                info!("Broke out of loop! You probably dropped the UPClientVsomeip");
            });
            trace!("Parking dedicated thread");
            thread::park();
            trace!("Made past dedicated thread park");
        });
        trace!("Past thread spawn");
    }

    fn create_app(app_name: &ApplicationName, config_path: Option<&Path>) -> Result<(), UStatus> {
        let app_name = app_name.to_string();
        let config_path = config_path.map(|p| p.to_path_buf());

        thread::spawn(move || {
            trace!(
                "{}:{} - Within start_app, spawned dedicated thread to park app",
                UP_CLIENT_VSOMEIP_TAG,
                UP_CLIENT_VSOMEIP_FN_TAG_START_APP
            );
            let config_path = config_path.map(|p| p.to_path_buf());
            let runtime_wrapper = make_runtime_wrapper(vsomeip::runtime::get());
            let_cxx_string!(app_name_cxx = app_name);
            let application_wrapper = {
                if let Some(config_path) = config_path {
                    let config_path_str = config_path.display().to_string();
                    let_cxx_string!(config_path_cxx_str = config_path_str);
                    make_application_wrapper(
                        get_pinned_runtime(&runtime_wrapper)
                            .create_application1(&app_name_cxx, &config_path_cxx_str),
                    )
                } else {
                    make_application_wrapper(
                        get_pinned_runtime(&runtime_wrapper).create_application(&app_name_cxx),
                    )
                }
            };
            if application_wrapper.get_mut().is_null() {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "Unable to create vsomeip application",
                ));
            }

            get_pinned_application(&application_wrapper).init();
            let client_id = get_pinned_application(&application_wrapper).get_client();
            trace!("start_app: after starting app we see its client_id: {client_id}");
            // FYI: thread is blocked by vsomeip here
            get_pinned_application(&application_wrapper).start();

            Ok(())
        });

        trace!(
            "{}:{} - Made it past creating app and starting it on dedicated thread",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_START_APP
        );

        // TODO: Should be removed in favor of a signal-based strategy
        thread::sleep(Duration::from_millis(50));

        Ok(())
    }

    async fn event_loop(
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
                    return_channel,
                ) => {
                    trace!(
                        "{}:{} - Attempting to register listener: src: {src:?} sink: {sink:?}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                    );

                    trace!("registration_type: {registration_type:?}");

                    let app_name = find_app_name(registration_type.client_id()).await;

                    let Ok(app_name) = app_name else {
                        Self::return_oneshot_result(Err(app_name.err().unwrap()), return_channel)
                            .await;
                        continue;
                    };

                    let_cxx_string!(app_name_cxx = app_name);

                    let mut application_wrapper = make_application_wrapper(
                        get_pinned_runtime(&runtime_wrapper).get_application(&app_name_cxx),
                    );

                    let res = Self::register_listener_internal(
                        src,
                        sink,
                        registration_type,
                        msg_handler,
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
                    return_channel,
                ) => {
                    let app_name = find_app_name(registration_type.client_id()).await;

                    let Ok(app_name) = app_name else {
                        Self::return_oneshot_result(Err(app_name.err().unwrap()), return_channel)
                            .await;
                        continue;
                    };

                    let_cxx_string!(app_name_cxx = app_name);

                    let application_wrapper = make_application_wrapper(
                        get_pinned_runtime(&runtime_wrapper).get_application(&app_name_cxx),
                    );

                    let res = Self::unregister_listener_internal(
                        src,
                        sink,
                        registration_type,
                        &application_wrapper,
                        &runtime_wrapper,
                    )
                    .await;
                    Self::return_oneshot_result(res, return_channel).await;
                }
                TransportCommand::Send(umsg, message_type, return_channel) => {
                    trace!(
                        "{}:{} - Attempting to send UMessage: {:?}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                        umsg
                    );

                    trace!(
                        "inside TransportCommand::Send dispatch, message_type: {message_type:?}"
                    );

                    let app_name = find_app_name(message_type.client_id()).await;

                    let Ok(app_name) = app_name else {
                        Self::return_oneshot_result(Err(app_name.err().unwrap()), return_channel)
                            .await;
                        continue;
                    };

                    let_cxx_string!(app_name_cxx = app_name.clone());

                    let application =
                        get_pinned_runtime(&runtime_wrapper).get_application(&app_name_cxx);
                    if application.is_null() {
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
                    }
                    let mut application_wrapper = make_application_wrapper(application);
                    let app_client_id = get_pinned_application(&application_wrapper).get_client();
                    trace!("Application existed for {app_name}, listed under client_id: {}, with app_client_id: {app_client_id}", message_type.client_id());

                    let res =
                        Self::send_internal(umsg, &mut application_wrapper, &runtime_wrapper).await;
                    Self::return_oneshot_result(res, return_channel).await;
                }
                TransportCommand::InitializeNewApp(client_id, app_name, return_channel) => {
                    trace!(
                        "{}:{} - Attempting to initialize new app for client_id: {} app_name: {}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                        client_id,
                        app_name
                    );
                    let new_app_res = Self::initialize_new_app_internal(
                        client_id,
                        app_name.clone(),
                        config_path.clone(),
                        &runtime_wrapper,
                    )
                    .await;
                    trace!(
                        "{}:{} - After attempt to create new app for client_id: {} app_name: {}",
                        UP_CLIENT_VSOMEIP_TAG,
                        UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP,
                        client_id,
                        app_name
                    );

                    if let Err(err) = new_app_res {
                        error!("Unable to create new app: {:?}", err);
                        Self::return_oneshot_result(Err(err), return_channel).await;
                        continue;
                    }

                    let add_res = add_client_id_app_name(client_id, &app_name).await;
                    Self::return_oneshot_result(add_res, return_channel).await;
                }
            }
            trace!("Hit bottom of event loop");
        }
    }

    async fn register_listener_internal(
        source_filter: UUri,
        sink_filter: Option<UUri>,
        registration_type: RegistrationType,
        msg_handler: MessageHandlerFnPtr,
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

                if !is_event_requested(service_id, instance_id, event_id).await {
                    get_pinned_application(application_wrapper).request_service(
                        service_id,
                        instance_id,
                        ANY_MAJOR,
                        vsomeip::ANY_MINOR,
                    );
                    request_single_event_safe(
                        application_wrapper,
                        service_id,
                        instance_id,
                        event_id,
                        event_id,
                    );
                    get_pinned_application(application_wrapper).subscribe(
                        service_id,
                        instance_id,
                        event_id,
                        ANY_MAJOR,
                        event_id,
                    );
                    insert_event_requested(service_id, instance_id, event_id).await;
                }

                register_message_handler_fn_ptr_safe(
                    application_wrapper,
                    service_id,
                    instance_id,
                    event_id,
                    msg_handler,
                );

                // Letting vsomeip settle
                tokio::time::sleep(Duration::from_millis(5)).await;

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

                trace!(
                    "{}:{} - register_message_handler: service: {} instance: {} method: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                    service_id,
                    instance_id,
                    method_id
                );

                if !is_service_offered(service_id, instance_id, method_id).await {
                    get_pinned_application(application_wrapper).offer_service(
                        service_id,
                        instance_id,
                        ANY_MAJOR,
                        ANY_MINOR,
                    );
                    insert_service_offered(service_id, instance_id, method_id).await;
                }

                register_message_handler_fn_ptr_safe(
                    application_wrapper,
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

                if !is_service_requested(service_id, instance_id, method_id).await {
                    get_pinned_application(application_wrapper).request_service(
                        service_id,
                        instance_id,
                        ANY_MAJOR,
                        ANY_MINOR,
                    );
                    insert_service_requested(service_id, instance_id, method_id).await;
                }

                trace!(
                    "{}:{} - register_message_handler: service: {} instance: {} method: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                    service_id,
                    instance_id,
                    method_id
                );

                register_message_handler_fn_ptr_safe(
                    application_wrapper,
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
            UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
            source_filter,
            sink_filter
        );

        match registration_type {
            RegistrationType::Publish(_) => {
                trace!(
                    "{}:{} - Unregistering for Publish style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );
                let (_, service_id) = split_u32_to_u16(source_filter.ue_id);
                let instance_id = vsomeip::ANY_INSTANCE; // TODO: Set this to 1? To ANY_INSTANCE?
                let (_, method_id) = split_u32_to_u16(source_filter.resource_id);

                get_pinned_application(application_wrapper).unregister_message_handler(
                    service_id,
                    instance_id,
                    method_id,
                );

                trace!(
                    "{}:{} - Unregistered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );
                Ok(())
            }
            RegistrationType::Request(_) => {
                trace!(
                    "{}:{} - Unregistering for Request style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
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

                get_pinned_application(application_wrapper).unregister_message_handler(
                    service_id,
                    instance_id,
                    method_id,
                );

                trace!(
                    "{}:{} - Unregistered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
                );

                Ok(())
            }
            RegistrationType::Response(_) => {
                trace!(
                    "{}:{} - Unregistering for Response style messages.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
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

                get_pinned_application(application_wrapper).unregister_message_handler(
                    service_id,
                    instance_id,
                    method_id,
                );

                trace!(
                    "{}:{} - Unregistered vsomeip message handler.",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL,
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
        application_wrapper: &mut UniquePtr<ApplicationWrapper>,
        runtime_wrapper: &UniquePtr<RuntimeWrapper>,
    ) -> Result<(), UStatus> {
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
                let _vsomeip_msg_res = convert_umsg_to_vsomeip_msg_and_send(
                    &umsg,
                    application_wrapper,
                    runtime_wrapper,
                )
                .await;
            }
            UMessageType::UMESSAGE_TYPE_REQUEST | UMessageType::UMESSAGE_TYPE_RESPONSE => {
                let _vsomeip_msg_res = convert_umsg_to_vsomeip_msg_and_send(
                    &umsg,
                    application_wrapper,
                    runtime_wrapper,
                )
                .await;
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

    async fn initialize_new_app_internal(
        client_id: ClientId,
        app_name: ApplicationName,
        config_path: Option<PathBuf>,
        runtime_wrapper: &UniquePtr<RuntimeWrapper>,
    ) -> Result<UniquePtr<ApplicationWrapper>, UStatus> {
        trace!(
            "{}:{} - Attempting to initialize new app for client_id: {} app_name: {}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
            client_id,
            app_name
        );
        let found_app_res = find_app_name(client_id).await;

        if found_app_res.is_ok() {
            let err = UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                format!(
                    "Application already exists for client_id: {} app_name: {}",
                    client_id, app_name
                ),
            );
            return Err(err);
        }

        Self::create_app(&app_name, config_path.as_deref())?;
        trace!(
            "{}:{} - After starting app for client_id: {} app_name: {}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
            client_id,
            app_name
        );

        let_cxx_string!(app_name_cxx = app_name);

        let app_wrapper = make_application_wrapper(
            get_pinned_runtime(runtime_wrapper).get_application(&app_name_cxx),
        );
        Ok(app_wrapper)
    }
}

// TODO: Add unit tests
