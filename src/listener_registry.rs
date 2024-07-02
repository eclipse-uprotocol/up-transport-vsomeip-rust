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

use crate::message_conversions::convert_vsomeip_msg_to_umsg;
use crate::{ApplicationName, AuthorityName, ClientId};
use cxx::{let_cxx_string, SharedPtr};
use lazy_static::lazy_static;
use log::{error, info, trace};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{mpsc, Arc};
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tokio::task::LocalSet;
use tokio::time::Instant;
use up_rust::{ComparableListener, UListener, UUri};
use up_rust::{UCode, UMessage, UStatus};
use vsomeip_proc_macro::generate_message_handler_extern_c_fns;
use vsomeip_sys::glue::{make_application_wrapper, make_message_wrapper, make_runtime_wrapper};
use vsomeip_sys::safe_glue::get_pinned_runtime;
use vsomeip_sys::vsomeip;

const THREAD_NUM: usize = 10;

// Create a separate tokio Runtime for running the callback
lazy_static! {
    static ref CB_RUNTIME: Runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(THREAD_NUM)
        .enable_all()
        .build()
        .expect("Unable to create callback runtime");
}

static RUNTIME: Lazy<Arc<Runtime>> =
    Lazy::new(|| Arc::new(Runtime::new().expect("Failed to create Tokio runtime")));

fn get_runtime() -> Arc<Runtime> {
    Arc::clone(&RUNTIME)
}

generate_message_handler_extern_c_fns!(10000);

type ListenerIdMap = RwLock<HashMap<(UUri, Option<UUri>, ComparableListener), usize>>;
lazy_static! {
    static ref LISTENER_ID_CLIENT_ID_MAPPING: RwLock<HashMap<usize, ClientId>> =
        RwLock::new(HashMap::new());
    static ref LISTENER_ID_AUTHORITY_NAME: RwLock<HashMap<usize, AuthorityName>> =
        RwLock::new(HashMap::new());
    static ref LISTENER_ID_REMOTE_AUTHORITY_NAME: RwLock<HashMap<usize, AuthorityName>> =
        RwLock::new(HashMap::new());
    static ref LISTENER_REGISTRY: RwLock<HashMap<usize, Arc<dyn UListener>>> =
        RwLock::new(HashMap::new());
    static ref LISTENER_ID_MAP: ListenerIdMap = RwLock::new(HashMap::new());
}

pub(crate) async fn free_listener_id(listener_id: usize) -> UStatus {
    info!("listener_id was not used since we already have registered for this");
    let mut free_ids = FREE_LISTENER_IDS.write().await;
    free_ids.insert(listener_id);
    UStatus::fail_with_code(
        UCode::ALREADY_EXISTS,
        "Already have registered with this source, sink and listener",
    )
}

pub(crate) async fn insert_into_listener_id_map(
    authority_name: &AuthorityName,
    remote_authority_name: &AuthorityName,
    key: (UUri, Option<UUri>, ComparableListener),
    listener_id: usize,
) -> bool {
    trace!(
        "authority_name: {}, remote_authority_name: {}, listener_id: {}",
        authority_name,
        remote_authority_name,
        listener_id
    );

    // TODO: Should ensure that we don't record a partial transaction by rolling back any pieces which succeeded if a latter part fails

    let mut id_map = LISTENER_ID_MAP.write().await;
    if id_map.insert(key, listener_id).is_some() {
        trace!(
            "Not inserted into LISTENER_ID_MAP since we already have registered for this Request"
        );
        return false;
    } else {
        trace!("Inserted into LISTENER_ID_MAP");
    }

    let mut listener_id_authority_name = LISTENER_ID_AUTHORITY_NAME.write().await;

    trace!(
        "checking listener_id_authority_name: {:?}",
        *listener_id_authority_name
    );

    if listener_id_authority_name
        .insert(listener_id, authority_name.to_string())
        .is_some()
    {
        trace!(
            "Not inserted into LISTENER_ID_AUTHORITY_NAME since we already have registered for this Request"
        );
        return false;
    } else {
        trace!("Inserted into LISTENER_ID_AUTHORITY_NAME");
    }

    let mut listener_id_remote_authority_name = LISTENER_ID_REMOTE_AUTHORITY_NAME.write().await;
    if listener_id_remote_authority_name
        .insert(listener_id, remote_authority_name.to_string())
        .is_some()
    {
        trace!(
            "Not inserted into LISTENER_ID_REMOTE_AUTHORITY_NAME since we already have registered for this Request"
        );
        return false;
    } else {
        trace!("Inserted into LISTENER_ID_REMOTE_AUTHORITY_NAME");
    }

    true
}

