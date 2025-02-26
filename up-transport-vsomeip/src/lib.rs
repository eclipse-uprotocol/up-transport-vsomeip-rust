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

use crate::determine_message_type::{determine_type, RegistrationType};
use crate::storage::message_handler_registry::{GetMessageHandlerError, MessageHandlerRegistry};
use crate::storage::UPTransportVsomeipStorage;
use crate::transport_engine::{TransportCommand, UPTransportVsomeipEngine};
use crate::transport_engine::{
    UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
    UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, UP_CLIENT_VSOMEIP_FN_TAG_STOP_APP,
    UP_CLIENT_VSOMEIP_TAG,
};
use crate::utils::any_uuri_fixed_authority_id;
use crate::vsomeip_config::extract_services;
use log::{error, trace, warn};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::task;
use tokio::time::timeout;
use up_rust::{ComparableListener, UCode, UListener, UStatus, UUri, UUID};
use vsomeip_config::extract_application;
pub use vsomeip_config::VsomeipApplicationConfig;

mod determine_message_type;
mod message_conversions;
mod storage;
mod transport;
mod transport_engine;
mod utils;
mod vsomeip_config;

/// A [up_rust::UUri::authority_name]
pub type AuthorityName = String;
/// A [up_rust::UUri::ue_id]
pub type UeId = u32;
/// A [vsomeip_sys::vsomeip::application]'s numeric identifier
pub type ClientId = u16;
/// A [vsomeip_sys::vsomeip::application]'s string-form identifier
pub type ApplicationName = String;

/// A [up_rust::UAttributes::reqid]
pub type UProtocolReqId = UUID;
/// A request ID used with vsomeip. See [vsomeip_sys::vsomeip::request_t]
pub type SomeIpRequestId = u32;
/// A session ID used with vsomeip. See [vsomeip_sys::vsomeip::session_t]
pub type SessionId = u16;

/// A service ID used with vsomeip. See [vsomeip_sys::vsomeip::service_t]
pub type ServiceId = u16;
/// An instance ID used with vsomeip. See [vsomeip_sys::vsomeip::instance_t]
pub type InstanceId = u16;
/// A method ID used with vsomeip. See [vsomeip_sys::vsomeip::method_t]
pub type MethodId = u16;
/// An event ID used with vsomeip. See [vsomeip_sys::vsomeip::event_t]
pub type EventId = u16;
/// Represents the id of an extern "C" fn used with which to register with vsomeip to listen for messages
type MessageHandlerId = usize;

/// Get a dedicated tokio Runtime Handle as well as the necessary infra to communicate back to the
/// thread contained internally when we would like to gracefully shut down the runtime
pub(crate) fn get_callback_runtime_handle(
    runtime_config: Option<RuntimeConfig>,
) -> (
    tokio::runtime::Handle,
    thread::JoinHandle<()>,
    std::sync::mpsc::Sender<()>,
) {
    let num_threads = {
        if let Some(runtime_config) = runtime_config {
            runtime_config.num_threads
        } else {
            DEFAULT_NUM_THREADS
        }
    };

    // Create a channel to signal when the runtime should shut down
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();
    let (handle_tx, handle_rx) = std::sync::mpsc::channel::<tokio::runtime::Handle>();

    // Spawn a new thread to run the dedicated runtime
    let thread_handle = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(num_threads as usize)
            .enable_all()
            .build()
            .expect("Unable to create runtime");

        let handle = runtime.handle();
        let handle_clone = handle.clone();
        handle_tx.send(handle_clone).expect("Unable to send handle");

        match shutdown_rx.recv() {
            Err(_) => panic!("Failed in getting shutdown signal"),
            Ok(_) => {
                // Will force shutdown after duration time if all tasks not finished sooner
                runtime.shutdown_timeout(Duration::from_millis(2000));
            }
        }
    });

    let runtime_handle = match handle_rx.recv() {
        Ok(r) => r,
        Err(_) => panic!("the sender dropped"),
    };

    (runtime_handle, thread_handle, shutdown_tx)
}

const DEFAULT_NUM_THREADS: u8 = 10;
pub struct RuntimeConfig {
    num_threads: u8,
}

/// UTransport implementation over top of the C++ vsomeip library
///
/// We hold a transport_inner internally which does the nitty-gritty
/// implementation of the transport
///
/// We do so in order to separate the "handle" to the inner transport
/// and the "engine" of the innner transport to allow mocking of them.
pub struct UPTransportVsomeip {
    storage: Arc<UPTransportVsomeipStorage>,
    engine: UPTransportVsomeipEngine,
    point_to_point_listener: RwLock<Option<Arc<dyn UListener>>>,
    config_path: Option<PathBuf>,
    thread_handle: Option<thread::JoinHandle<()>>,
    shutdown_runtime_tx: std::sync::mpsc::Sender<()>,
}

