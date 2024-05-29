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

use cxx::{let_cxx_string, UniquePtr};
use lazy_static::lazy_static;
use log::{error, info, trace};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::{oneshot, RwLock};
use up_rust::{UCode, UListener, UMessage, UMessageType, UStatus, UUri, UUID};
use vsomeip_sys::extern_callback_wrappers::MessageHandlerFnPtr;
use vsomeip_sys::glue::{
    make_application_wrapper, make_runtime_wrapper, ApplicationWrapper, RuntimeWrapper,
};
use vsomeip_sys::safe_glue::{
    get_message_payload, get_pinned_application, get_pinned_message_base, get_pinned_runtime,
    register_message_handler_fn_ptr_safe, request_single_event_safe,
};
use vsomeip_sys::vsomeip;
use vsomeip_sys::vsomeip::{ANY_MAJOR, ANY_MINOR};

pub mod transport;
use transport::CLIENT_ID_APP_MAPPING;

mod message_conversions;
use message_conversions::convert_umsg_to_vsomeip_msg;

mod determinations;
use determinations::{
    create_request_id, find_app_name, retrieve_session_id, split_u32_to_u16, split_u32_to_u8,
};

mod vsomeip_config;

const UP_CLIENT_VSOMEIP_TAG: &str = "UPClientVsomeip";
const UP_CLIENT_VSOMEIP_FN_TAG_NEW_INTERNAL: &str = "new_internal";
const UP_CLIENT_VSOMEIP_FN_TAG_APP_EVENT_LOOP: &str = "app_event_loop";
const UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL: &str = "register_listener_internal";
const UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL: &str = "unregister_listener_internal";
const UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL: &str = "send_internal";
const UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL: &str = "initialize_new_app_internal";
const UP_CLIENT_VSOMEIP_FN_TAG_START_APP: &str = "start_app";

// TODO: Revisit what authority to attach, if any, to the remote mE. For now, just assign "me_authority"
const ME_AUTHORITY: &str = "me_authority";

lazy_static! {
    static ref OFFERED_SERVICES: RwLock<HashSet<(ServiceId, InstanceId, MethodId)>> =
        RwLock::new(HashSet::new());
    static ref REQUESTED_SERVICES: RwLock<HashSet<(ServiceId, InstanceId, MethodId)>> =
        RwLock::new(HashSet::new());
    static ref REQUESTED_EVENTS: RwLock<HashSet<(ServiceId, InstanceId, MethodId)>> =
        RwLock::new(HashSet::new());
}

