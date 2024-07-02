use cxx::let_cxx_string;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use vsomeip_sys::glue::{
    make_application_wrapper, make_message_wrapper, make_payload_wrapper, make_runtime_wrapper,
};
use vsomeip_sys::vsomeip::runtime;

const SAMPLE_SERVICE_ID: u16 = 0x1234;
// const SAMPLE_INSTANCE_ID: u16 = 0x5678;
const SAMPLE_INSTANCE_ID: u16 = 1;
const SAMPLE_METHOD_ID: u16 = 0x0421;
const APP_NAME: &str = "Hello";

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
        panic!("Unable to find application");
    };

    app_wrapper.get_pinned().request_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        vsomeip_sys::vsomeip::ANY_MAJOR,
        vsomeip_sys::vsomeip::ANY_MINOR,
    );

    loop {
        sleep(Duration::from_millis(1000));

        let request = make_message_wrapper(runtime_wrapper.get_pinned().create_request(true));
        request
            .get_message_base_pinned()
            .set_service(SAMPLE_SERVICE_ID);
        request
            .get_message_base_pinned()
            .set_instance(SAMPLE_INSTANCE_ID);
        request
            .get_message_base_pinned()
            .set_method(SAMPLE_METHOD_ID);

        let mut payload_wrapper =
            make_payload_wrapper(runtime_wrapper.get_pinned().create_payload());

        let payload_string = "Hello, vsomeip!";
        let payload_data = payload_string.as_bytes();

        payload_wrapper.set_data_safe(payload_data);
        request.set_message_payload(&mut payload_wrapper);

        let shared_ptr_message = request.as_ref().unwrap().get_shared_ptr();
        println!("attempting send...");
        app_wrapper.get_pinned().send(shared_ptr_message);
    }
}
