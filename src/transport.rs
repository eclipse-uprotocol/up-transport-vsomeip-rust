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

use crate::determinations::{
    any_uuri, any_uuri_fixed_authority_id, determine_message_type, determine_registration_type,
    find_app_name, find_available_listener_id, free_listener_id, insert_into_listener_id_map,
};
use crate::message_conversions::convert_vsomeip_msg_to_umsg;
use crate::vsomeip_config::extract_applications;
use crate::{
    ApplicationName, ClientId, ReqId, RequestId, SessionId,
    UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL, UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL,
    UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL,
};
use crate::{RegistrationType, UPTransportVsomeip};
use crate::{
    TransportCommand, UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, UP_CLIENT_VSOMEIP_TAG,
};
use async_trait::async_trait;
use cxx::{let_cxx_string, SharedPtr};
use lazy_static::lazy_static;
use log::{error, trace, warn};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::task::LocalSet;
use tokio::time::{timeout, Instant};
use up_rust::{ComparableListener, UCode, UListener, UMessage, UStatus, UTransport, UUri};
use vsomeip_proc_macro::generate_message_handler_extern_c_fns;
use vsomeip_sys::extern_callback_wrappers::MessageHandlerFnPtr;
use vsomeip_sys::glue::{make_application_wrapper, make_message_wrapper, make_runtime_wrapper};
use vsomeip_sys::safe_glue::get_pinned_runtime;
use vsomeip_sys::vsomeip;

const UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER: &str = "register_listener";

const INTERNAL_FUNCTION_TIMEOUT: u64 = 3;

static RUNTIME: Lazy<Arc<Runtime>> =
    Lazy::new(|| Arc::new(Runtime::new().expect("Failed to create Tokio runtime")));

fn get_runtime() -> Arc<Runtime> {
    Arc::clone(&RUNTIME)
}

const THREAD_NUM: usize = 10;

// Create a separate tokio Runtime for running the callback
lazy_static! {
    static ref CB_RUNTIME: Runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(THREAD_NUM)
        .enable_all()
        .build()
        .expect("Unable to create callback runtime");
}

lazy_static! {
    pub(crate) static ref AUTHORITY_NAME: Mutex<String> = Mutex::new(String::new());
    pub(crate) static ref LISTENER_CLIENT_ID_MAPPING: RwLock<HashMap<usize, ClientId>> =
        RwLock::new(HashMap::new());
    pub(crate) static ref CLIENT_ID_APP_MAPPING: RwLock<HashMap<ClientId, String>> =
        RwLock::new(HashMap::new());
    pub(crate) static ref UE_REQUEST_CORRELATION: RwLock<HashMap<RequestId, ReqId>> =
        RwLock::new(HashMap::new());
    pub(crate) static ref ME_REQUEST_CORRELATION: RwLock<HashMap<ReqId, RequestId>> =
        RwLock::new(HashMap::new());
    pub(crate) static ref CLIENT_ID_SESSION_ID_TRACKING: RwLock<HashMap<ClientId, SessionId>> =
        RwLock::new(HashMap::new());
}

type ListenerIdMap = RwLock<HashMap<(UUri, Option<UUri>, ComparableListener), usize>>;

lazy_static! {
    pub(crate) static ref LISTENER_REGISTRY: RwLock<HashMap<usize, Arc<dyn UListener>>> =
        RwLock::new(HashMap::new());
    pub(crate) static ref LISTENER_ID_MAP: ListenerIdMap = RwLock::new(HashMap::new());
}

generate_message_handler_extern_c_fns!(10000);

async fn await_internal_function(
    function_id: &str,
    rx: oneshot::Receiver<Result<(), UStatus>>,
) -> Result<(), UStatus> {
    match timeout(Duration::from_secs(INTERNAL_FUNCTION_TIMEOUT), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(UStatus::fail_with_code(
            UCode::INTERNAL,
            format!(
                "Unable to receive status back from internal function: {}",
                function_id
            ),
        )),
        Err(_) => Err(UStatus::fail_with_code(
            UCode::DEADLINE_EXCEEDED,
            format!(
                "Unable to receive status back from internal function: {} within {} second window.",
                function_id, INTERNAL_FUNCTION_TIMEOUT
            ),
        )),
    }
}

