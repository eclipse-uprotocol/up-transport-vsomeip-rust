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
    let mut message_handler_ids_init = quote! {
        let mut set = HashSet::with_capacity(#num_fns);
    };

    for i in 0..num_fns {
        let extern_fn_name = format_ident!("extern_fn_message_handler_{}", i);

        let fn_code = quote! {
            #[no_mangle]
            extern "C" fn #extern_fn_name(vsomeip_msg: &SharedPtr<vsomeip::message>) {
                trace!("ext: {}", #i);
                call_shared_extern_fn(#i, vsomeip_msg);
            }
        };

        generated_fns.extend(fn_code);

        let match_arm = quote! {
            #i => #extern_fn_name,
        };
        match_arms.push(match_arm);

        message_handler_ids_init.extend(quote! {
            set.insert(#i);
        });
    }

    let expanded = quote! {
        pub(super) mod message_handler_proc_macro {
            use super::*;

            lazy_static! {
                pub(super) static ref FREE_MESSAGE_HANDLER_IDS: RwLock<HashSet<usize>> = {
                    #message_handler_ids_init
                    RwLock::new(set)
                };
            }

            #generated_fns

            fn call_shared_extern_fn(message_handler_id: usize, vsomeip_msg: &SharedPtr<vsomeip::message>) {
                let transport_storage_res = ProcMacroMessageHandlerAccess::get_message_handler_id_transport(message_handler_id);
                let transport_storage = {
                    match transport_storage_res {
                        Some(transport_storage) => transport_storage.clone(),
                        None => {
                            warn!("No transport storage found for message_handler_id: {message_handler_id}");
                            return;
                        }
                    }
                };
                let runtime_handle = transport_storage.get_runtime_handle();

                let vsomeip_msg = make_message_wrapper(vsomeip_msg.clone()).get_shared_ptr();
                let (tx, rx) = mpsc::channel();
                // Create a LocalSet for running !Send futures
                let local_set = LocalSet::new();
                // Use the runtime to run the async function within the LocalSet
                local_set.spawn_local(async move {

                    let cloned_vsomeip_msg = vsomeip_msg.clone();
                    let mut vsomeip_msg_wrapper = make_message_wrapper(cloned_vsomeip_msg);

                    trace!("Made vsomeip_msg_wrapper");

                    let authority_name = transport_storage.get_local_authority();
                    let remote_authority_name = transport_storage.get_remote_authority();

                    let transport_storage_clone = transport_storage.clone();
                    let res = VsomeipMessageToUMessage::convert_vsomeip_msg_to_umsg(
                        &authority_name,
                        &remote_authority_name,
                        transport_storage_clone,
                        &mut vsomeip_msg_wrapper,
                    )
                    .await;

                    trace!("Ran convert_vsomeip_msg_to_umsg");

                    let Ok(umsg) = res else {
                        if let Err(err) = res {
                            error!("Unable to convert vsomeip message to UMessage: {:?}", err);
                        }
                        return;
                    };

                    trace!("Was able to convert to UMessage");

                    trace!("Calling listener registered under {}", message_handler_id);

                    // Separate the scope for accessing the registry for listener
                    let listener = {
                        let registry = transport_storage.clone();
                        match registry.get_listener_for_message_handler_id(message_handler_id) {
                            Some(listener) => {
                                // Send the listener and umsg back to the main thread
                                if tx.send((listener, umsg)).is_err() {
                                    error!("Failed to send listener and umsg to main thread");
                                }
                            },
                            None => {
                                error!("Listener not found for ID {}", message_handler_id);
                                return;
                            }
                        }
                    };
                });
                runtime_handle.block_on(local_set);

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
                runtime_handle.spawn(async move {
                    trace!("Within spawned thread -- calling shared_async_fn");
                    shared_async_fn(listener, umsg).await;
                    trace!("Within spawned thread -- finished shared_async_fn");
                });
            }

            async fn shared_async_fn(listener: Arc<dyn UListener>, umsg: UMessage) {
                trace!("shared_async_fn with umsg: {:?}", umsg);
                listener.on_receive(umsg).await;
            }

            pub(super) fn get_extern_fn(message_handler_id: usize) -> extern "C" fn(&SharedPtr<vsomeip::message>) {
                trace!("get_extern_fn with message_handler_id: {}", message_handler_id);
                match message_handler_id{
                    #(#match_arms)*
                    _ => panic!("MessageHandlerId out of range: {message_handler_id}"),
                }
            }
        }
    };

    expanded.into()
}

#[proc_macro]
pub fn generate_available_state_handler_extern_c_fns(input: TokenStream) -> TokenStream {
    let num_fns = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();

    let mut generated_fns = quote! {};
    let mut match_arms = Vec::with_capacity(num_fns);
    let mut free_available_state_handler_extern_fn_ids = quote! {
        let mut set = HashSet::with_capacity(#num_fns);
    };
    let mut sender_receiver_initialization = quote! {};

    for i in 0..num_fns {
        let extern_fn_name = format_ident!("available_state_handler_extern_fn_{}", i);

        let fn_code = quote! {
            #[no_mangle]
            extern "C" fn #extern_fn_name(available_state: vsomeip::state_type_e) {
                trace!("Calling extern_fn: {}", #i);
                call_shared_extern_fn(#i, available_state);
            }
        };

        generated_fns.extend(fn_code);

        let match_arm = quote! {
            #i => #extern_fn_name,
        };
        match_arms.push(match_arm);

        free_available_state_handler_extern_fn_ids.extend(quote! {
            set.insert(#i);
        });

        sender_receiver_initialization.extend(quote! {
            // recently changed the inside of this, afterwards, got compiler error
            let (sender, receiver) = crossbeam_channel::bounded(1000);

            AVAILABLE_STATE_SENDERS.write().unwrap().insert(#i, Some(sender));
            AVAILABLE_STATE_RECEIVERS.write().unwrap().insert(#i, Some(receiver));
        });
    }

    let expanded = quote! {
        pub(super) mod available_state_handler_proc_macro {
            use super::*;

            lazy_static! {
                pub(super) static ref FREE_AVAILABLE_STATE_HANDLER_EXTERN_FN_IDS: RwLock<HashSet<usize>> = {
                    #free_available_state_handler_extern_fn_ids
                    RwLock::new(set)
                };
                static ref AVAILABLE_STATE_SENDERS: RwLock<HashMap<usize, Option<Sender<vsomeip::state_type_e>>>> = RwLock::new(HashMap::new());
                static ref AVAILABLE_STATE_RECEIVERS: RwLock<HashMap<usize, Option<Receiver<vsomeip::state_type_e>>>> = RwLock::new(HashMap::new());
            }

            #generated_fns

            // TODO: remember to call unregister_state_handler after we either receive the state on
            //  this channel OR we timeout
            fn call_shared_extern_fn(available_handler_id: usize, available_state: vsomeip::state_type_e) {

                let available_state_senders = AVAILABLE_STATE_SENDERS.read().unwrap();

                match available_state_senders.get(&available_handler_id) {
                    None => {
                        error!("No available state sender for available_handler_id: {available_handler_id}");
                        return;
                    }
                    Some(sender_option) => {
                        if let Some(sender) = sender_option {
                            if let Err(e) = sender.send(available_state) {
                                error!("Failed to send state for available_handler_id: {available_handler_id}, error: {:?}", e);
                                return;
                            }
                        } else {
                            error!("No sender available for available_handler_id: {available_handler_id}");
                            return;
                        }
                    }
                }
            }

            pub(super) fn get_extern_fn(available_handler_id: usize) -> (extern "C" fn(available_state: vsomeip::state_type_e), Receiver<vsomeip::state_type_e>) {
                trace!("get_extern_fn with available_handler_id: {}", available_handler_id);
                let extern_fn = match available_handler_id {
                    #(#match_arms)*
                    _ => panic!("available_handler_id out of range"),
                };

                let available_state_receivers = AVAILABLE_STATE_RECEIVERS.read().unwrap();

                let receiver = {
                    match available_state_receivers.get(&available_handler_id) {
                        None => {
                            panic!("No available state receiver for available_handler_id: {available_handler_id}");
                        }
                        Some(receiver_option) => {
                            if let Some(receiver) = receiver_option {
                                receiver.clone()
                            } else {
                                panic!("No receiver available for available_handler_id: {available_handler_id}");
                            }
                        }
                    }
                };

                (extern_fn, receiver)
            }

            // TODO: It would be good to ensure that this function can only be called once
            //  in the type system to prevent accidents
            pub(super) fn initialize_sender_receiver() {
                trace!("initializing AVAILABLE_STATE_SENDERS and AVAILABLE_STATE_RECEIVERS");

                #sender_receiver_initialization
            }
        }
    };

    expanded.into()
}