enum TransportCommand {
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

type ApplicationName = String;
type AuthorityName = String;
type UeId = u16;
type ClientId = u16;
type ReqId = UUID;
type SessionId = u16;
type RequestId = u32;
type EventId = u16;
type ServiceId = u16;
type InstanceId = u16;
type MethodId = u16;

#[derive(Clone, Debug, PartialEq)]
enum RegistrationType {
    Publish(ClientId),
    Request(ClientId),
    Response(ClientId),
    AllPointToPoint(ClientId),
}

impl RegistrationType {
    pub fn client_id(&self) -> ClientId {
        match self {
            RegistrationType::Publish(client_id) => *client_id,
            RegistrationType::Request(client_id) => *client_id,
            RegistrationType::Response(client_id) => *client_id,
            RegistrationType::AllPointToPoint(client_id) => *client_id,
        }
    }
}

pub struct UPClientVsomeip {
    // we're going to be using this for error messages, so suppress this warning for now
    #[allow(dead_code)]
    authority_name: AuthorityName,
    // we're going to be using this for error messages, so suppress this warning for now
    #[allow(dead_code)]
    ue_id: UeId,
    // we're going to be using this for error messages, so suppress this warning for now
    #[allow(dead_code)]
    config_path: Option<PathBuf>,
    // if this is not None, indicates that we are in a dedicated point-to-point mode
    point_to_point_listener: RwLock<Option<Arc<dyn UListener>>>,
    tx_to_event_loop: Sender<TransportCommand>,
}

impl UPClientVsomeip {
    pub fn new_with_config(
        authority_name: &AuthorityName,
        ue_id: UeId,
        config_path: &Path,
    ) -> Result<Self, UStatus> {
        if !config_path.exists() {
            return Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                format!("Configuration file not found at: {:?}", config_path),
            ));
        }
        Self::new_internal(authority_name, ue_id, Some(config_path))
    }

    pub fn new(authority_name: &AuthorityName, ue_id: UeId) -> Result<Self, UStatus> {
        Self::new_internal(authority_name, ue_id, None)
    }

    fn new_internal(
        authority_name: &AuthorityName,
        ue_id: UeId,
        config_path: Option<&Path>,
    ) -> Result<Self, UStatus> {
        let (tx, rx) = channel(10000);

        Self::start_event_loop(rx, config_path);

        let config_path: Option<PathBuf> = config_path.map(|p| p.to_path_buf());

        trace!(
            "{}:{} - Initializing UPClientVsomeip with authority_name: {} ue_id: {}",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_NEW_INTERNAL,
            authority_name,
            ue_id
        );

        Ok(Self {
            authority_name: authority_name.to_string(),
            ue_id,
            tx_to_event_loop: tx,
            point_to_point_listener: None.into(),
            config_path,
        })
    }

    fn create_app(app_name: &ApplicationName, config_path: Option<&Path>) {
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
            // TODO: Add some additional checks to ensure we succeeded, e.g. check not null pointer
            //  This is probably best handled at a lower level in vsomeip-sys
            //  and surfacing a new API
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

            get_pinned_application(&application_wrapper).init();
            let client_id = get_pinned_application(&application_wrapper).get_client();
            trace!("start_app: after starting app we see its client_id: {client_id}");
            // FYI: thread is blocked by vsomeip here
            get_pinned_application(&application_wrapper).start();
        });

        trace!(
            "{}:{} - Made it past creating app and starting it on dedicated thread",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_START_APP
        );

        // TODO: Should be removed in favor of a signal-based strategy
        thread::sleep(Duration::from_millis(50));
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

                    let mut client_id_app_mapping = CLIENT_ID_APP_MAPPING.write().await;
                    if let std::collections::hash_map::Entry::Vacant(e) =
                        client_id_app_mapping.entry(client_id)
                    {
                        e.insert(app_name.clone());
                        trace!(
                            "Inserted client_id: {} and app_name: {} into CLIENT_ID_APP_MAPPING",
                            client_id,
                            app_name
                        );
                        Self::return_oneshot_result(Ok(()), return_channel).await;
                    } else {
                        let err_msg = format!("Already had key. Somehow we already had an application running for client_id: {}", client_id);
                        let err = UStatus::fail_with_code(UCode::ALREADY_EXISTS, err_msg);
                        Self::return_oneshot_result(Err(err), return_channel).await;
                    }
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

                {
                    let requested_events = REQUESTED_EVENTS.write().await;
                    if !requested_events.contains(&(service_id, instance_id, event_id)) {
                        // TODO: Need only do these things once per register
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
                    }
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

                {
                    let mut offered_services = OFFERED_SERVICES.write().await;
                    if !offered_services.contains(&(service_id, instance_id, method_id)) {
                        get_pinned_application(application_wrapper).offer_service(
                            service_id,
                            instance_id,
                            ANY_MAJOR,
                            ANY_MINOR,
                        );
                        offered_services.insert((service_id, instance_id, method_id));
                    }
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

                // TODO: Fix this, should not be ANY_MAJOR and ANY_MINOR
                {
                    let requested_services = REQUESTED_SERVICES.write().await;
                    if !requested_services.contains(&(service_id, instance_id, method_id)) {
                        get_pinned_application(application_wrapper).request_service(
                            service_id,
                            instance_id,
                            ANY_MAJOR,
                            ANY_MINOR,
                        );
                    }
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
                let vsomeip_msg_res =
                    convert_umsg_to_vsomeip_msg(&umsg, application_wrapper, runtime_wrapper).await;

                let Ok(mut vsomeip_msg) = vsomeip_msg_res else {
                    let err = vsomeip_msg_res.err().unwrap();
                    error!(
                        "{}:{} Converting UMessage to vsomeip message failed: {:?}",
                        UP_CLIENT_VSOMEIP_TAG, UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL, err
                    );
                    return Err(err);
                };

                let service_id = get_pinned_message_base(&vsomeip_msg).get_service();
                let instance_id = get_pinned_message_base(&vsomeip_msg).get_instance();
                let event_id = get_pinned_message_base(&vsomeip_msg).get_method();
                let _interface_version =
                    get_pinned_message_base(&vsomeip_msg).get_interface_version();

                trace!(
                    "{}:{} Sending SOME/IP NOTIFICATION with service: {} instance: {} event: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL,
                    service_id,
                    instance_id,
                    event_id
                );

                let payload = get_message_payload(&mut vsomeip_msg).get_shared_ptr();
                // TODO: Talk about with @StevenHartley. Note that we cannot set the interface_version
                get_pinned_application(application_wrapper).notify(
                    service_id,
                    instance_id,
                    event_id,
                    payload,
                    true,
                );
            }
            UMessageType::UMESSAGE_TYPE_REQUEST | UMessageType::UMESSAGE_TYPE_RESPONSE => {
                let vsomeip_msg_res =
                    convert_umsg_to_vsomeip_msg(&umsg, application_wrapper, runtime_wrapper).await;

                let Ok(vsomeip_msg) = vsomeip_msg_res else {
                    let err = vsomeip_msg_res.err().unwrap();
                    error!(
                        "{}:{} Converting UMessage to vsomeip message failed: {:?}",
                        UP_CLIENT_VSOMEIP_TAG, UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL, err
                    );
                    return Err(err);
                };
                let service_id = get_pinned_message_base(&vsomeip_msg).get_service();
                let instance_id = get_pinned_message_base(&vsomeip_msg).get_instance();
                let method_id = get_pinned_message_base(&vsomeip_msg).get_method();
                let _interface_version =
                    get_pinned_message_base(&vsomeip_msg).get_interface_version();

                trace!(
                    "{}:{} Sending SOME/IP message with service: {} instance: {} method: {}",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL,
                    service_id,
                    instance_id,
                    method_id
                );

                // TODO: Add logging here that we succeeded
                let shared_ptr_message = vsomeip_msg.as_ref().unwrap().get_shared_ptr();
                get_pinned_application(application_wrapper).send(shared_ptr_message);
            }
        }
        Ok(())
    }

    async fn return_oneshot_result(
        result: Result<(), UStatus>,
        tx: oneshot::Sender<Result<(), UStatus>>,
    ) {
        if let Err(_err) = tx.send(result) {
            // TODO: Add logging here that we couldn't return a result
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
        let existing_app = {
            let client_id_app_mapping = CLIENT_ID_APP_MAPPING.read().await;
            client_id_app_mapping.contains_key(&client_id)
        };

        if existing_app {
            let err = UStatus::fail_with_code(
                UCode::ALREADY_EXISTS,
                format!(
                    "Application already exists for client_id: {} app_name: {}",
                    client_id, app_name
                ),
            );
            return Err(err);
        }

        // TODO: Should this be fallible?
        Self::create_app(&app_name, config_path.as_deref());
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

// TODO: We need to ensure that we properly cleanup / unregister all message handlers
//  and then remote the application
// impl Drop for UPClientVsomeip {
//     fn drop(&mut self) {
//         // TODO: Should do this a bit more carefully, for now we will just stop all active vsomeip
//         //  applications
//         //  - downside of doing this drastic option is that _if_ you wanted to keep one client
//         //    active and let another be dropped, this would put your client in a bad state
//
//     }
// }