pub(crate) async fn find_available_listener_id() -> Result<usize, UStatus> {
    let mut free_ids = FREE_LISTENER_IDS.write().await;
    if let Some(&id) = free_ids.iter().next() {
        free_ids.remove(&id);
        trace!("find_available_listener_id: {id}");
        Ok(id)
    } else {
        Err(UStatus::fail_with_code(
            UCode::RESOURCE_EXHAUSTED,
            "No more extern C fns available",
        ))
    }
}

pub(crate) async fn release_listener_id(
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
        let mut listener_id_authority_name = LISTENER_ID_AUTHORITY_NAME.write().await;

        trace!(
            "checking listener_id_authority_name: {:?}",
            *listener_id_authority_name
        );

        listener_id_authority_name
            .remove(&listener_id)
            .ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!("Unable to locate authority_name for listener_id: {listener_id}"),
                )
            })?;

        let mut listener_id_remote_authority_name = LISTENER_ID_REMOTE_AUTHORITY_NAME.write().await;

        listener_id_remote_authority_name
            .remove(&listener_id)
            .ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!(
                        "Unable to locate remote_authority_name for listener_id: {listener_id}"
                    ),
                )
            })?;

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

        let mut listener_client_id_mapping = LISTENER_ID_CLIENT_ID_MAPPING.write().await;
        listener_client_id_mapping
            .remove(&listener_id)
            .ok_or_else(|| {
                UStatus::fail_with_code(
                    UCode::INTERNAL,
                    format!(
                        "Unable to locate client_id (i.e. for app) for listener_id: {listener_id}"
                    ),
                )
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

pub(crate) async fn register_listener_id_with_listener(
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

lazy_static! {
    static ref CLIENT_ID_APP_MAPPING: RwLock<HashMap<ClientId, String>> =
        RwLock::new(HashMap::new());
}

pub(crate) async fn map_listener_id_to_client_id(
    client_id: ClientId,
    listener_id: usize,
) -> Result<(), UStatus> {
    let mut listener_client_id_mapping = LISTENER_ID_CLIENT_ID_MAPPING.write().await;
    listener_client_id_mapping.insert(listener_id, client_id).map(|_| Err(UStatus::fail_with_code(
        UCode::INTERNAL,
        format!("Unable to have the same listener_id with a different client_id, i.e. tied to app: listener_id: {} client_id: {}", listener_id, client_id),
    )))
        .unwrap_or(Ok(()))?;
    Ok(())
}

pub(crate) async fn find_app_name(client_id: ClientId) -> Result<ApplicationName, UStatus> {
    let client_id_app_mapping = CLIENT_ID_APP_MAPPING.read().await;
    if let Some(app_name) = client_id_app_mapping.get(&client_id) {
        Ok(app_name.clone())
    } else {
        Err(UStatus::fail_with_code(
            UCode::NOT_FOUND,
            format!("There was no app_name found for client_id: {}", client_id),
        ))
    }
}

pub(crate) async fn add_client_id_app_name(
    client_id: ClientId,
    app_name: &ApplicationName,
) -> Result<(), UStatus> {
    let mut client_id_app_mapping = CLIENT_ID_APP_MAPPING.write().await;
    if let std::collections::hash_map::Entry::Vacant(e) = client_id_app_mapping.entry(client_id) {
        e.insert(app_name.clone());
        trace!(
            "Inserted client_id: {} and app_name: {} into CLIENT_ID_APP_MAPPING",
            client_id,
            app_name
        );
        Ok(())
    } else {
        let err_msg = format!(
            "Already had key. Somehow we already had an application running for client_id: {}",
            client_id
        );
        Err(UStatus::fail_with_code(UCode::ALREADY_EXISTS, err_msg))
    }
}

// TODO: Add unit tests
