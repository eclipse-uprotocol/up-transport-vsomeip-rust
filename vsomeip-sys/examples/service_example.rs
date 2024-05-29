use cxx::{let_cxx_string, SharedPtr};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use vsomeip_sys::glue::{make_application_wrapper, make_runtime_wrapper};
use vsomeip_sys::safe_glue::{
    get_pinned_application, get_pinned_runtime, register_message_handler_fn_ptr_safe,
};
use vsomeip_sys::vsomeip::{message, runtime};
use vsomeip_sys::{extern_callback_wrappers::MessageHandlerFnPtr, vsomeip};

const SAMPLE_SERVICE_ID: u16 = 0x1234;
const SAMPLE_INSTANCE_ID: u16 = 1;
// const SAMPLE_INSTANCE_ID: u16 = 0x5678;
const SAMPLE_METHOD_ID: u16 = 0x0421;

fn start_app() {
    let my_runtime = runtime::get();
    let runtime_wrapper = make_runtime_wrapper(my_runtime);

    let_cxx_string!(my_app_str = "World");
    let app_wrapper = make_application_wrapper(
        get_pinned_runtime(&runtime_wrapper).create_application(&my_app_str),
    );
    get_pinned_application(&app_wrapper).init();
    get_pinned_application(&app_wrapper).start();
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

    let_cxx_string!(my_app_str = "World");

    let mut app_wrapper =
        make_application_wrapper(get_pinned_runtime(&runtime_wrapper).get_application(&my_app_str));

    get_pinned_application(&app_wrapper).offer_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        vsomeip::ANY_MAJOR,
        vsomeip::ANY_MINOR,
    );

    extern "C" fn my_msg_handler(_msg: &SharedPtr<message>) {
        println!("received Request!");
    }
    let my_callback = MessageHandlerFnPtr(my_msg_handler);

    register_message_handler_fn_ptr_safe(
        &mut app_wrapper,
        SAMPLE_SERVICE_ID,
        vsomeip::ANY_INSTANCE,
        SAMPLE_METHOD_ID,
        my_callback,
    );

    thread::park();
}