async fn send_to_inner_with_status(
    tx: &tokio::sync::mpsc::Sender<TransportCommand>,
    transport_command: TransportCommand,
) -> Result<(), UStatus> {
    tx.send(transport_command).await.map_err(|e| {
        UStatus::fail_with_code(
            UCode::INTERNAL,
            format!(
                "Unable to transmit request to internal vsomeip application handler, err: {:?}",
                e
            ),
        )
    })
}

#[async_trait]
impl UTransport for UPTransportVsomeip {
    async fn send(&self, message: UMessage) -> Result<(), UStatus> {
        // TODO: Add validtion on message before send, using UAttributesValidators

        trace!("Sending message: {:?}", message);

        let Some(source_filter) = message.attributes.source.as_ref() else {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "UMessage provided with no source",
            ));
        };

        let sink_filter = message.attributes.sink.as_ref();
        let message_type = determine_message_type(source_filter, &sink_filter.cloned())?;
        trace!("inside send(), message_type: {message_type:?}");
        let app_name = find_app_name(message_type.client_id()).await;
        trace!("app_name: {app_name:?}");

        self.initialize_vsomeip_app_as_needed(&message_type, app_name)
            .await?;

        let app_name = format!("{}", message_type.client_id());
        trace!("app_name: {app_name}");

        self.register_for_returning_response_if_point_to_point_listener_and_sending_request(
            &message,
            source_filter,
            sink_filter,
            message_type.clone(),
        )
        .await?;

        let (tx, rx) = oneshot::channel();
        send_to_inner_with_status(
            &self.tx_to_event_loop,
            TransportCommand::Send(message, message_type, tx),
        )
        .await?;
        await_internal_function(UP_CLIENT_VSOMEIP_FN_TAG_SEND_INTERNAL, rx).await
    }

    async fn register_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        // TODO: Must add additional validation up here on the UUri filters

        let registration_type_res =
            determine_registration_type(source_filter, &sink_filter.cloned(), self.ue_id);
        let Ok(registration_type) = registration_type_res else {
            return Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, "Invalid source and sink filters for registerable types: Publish, Request, Response, AllPointToPoint"));
        };

        trace!("registration_type: {registration_type:?}");

        if registration_type == RegistrationType::AllPointToPoint(0xFFFF) {
            return self.register_point_to_point_listener(&listener).await;
        }

        let listener_id = find_available_listener_id().await?;

        trace!("Obtained listener_id: {}", listener_id);

        let comp_listener = ComparableListener::new(listener.clone());
        let key = (source_filter.clone(), sink_filter.cloned(), comp_listener);

        if !insert_into_listener_id_map(key, listener_id).await {
            return Err(free_listener_id(listener_id).await);
        }
        trace!("Inserted into LISTENER_ID_MAP");

        Self::register_listener_id_with_listener(listener_id, listener).await?;

        let app_name = find_app_name(registration_type.client_id()).await;
        let extern_fn = get_extern_fn(listener_id);
        let msg_handler = MessageHandlerFnPtr(extern_fn);
        let src = source_filter.clone();
        let sink = sink_filter.cloned();

        trace!("Obtained extern_fn");

        self.initialize_vsomeip_app_as_needed(&registration_type, app_name)
            .await?;

        Self::map_listener_id_to_client_id(registration_type.client_id(), listener_id).await?;

        let (tx, rx) = oneshot::channel();
        send_to_inner_with_status(
            &self.tx_to_event_loop,
            TransportCommand::RegisterListener(src, sink, registration_type, msg_handler, tx),
        )
        .await?;
        await_internal_function(UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, rx).await
    }

    async fn unregister_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        let src = source_filter.clone();
        let sink = sink_filter.cloned();

        let registration_type_res =
            determine_registration_type(source_filter, &sink_filter.cloned(), self.ue_id);
        let Ok(registration_type) = registration_type_res else {
            return Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, "Invalid source and sink filters for registerable types: Publish, Request, Response, AllPointToPoint"));
        };

        if registration_type == RegistrationType::AllPointToPoint(0xFFFF) {
            return self
                .unregister_point_to_point_listener(&listener, &registration_type)
                .await;
        }

        {
            let client_id_app_mapping = CLIENT_ID_APP_MAPPING.read().await;
            if client_id_app_mapping
                .get(&registration_type.client_id())
                .is_none()
            {
                return Err(UStatus::fail_with_code(
                    UCode::NOT_FOUND,
                    format!(
                        "There was no app_name found for client_id: {}",
                        registration_type.client_id()
                    ),
                ));
            }
        }

        let (tx, rx) = oneshot::channel();
        send_to_inner_with_status(
            &self.tx_to_event_loop,
            TransportCommand::UnregisterListener(src, sink, registration_type.clone(), tx),
        )
        .await?;
        await_internal_function(UP_CLIENT_VSOMEIP_FN_TAG_UNREGISTER_LISTENER_INTERNAL, rx).await?;

        let comp_listener = ComparableListener::new(listener);
        Self::release_listener_id(source_filter, &sink_filter, &comp_listener).await?;

        Ok(())
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