impl UPTransportVsomeip {
    /// Creates a UPTransportVsomeip based on a path provided to a vsomeip configuration JSON file
    ///
    /// # Parameters
    ///
    /// * `local_authority_name` - authority_name of the host device
    /// * `remote_authority_name` - authority_name to attach for messages originating from SOME/IP network.
    ///                             Should be set to `IP:port` of the endpoint mDevice
    /// * `ue_id` - the ue_id of the uEntity
    /// * `config_path` - path to a JSON vsomeip configuration file
    ///
    /// Further details on vsomeip configuration files can be found in the COVESA [vsomeip repo](https://github.com/COVESA/vsomeip)
    pub fn new_with_config(
        uri: UUri,
        remote_authority_name: &AuthorityName,
        config_path: &Path,
        runtime_config: Option<RuntimeConfig>,
    ) -> Result<Self, UStatus> {
        if !config_path.exists() {
            return Err(UStatus::fail_with_code(
                UCode::NOT_FOUND,
                format!("Configuration file not found at: {:?}", config_path),
            ));
        }

        let vsomeip_application_config = extract_application(config_path)?;

        Self::new_internal(
            vsomeip_application_config,
            uri,
            remote_authority_name,
            Some(config_path),
            runtime_config,
        )
    }

    /// Creates a UPTransportVsomeip
    ///
    /// # Parameters
    ///
    /// * `local_authority_name` - authority_name of the host device
    /// * `remote_authority_name` - authority_name to attach for messages originating from SOME/IP network
    ///                             Should be set to `IP:port` of the endpoint mDevice
    /// * `ue_id` - the ue_id of the uEntity
    pub fn new(
        vsomeip_application_config: VsomeipApplicationConfig,
        uri: UUri,
        remote_authority_name: &AuthorityName,
        runtime_config: Option<RuntimeConfig>,
    ) -> Result<Self, UStatus> {
        Self::new_internal(
            vsomeip_application_config,
            uri,
            remote_authority_name,
            None,
            runtime_config,
        )
    }

