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

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, LitInt};

/// Generates "N" number of extern "C" fns to be used and recycled by the up-client-vsomeip-rust
/// implementation.
///
/// # Rationale
///
/// The vsomeip-sys crate requires extern "C" fns to be passed to it when registering a message handler
///
/// By using pre-generated extern "C" fns we are then able to ignore that implementation detail inside
/// of the UTransport implementation of vsomeip
#[proc_macro]
pub fn generate_message_handler_extern_c_fns(input: TokenStream) -> TokenStream {
    let num_fns = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();

    let mut generated_fns = quote! {};
    let mut match_arms = Vec::with_capacity(num_fns);
    let mut free_listener_ids_init = quote! {
        let mut set = HashSet::with_capacity(#num_fns);
    };

    for i in 0..num_fns {
        let extern_fn_name = format_ident!("extern_on_msg_wrapper_{}", i);

        let fn_code = quote! {
            #[no_mangle]
            pub extern "C" fn #extern_fn_name(vsomeip_msg: &SharedPtr<vsomeip::message>) {
                trace!("Calling extern_fn: {}", #i);
                call_shared_extern_fn(#i, vsomeip_msg);
            }
        };

        generated_fns.extend(fn_code);

        let match_arm = quote! {
            #i => #extern_fn_name,
        };
        match_arms.push(match_arm);

        free_listener_ids_init.extend(quote! {
            set.insert(#i);
        });
    }

    let expanded = quote! {

        lazy_static! {
            static ref FREE_LISTENER_IDS: RwLock<HashSet<usize>> = {
                #free_listener_ids_init
                RwLock::new(set)
            };
        }

        #generated_fns

        fn call_shared_extern_fn(listener_id: usize, vsomeip_msg: &SharedPtr<vsomeip::message>) {
            // Ensure the runtime is initialized and get a handle to it
            let runtime = get_runtime();

            // Create a LocalSet for running !Send futures
            let local_set = LocalSet::new();

            let vsomeip_msg = make_message_wrapper(vsomeip_msg.clone()).get_shared_ptr();

            // Create a standard mpsc channel to send the listener and umsg back to the main thread
            let (tx, rx) = mpsc::channel();

            let before_local_set_spawn_local = Instant::now();

            // Use the runtime to run the async function within the LocalSet
            local_set.spawn_local(async move {
                let app_name = {
                    let listener_client_id_mapping = LISTENER_ID_CLIENT_ID_MAPPING.read().await;
                    if let Some(client_id) = listener_client_id_mapping.get(&listener_id) {
                        let client_id_app_mapping = CLIENT_ID_APP_MAPPING.read().await;
                        if let Some(app_name) = client_id_app_mapping.get(&client_id) {
                            Ok(app_name.clone())
                        } else {
                            Err(UStatus::fail_with_code(UCode::NOT_FOUND, format!("There was no app_name found for listener_id: {} and client_id: {}", listener_id, client_id)))
                        }
                    } else {
                        Err(UStatus::fail_with_code(UCode::NOT_FOUND, format!("There was no client_id found for listener_id: {}", listener_id)))
                    }
                };

                let Ok(app_name) = app_name else {
                    error!("App wasn't found to interact with: {:?}", app_name.err().unwrap());
                    return;
                };

                let runtime_wrapper = make_runtime_wrapper(vsomeip::runtime::get());
                let_cxx_string!(app_name_cxx = &*app_name);

                let application_wrapper = make_application_wrapper(
                    get_pinned_runtime(&runtime_wrapper).get_application(&app_name_cxx),
                );
                if application_wrapper.get_mut().is_null() {
                    error!("Unable to obtain vsomeip application with app_name: {app_name}");
                    return;
                }
                let cloned_vsomeip_msg = vsomeip_msg.clone();
                let mut vsomeip_msg_wrapper = make_message_wrapper(cloned_vsomeip_msg);

                trace!("Made vsomeip_msg_wrapper");

                let listener_id_authority_name = LISTENER_ID_AUTHORITY_NAME.read().await;
                let Some(authority_name) = listener_id_authority_name.get(&listener_id) else {
                    error!("No authority_name found for listener_id: {listener_id}");
                    return;
                };
                let listener_id_remote_authority_name = LISTENER_ID_REMOTE_AUTHORITY_NAME.read().await;
                let Some(remote_authority_name) = listener_id_remote_authority_name.get(&listener_id) else {
                    error!("No remote_authority_name found for listener_id: {listener_id}");
                    return;
                };
                let res = convert_vsomeip_msg_to_umsg(&authority_name, &remote_authority_name, &mut vsomeip_msg_wrapper, &application_wrapper, &runtime_wrapper).await;

                trace!("Ran convert_vsomeip_msg_to_umsg");

                let Ok(umsg) = res else {
                    if let Err(err) = res {
                        error!("Unable to convert vsomeip message to UMessage: {:?}", err);
                    }
                    return;
                };

                trace!("Was able to convert to UMessage");

                trace!("Calling extern function {}", listener_id);
                let registry = LISTENER_REGISTRY.read().await;
                if let Some(listener) = registry.get(&listener_id) {
                    trace!("Retrieved listener");
                    let listener = Arc::clone(listener);

                    // Send the listener and umsg back to the main thread
                    if tx.send((listener, umsg)).is_err() {
                        error!("Failed to send listener and umsg to main thread");
                    }

                } else {
                    error!("Listener not found for ID {}", listener_id);
                }
            });
            runtime.block_on(local_set);

            trace!("Reached bottom of call_shared_extern_fn");

            // Receive the listener and umsg from the mpsc channel
            let (listener, umsg) = match rx.recv() {
                Ok((listener, umsg)) => (listener, umsg),
                Err(_) => {
                    error!("Failed to receive listener and umsg from mpsc channel");
                    return;
                }
            };

            // Spawn shared_async_fn on the multi-threaded executor
            CB_RUNTIME.spawn(async move {
                trace!("Within spawned thread -- calling shared_async_fn");
                shared_async_fn(listener, umsg).await;
                trace!("Within spawned thread -- finished shared_async_fn");
            });
        }

        async fn shared_async_fn(listener: Arc<dyn UListener>, umsg: UMessage) {
            trace!("shared_async_fn with umsg: {:?}", umsg);
            listener.on_receive(umsg).await;
        }

        pub(crate) fn get_extern_fn(listener_id: usize) -> extern "C" fn(&SharedPtr<vsomeip::message>) {
            trace!("get_extern_fn with listener_id: {}", listener_id);
            match listener_id {
                #(#match_arms)*
                _ => panic!("Listener ID out of range"),
            }
        }
    };

    expanded.into()
}