impl UPTransportVsomeip {
    async fn register_for_returning_response_if_point_to_point_listener_and_sending_request(
        &self,
        message: &UMessage,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        message_type: RegistrationType,
    ) -> Result<(), UStatus> {
        let maybe_point_to_point_listener = {
            let point_to_point_listener = self.point_to_point_listener.read().await;
            (*point_to_point_listener).as_ref().cloned()
        };

        if let Some(ref point_to_point_listener) = maybe_point_to_point_listener {
            if message_type != RegistrationType::Request(message_type.client_id()) {
                trace!("Sending non-Request when we have a point-to-point listener established");
                return Ok(());
            }
            trace!("Sending a Request and we have a point-to-point listener");

            let listener_id = find_available_listener_id().await?;
            let listener = point_to_point_listener.clone();
            let comp_listener = ComparableListener::new(Arc::clone(&listener));
            let key = (source_filter.clone(), sink_filter.cloned(), comp_listener);
            if !insert_into_listener_id_map(key, listener_id).await {
                trace!("{:?}", free_listener_id(listener_id).await);
                return Ok(());
            }
            Self::register_listener_id_with_listener(listener_id, listener).await?;

            let extern_fn = get_extern_fn(listener_id);
            let msg_handler = MessageHandlerFnPtr(extern_fn);

            let Some(src) = message.attributes.sink.as_ref() else {
                let err_msg = "Request message doesn't have a sink";
                error!("{err_msg}");
                return Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, err_msg));
            };
            let Some(sink) = message.attributes.source.as_ref() else {
                let err_msg = "Request message doesn't have a source";
                error!("{err_msg}");
                return Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, err_msg));
            };

            trace!("source used when registering:\n{src:?}");
            trace!("sink used when registering:\n{sink:?}");

            Self::map_listener_id_to_client_id(message_type.client_id(), listener_id).await?;

            trace!(
                "listener_id mapped to client_id: listener_id: {listener_id} client_id: {}",
                message_type.client_id()
            );

            let message_type = RegistrationType::Response(message_type.client_id());
            let (tx, rx) = oneshot::channel();
            send_to_inner_with_status(
                &self.tx_to_event_loop,
                TransportCommand::RegisterListener(
                    src.clone(),
                    Some(sink.clone()),
                    message_type,
                    msg_handler,
                    tx,
                ),
            )
            .await?;
            await_internal_function(UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, rx)
                .await?;
        }
        Ok(())
    }

    async fn register_listener_id_with_listener(
        listener_id: usize,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        LISTENER_REGISTRY
            .write()
            .await
            .insert(listener_id, listener.clone())
            .map(|_| {
                Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "Unable to register the same listener_id and listener twice",
                ))
            })
            .unwrap_or(Ok(()))?;
        Ok(())
    }

    async fn register_point_to_point_listener(
        &self,
        listener: &Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        let Some(config_path) = &self.config_path else {
            let err_msg = "No path to a vsomeip config file was provided";
            error!("{err_msg}");
            return Err(UStatus::fail_with_code(UCode::NOT_FOUND, err_msg));
        };

        let application_configs = extract_applications(config_path)?;
        trace!("Got vsomeip application_configs: {application_configs:?}");

        {
            let mut point_to_point_listener = self.point_to_point_listener.write().await;
            if point_to_point_listener.is_some() {
                return Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "We already have a point-to-point UListener registered",
                ));
            }
            *point_to_point_listener = Some(listener.clone());
            trace!("We found a point-to-point listener and set it");
        }

        for app_config in &application_configs {
            let (tx, rx) = oneshot::channel();
            send_to_inner_with_status(
                &self.tx_to_event_loop,
                TransportCommand::InitializeNewApp(app_config.id, app_config.name.clone(), tx),
            )
            .await?;
            await_internal_function(
                &format!("Initializing point-to-point listener apps. ApplicationConfig: {:?} app_config.id: {} app_config.name: {}",
                app_config, app_config.id, app_config.name),
                rx,
            )
            .await?;

            let listener_id = find_available_listener_id().await?;
            let comp_listener = ComparableListener::new(listener.clone());

            let src = any_uuri();
            // TODO: How to explicitly handle instance_id?
            //  I'm not sure it's possible to set within the vsomeip config file
            let sink = any_uuri_fixed_authority_id(&self.authority_name, app_config.id);

            let key = (src.clone(), Some(sink.clone()), comp_listener.clone());
            if !insert_into_listener_id_map(key, listener_id).await {
                return Err(free_listener_id(listener_id).await);
            }

            Self::register_listener_id_with_listener(listener_id, listener.clone()).await?;

            Self::map_listener_id_to_client_id(app_config.id, listener_id).await?;

            let extern_fn = get_extern_fn(listener_id);
            let msg_handler = MessageHandlerFnPtr(extern_fn);

            let registration_type = RegistrationType::Request(app_config.id);

            let (tx, rx) = oneshot::channel();
            send_to_inner_with_status(
                &self.tx_to_event_loop,
                TransportCommand::RegisterListener(
                    src,
                    Some(sink),
                    registration_type,
                    msg_handler,
                    tx,
                ),
            )
            .await?;
            await_internal_function(UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, rx)
                .await?;
        }
        Ok(())
    }

    async fn unregister_point_to_point_listener(
        &self,
        listener: &Arc<dyn UListener>,
        _registration_type: &RegistrationType, // keep this for now as we may stop the application associated
    ) -> Result<(), UStatus> {
        let Some(config_path) = &self.config_path else {
            let err_msg = "No path to a vsomeip config file was provided";
            error!("{err_msg}");
            return Err(UStatus::fail_with_code(UCode::NOT_FOUND, err_msg));
        };

        let application_configs = extract_applications(config_path)?;
        trace!("Got vsomeip application_configs: {application_configs:?}");

        let ptp_comp_listener = {
            let point_to_point_listener = self.point_to_point_listener.read().await;
            let Some(ref point_to_point_listener) = *point_to_point_listener else {
                return Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "No point-to-point listener found, we can't unregister it",
                ));
            };
            ComparableListener::new(point_to_point_listener.clone())
        };
        let comp_listener = ComparableListener::new(listener.clone());
        if ptp_comp_listener != comp_listener {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "listener provided doesn't match registered point_to_point_listener",
            ));
        }

        for app_config in &application_configs {
            let src = any_uuri();
            let sink = any_uuri_fixed_authority_id(&self.authority_name, app_config.id);

            trace!("Searching for src: {src:?} sink: {sink:?} to find listener_id");

            Self::release_listener_id(&src, &Some(&sink), &comp_listener).await?;
        }
        Ok(())
    }

    async fn release_listener_id(
        source_filter: &UUri,
        sink_filter: &Option<&UUri>,
        comp_listener: &ComparableListener,
    ) -> Result<(), UStatus> {
        let listener_id = {
            let mut id_map = LISTENER_ID_MAP.write().await;
            if let Some(&id) = id_map.get(&(
                source_filter.clone(),
                sink_filter.cloned(),
                comp_listener.clone(),
            )) {
                id_map.remove(&(
                    source_filter.clone(),
                    sink_filter.cloned(),
                    comp_listener.clone(),
                ));
                id
            } else {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "Listener not found",
                ));
            }
        };

        let _client_id = {
            let mut registry = LISTENER_REGISTRY.write().await;
            registry.remove(&listener_id).ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to locate UListener for listener_id: {listener_id}"),
                )
            })?;

            let mut free_ids = FREE_LISTENER_IDS.write().await;
            free_ids.insert(listener_id).then_some(()).ok_or_else(|| {
                UStatus::fail_with_code(UCode::INTERNAL, format!("Unable to re-insert listener_id back into free listeners, listener_id: {listener_id}"))
            })?;

            let mut listener_client_id_mapping = LISTENER_CLIENT_ID_MAPPING.write().await;
            listener_client_id_mapping.remove(&listener_id).ok_or_else(|| {
                UStatus::fail_with_code(UCode::INTERNAL, format!("Unable to locate client_id (i.e. for app) for listener_id: {listener_id}"))
            })?
        };

        // TODO: If we're going to remove the client_id -> app_name mapping we should only do so if
        //  there are no other users of this client_id
        //  Would also imply that we should close down the vsomeip application
        // {
        //     let mut client_id_app_mapping = CLIENT_ID_APP_MAPPING.write().await;
        //     client_id_app_mapping.remove(&registration_type.client_id());
        // }

        Ok(())
    }

    async fn initialize_vsomeip_app_as_needed(
        &self,
        registration_type: &RegistrationType,
        app_name: Result<ApplicationName, UStatus>,
    ) -> Result<ApplicationName, UStatus> {
        {
            if let Err(err) = app_name {
                warn!(
                    "No app found for client_id: {}, err: {err:?}",
                    registration_type.client_id()
                );

                let app_name = format!("{}", registration_type.client_id());

                let (tx, rx) = oneshot::channel();
                trace!(
                    "{}:{} - Sending TransportCommand for InitializeNewApp",
                    UP_CLIENT_VSOMEIP_TAG,
                    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER
                );
                send_to_inner_with_status(
                    &self.tx_to_event_loop,
                    TransportCommand::InitializeNewApp(
                        registration_type.client_id(),
                        app_name.clone(),
                        tx,
                    ),
                )
                .await?;
                let internal_res = await_internal_function(
                    UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
                    rx,
                )
                .await;
                if let Err(err) = internal_res {
                    Err(UStatus::fail_with_code(
                        UCode::INTERNAL,
                        format!("Unable to start app for app_name: {app_name}, err: {err:?}"),
                    ))
                } else {
                    Ok(app_name)
                }
            } else {
                Ok(app_name.unwrap())
            }
        }
    }

    async fn map_listener_id_to_client_id(
        client_id: ClientId,
        listener_id: usize,
    ) -> Result<(), UStatus> {
        let mut listener_client_id_mapping = LISTENER_CLIENT_ID_MAPPING.write().await;
        listener_client_id_mapping.insert(listener_id, client_id).map(|_| Err(UStatus::fail_with_code(
            UCode::INTERNAL,
            format!("Unable to have the same listener_id with a different client_id, i.e. tied to app: listener_id: {} client_id: {}", listener_id, client_id),
        )))
            .unwrap_or(Ok(()))?;
        Ok(())
    }
}
