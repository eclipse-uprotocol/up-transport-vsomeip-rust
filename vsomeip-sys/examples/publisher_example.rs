use cxx::let_cxx_string;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use vsomeip_sys::glue::{make_application_wrapper, make_payload_wrapper, make_runtime_wrapper};
use vsomeip_sys::vsomeip;
use vsomeip_sys::vsomeip::runtime;

const SAMPLE_SERVICE_ID: u16 = 0x1234;
const SAMPLE_INSTANCE_ID: u16 = 1;

const SAMPLE_EVENTGROUP_ID: u16 = 0x4465;
const SAMPLE_EVENT_ID: u16 = 0x4465;
const APP_NAME: &str = "Publisher";

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

    app_wrapper.offer_single_event_safe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENT_ID,
        SAMPLE_EVENTGROUP_ID,
    );

    loop {
        let payload_wrapper = make_payload_wrapper(runtime_wrapper.get_pinned().create_payload());

        let mut payload_data: Vec<u8> = Vec::new();
        for i in 0..10 {
            payload_data.push((i as u16 % 256) as u8);
        }

        payload_wrapper.set_data_safe(&payload_data);

        println!("packed message with payload:\n{payload_data:?}");

        println!("attempting notify...");

        app_wrapper.get_pinned().notify(
            SAMPLE_SERVICE_ID,
            SAMPLE_INSTANCE_ID,
            SAMPLE_EVENT_ID,
            payload_wrapper.get_shared_ptr(),
            true,
        );

        sleep(Duration::from_millis(2000));
    }
}
