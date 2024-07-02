use cxx::{let_cxx_string, SharedPtr};
use log::error;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use vsomeip_sys::glue::{
    make_application_wrapper, make_message_wrapper, make_runtime_wrapper, MessageHandlerFnPtr,
};
use vsomeip_sys::vsomeip;
use vsomeip_sys::vsomeip::{message, runtime};

const SAMPLE_SERVICE_ID: u16 = 0x1234;
const SAMPLE_INSTANCE_ID: u16 = 1;
// const SAMPLE_INSTANCE_ID: u16 = 0x5678;
const SAMPLE_METHOD_ID: u16 = 0x0421;
const APP_NAME: &str = "World";

fn start_app() {
    let my_runtime = runtime::get();
    let runtime_wrapper = make_runtime_wrapper(my_runtime);

    let_cxx_string!(my_app_str = APP_NAME);
    let Some(app_wrapper) =
        make_application_wrapper(runtime_wrapper.get_pinned().create_application(&my_app_str))
    else {
        panic!("No app created for app_name: {APP_NAME}");
    };
    app_wrapper.get_pinned().init();
    app_wrapper.get_pinned().start();
}

fn main() {
    thread::spawn(move || {
        start_app();
    });

    println!("past the thread spawn");

    sleep(Duration::from_millis(2000));

    println!("past the sleep");

    let my_runtime = runtime::get();
    let runtime_wrapper = make_runtime_wrapper(my_runtime);

    println!("after we get the runtime");

    let_cxx_string!(my_app_str = APP_NAME);

    let Some(app_wrapper) =
        make_application_wrapper(runtime_wrapper.get_pinned().get_application(&my_app_str))
    else {
        panic!("No app found for app_name: {APP_NAME}");
    };
    app_wrapper.get_pinned().offer_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        vsomeip::ANY_MAJOR,
        vsomeip::ANY_MINOR,
    );

    extern "C" fn my_msg_handler(_msg: &SharedPtr<message>) {
        println!("received Request!");

        let cloned_msg = _msg.clone();
        let msg_wrapper = make_message_wrapper(cloned_msg);

        let msg_type = msg_wrapper.get_message_base_pinned().get_message_type();
        println!("message_type_e: {msg_type:?}");

        let Some(payload_wrapper) = msg_wrapper.get_message_payload() else {
            panic!("Unable to get PayloadWrapper from MessageWrapper");
        };
        let payload = payload_wrapper.get_data_safe();

        println!("payload:\n{payload:?}");

        let payload_string = std::str::from_utf8(&payload);
        match payload_string {
            Ok(str) => {
                println!("payload_string: {str}");
            }
            Err(err) => {
                error!("unable to convert bytes to string: {err:?}");
            }
        }
    }
    let my_callback = MessageHandlerFnPtr(my_msg_handler);

    app_wrapper.register_message_handler_fn_ptr_safe(
        SAMPLE_SERVICE_ID,
        vsomeip::ANY_INSTANCE,
        SAMPLE_METHOD_ID,
        my_callback,
    );

    thread::park();
}