    /// Creates a UPTransportVsomeip whether a vsomeip config file was provided or not
    fn new_internal(
        vsomeip_application_config: VsomeipApplicationConfig,
        uri: UUri,
        remote_authority_name: &AuthorityName,
        config_path: Option<&Path>,
        runtime_config: Option<RuntimeConfig>,
    ) -> Result<Self, UStatus> {
        let check_ue_version_major: Result<u8, _> = uri.ue_version_major.try_into();
        if check_ue_version_major.is_err() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!(
                    "uri's ue_version_major doesn't fit allotted 8 bits: uri.ue_version_major: {}",
                    uri.ue_version_major
                ),
            ));
        }

        uri.verify_rpc_response().map_err(|e| {
            UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                format!("uri provided to transport is incorrect: {e:?}"),
            )
        })?;

        let (runtime_handle, thread_handle, shutdown_runtime_tx) =
            get_callback_runtime_handle(runtime_config);

        let storage = Arc::new(UPTransportVsomeipStorage::new(
            vsomeip_application_config,
            uri.clone(),
            remote_authority_name.clone(),
            runtime_handle.clone(),
        ));

        let engine = UPTransportVsomeipEngine::new(uri, None);
        let point_to_point_listener = RwLock::new(None);
        let optional_config_path: Option<PathBuf> = config_path.map(|p| p.to_path_buf());

        let me = Self {
            storage: storage.clone(),
            engine,
            point_to_point_listener,
            config_path: optional_config_path,
            thread_handle: Some(thread_handle),
            shutdown_runtime_tx,
        };

        me.initialize_vsomeip_app(storage.get_vsomeip_application_config().name)?;

        Ok(me)
    }
    async fn await_engine(
        function_id: &str,
        rx: oneshot::Receiver<Result<(), UStatus>>,
    ) -> Result<(), UStatus> {
        match timeout(Duration::from_secs(crate::transport_engine::INTERNAL_FUNCTION_TIMEOUT), rx).await {
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
                    function_id, crate::transport_engine::INTERNAL_FUNCTION_TIMEOUT
                ),
            )),
        }
    }

    async fn send_to_engine_with_status(
        tx: &Sender<TransportCommand>,
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

    fn unregister_listener(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        listener: Arc<dyn UListener>,
    ) -> Result<(), UStatus> {
        let src = source_filter.clone();
        let sink = sink_filter.cloned();

        let registration_type_res = determine_type(source_filter, &sink_filter.cloned());
        let Ok(registration_type) = registration_type_res else {
            return Err(UStatus::fail_with_code(UCode::INVALID_ARGUMENT, "Invalid source and sink filters for registerable types: Publish, Request, Response, AllPointToPoint"));
        };

        if registration_type == RegistrationType::AllPointToPoint {
            return self.unregister_point_to_point_listener();
        }

        let app_name = self.storage.get_vsomeip_application_config().name;

        let (tx, rx) = oneshot::channel();
        trace!("attempting to block_on");

        // Using block_in_place to perform async operation in sync context
        let send_to_engine_res = task::block_in_place(|| {
            self.storage
                .get_runtime_handle()
                .block_on(Self::send_to_engine_with_status(
                    &self.engine.transport_command_sender,
                    TransportCommand::UnregisterListener(
                        src,
                        sink,
                        registration_type.clone(),
                        app_name,
                        tx,
                    ),
                ))
        });

        trace!("after attempting to block_on");
        if let Err(err) = send_to_engine_res {
            panic!("engine has stopped! unable to proceed! with err: {err:?}");
        }
        let await_engine_res = task::block_in_place(|| {
            self.storage
                .get_runtime_handle()
                .block_on(Self::await_engine("unregister", rx))
        });
        if let Err(warn) = await_engine_res {
            warn!("{warn}");
        }

        let comp_listener = ComparableListener::new(listener);
        let listener_config = (source_filter.clone(), sink_filter.cloned(), comp_listener);
        let release_res = self.storage.release_message_handler(listener_config);

        // TODO: We should probably also remove entries from:
        //  * rpc_correlation -> send an error
        //  * vsomep_offered_requested -> unoffer / unrequest

        if let Err(e) = release_res {
            warn!("{}", e);
            return Ok(());
        };

        Ok(())
    }

    async fn register_for_returning_response_if_point_to_point_listener_and_sending_request(
        &self,
        msg_src: &UUri,
        msg_sink: Option<&UUri>,
        message_type: RegistrationType,
    ) -> Result<bool, UStatus> {
        let maybe_point_to_point_listener = {
            let point_to_point_listener = self.point_to_point_listener.read().unwrap();
            (*point_to_point_listener).as_ref().cloned()
        };

        let Some(ref point_to_point_listener) = maybe_point_to_point_listener else {
            return Ok(false);
        };

        if message_type != RegistrationType::Request {
            trace!("Sending non-Request when we have a point-to-point listener established");
            return Ok(true);
        }
        trace!("Sending a Request and we have a point-to-point listener");

        let Some(msg_sink) = msg_sink else {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "Missing sink for message",
            ));
        };

        // swap source and sink here since this is nominally representing a message and not a source
        // and sink filter
        let source_filter = msg_sink.clone();
        let sink_filter = Some(msg_src.clone());

        let listener = point_to_point_listener.clone();
        let comp_listener = ComparableListener::new(Arc::clone(&listener));
        let listener_config = (source_filter.clone(), sink_filter.clone(), comp_listener);
        let message_type = RegistrationType::Response;
        let msg_handler_res = self
            .storage
            .get_message_handler(self.storage.clone(), listener_config);

        let msg_handler = {
            match msg_handler_res {
                Ok(msg_handler) => msg_handler,
                Err(e) => match e {
                    GetMessageHandlerError::ListenerConfigAlreadyExists(_msg_handler) => {
                        return Ok(true);
                    }
                    GetMessageHandlerError::ListenerIdAlreadyExists(listener_id) => {
                        return Err(UStatus::fail_with_code(
                            UCode::INTERNAL,
                            format!("listener_id already exists: {listener_id}"),
                        ));
                    }
                    GetMessageHandlerError::OtherError(s) => {
                        return Err(UStatus::fail_with_code(UCode::INTERNAL, s));
                    }
                },
            }
        };

        let app_name = self.storage.get_vsomeip_application_config().name;

        let (tx, rx) = oneshot::channel();
        let send_to_engine_res = Self::send_to_engine_with_status(
            &self.engine.transport_command_sender,
            TransportCommand::RegisterListener(
                source_filter.clone(),
                sink_filter.clone(),
                message_type,
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
        let await_res = Self::await_engine("register", rx).await;
        if let Err(err) = await_res {
            panic!("Unable to register: {err:?}");
        }

        trace!("Registered returning response listener for source_filter: {source_filter:?} sink_filter: {sink_filter:?}");

        Ok(true)
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

        let service_configs = extract_services(config_path)?;
        trace!("Got vsomeip service_configs: {service_configs:?}");

        {
            let mut point_to_point_listener = self.point_to_point_listener.write().unwrap();
            if point_to_point_listener.is_some() {
                return Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "We already have a point-to-point UListener registered",
                ));
            }
            *point_to_point_listener = Some(listener.clone());
            trace!("We found a point-to-point listener and set it");
        }

        for service_config in service_configs {
            let registration_type = RegistrationType::Request;
            let service_config = Arc::new(service_config);
            let ue_id = ((service_config.instance as u32) << 16) | service_config.service as u32;

            let comp_listener = ComparableListener::new(listener.clone());
            let source_filter = UUri::any();
            let sink_filter =
                any_uuri_fixed_authority_id(&self.storage.get_local_authority(), ue_id);
            let listener_config = (
                source_filter.clone(),
                Some(sink_filter.clone()),
                comp_listener,
            );
            let Ok(msg_handler) = self
                .storage
                .get_message_handler(self.storage.clone(), listener_config)
            else {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    "Unable to get message handler for register_point_to_point_listener",
                ));
            };

            let (tx, rx) = oneshot::channel();
            let send_to_engine_res = Self::send_to_engine_with_status(
                &self.engine.transport_command_sender,
                TransportCommand::RegisterListener(
                    source_filter.clone(),
                    Some(sink_filter.clone()),
                    registration_type.clone(),
                    msg_handler,
                    self.storage.get_vsomeip_application_config().name,
                    self.storage.clone(),
                    tx,
                ),
            )
            .await;

            if let Err(err) = send_to_engine_res {
                panic!("engine has stopped! unable to proceed! with err: {err:?}");
            }
            let internal_res =
                Self::await_engine(UP_CLIENT_VSOMEIP_FN_TAG_REGISTER_LISTENER_INTERNAL, rx).await;
            if let Err(err) = internal_res {
                return Err(UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to register point to point listener: err: {err:?}"),
                ));
            }
        }

        Ok(())
    }

    fn unregister_point_to_point_listener(&self) -> Result<(), UStatus> {
        let ptp_comp_listener = {
            let point_to_point_listener = self.point_to_point_listener.read().unwrap();
            let Some(ref point_to_point_listener) = *point_to_point_listener else {
                return Err(UStatus::fail_with_code(
                    UCode::ALREADY_EXISTS,
                    "No point-to-point listener found, we can't unregister it",
                ));
            };
            ComparableListener::new(point_to_point_listener.clone())
        };

        let Some(config_path) = &self.config_path else {
            let err_msg = "No path to a vsomeip config file was provided";
            error!("{err_msg}");
            return Err(UStatus::fail_with_code(UCode::NOT_FOUND, err_msg));
        };

        let service_configs = extract_services(config_path)?;
        trace!("Got vsomeip service_configs: {service_configs:?}");

        for service_config in &service_configs {
            let ue_id = ((service_config.instance as u32) << 16) | service_config.service as u32;
            let source_filter = UUri::any();
            let sink_filter =
                any_uuri_fixed_authority_id(&self.storage.get_local_authority(), ue_id);

            let registration_type = {
                let reg_type_res =
                    determine_type(&source_filter.clone(), &Some(sink_filter.clone()));
                match reg_type_res {
                    Ok(registration_type) => registration_type,
                    Err(warn) => {
                        // we must still attempt to unregister the rest
                        warn!("{warn}");
                        continue;
                    }
                }
            };

            let app_name = self.storage.get_vsomeip_application_config().name;

            let (tx, rx) = oneshot::channel();

            let send_to_engine_res = task::block_in_place(|| {
                self.storage
                    .get_runtime_handle()
                    .block_on(Self::send_to_engine_with_status(
                        &self.engine.transport_command_sender,
                        TransportCommand::UnregisterListener(
                            source_filter.clone(),
                            Some(sink_filter.clone()),
                            registration_type,
                            app_name,
                            tx,
                        ),
                    ))
            });
            if let Err(err) = send_to_engine_res {
                panic!("engine has stopped! unable to proceed! with err: {err:?}");
            }
            let await_engine_res = task::block_in_place(|| {
                self.storage
                    .get_runtime_handle()
                    .block_on(Self::await_engine("unregister", rx))
            });
            if let Err(warn) = await_engine_res {
                warn!("{warn}");
                continue;
            }

            // TODO: We should probably also remove entries from:
            //  * rpc_correlation -> send an error
            //  * vsomep_offered_requested -> unoffer / unrequest

            let listener_config = (source_filter, Some(sink_filter), ptp_comp_listener.clone());
            let release_res = self.storage.release_message_handler(listener_config);

            // TODO: We should probably also remove entries from:
            //  * rpc_correlation -> send an error
            //  * vsomep_offered_requested -> unoffer / unrequest

            if let Err(e) = release_res {
                warn!("{}", e);
                continue;
            };
        }

        Ok(())
    }

    fn initialize_vsomeip_app(
        &self,
        app_name: ApplicationName,
    ) -> Result<ApplicationName, UStatus> {
        let (tx, rx) = oneshot::channel();
        trace!(
            "{}:{} - Sending TransportCommand for InitializeNewApp",
            UP_CLIENT_VSOMEIP_TAG,
            UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL,
        );
        let client_id = self.storage.get_vsomeip_application_config().id;
        let send_to_engine = Self::send_to_engine_with_status(
            &self.engine.transport_command_sender,
            TransportCommand::StartVsomeipApp(
                client_id,
                app_name.clone(),
                self.storage.clone(),
                tx,
            ),
        );

        let send_to_engine_res =
            task::block_in_place(|| self.storage.get_runtime_handle().block_on(send_to_engine));

        if let Err(err) = send_to_engine_res {
            panic!("engine has stopped! unable to proceed! with err: {err:?}");
        }

        let internal = Self::await_engine(UP_CLIENT_VSOMEIP_FN_TAG_INITIALIZE_NEW_APP_INTERNAL, rx);
        let internal_res =
            task::block_in_place(|| self.storage.get_runtime_handle().block_on(internal));
        if let Err(err) = internal_res {
            return Err(UStatus::fail_with_code(
                UCode::INTERNAL,
                format!("Unable to start app for app_name: {app_name}, err: {err:?}"),
            ));
        }

        Ok(app_name)
    }

    fn shutdown_vsomeip_app(&self) -> Result<(), UStatus> {
        let app_name = self.storage.get_vsomeip_application_config().name;

        let (tx, rx) = oneshot::channel();
        let send_to_engine = Self::send_to_engine_with_status(
            &self.engine.transport_command_sender,
            TransportCommand::StopVsomeipApp(app_name, tx),
        );
        let send_to_engine_res =
            task::block_in_place(|| self.storage.get_runtime_handle().block_on(send_to_engine));

        if let Err(err) = send_to_engine_res {
            panic!("engine has stopped! unable to proceed! with err: {err:?}");
        }
        let internal = Self::await_engine(UP_CLIENT_VSOMEIP_FN_TAG_STOP_APP, rx);
        task::block_in_place(|| self.storage.get_runtime_handle().block_on(internal))
    }
}

impl Drop for UPTransportVsomeip {
    fn drop(&mut self) {
        trace!("Running Drop for UPTransportVsomeip");

        let ue_id = self.storage.get_ue_id();
        trace!("Running Drop for UPTransportVsomeipInnerHandle, ue_id: {ue_id}");

        let storage = self.storage.clone();
        let all_listener_configs = storage.get_all_listener_configs();
        for listener_config in all_listener_configs {
            let (src_filter, sink_filter, comp_listener) = listener_config;
            let listener = comp_listener.into_inner();
            trace!(
                "attempting to unregister: src_filter: {src_filter:?} sink_filter: {sink_filter:?}"
            );
            let unreg_res = self.unregister_listener(&src_filter, sink_filter.as_ref(), listener);
            if let Err(warn) = unreg_res {
                warn!("{warn}");
            }
        }

        trace!("Finished running Drop for ue_id: {ue_id}");

        if let Err(err) = self.shutdown_vsomeip_app() {
            error!("Unable to shut down vsomeip application gracefully. err: {err:?}");
        }

        trace!("Signaling shutdown of runtime");
        // Signal the dedicated runtime to shut down
        self.shutdown_runtime_tx
            .send(())
            .expect("Unable to send command to shutdown runtime");

        // Wait for the dedicated runtime thread to finish
        if let Some(handle) = self.thread_handle.take() {
            handle.join().expect("Thread panicked");
        }

        trace!("Finished Drop for UPTransportVSomeip");
    }
}
