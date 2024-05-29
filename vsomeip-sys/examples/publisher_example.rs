use cxx::let_cxx_string;
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use vsomeip_sys::extern_callback_wrappers::AvailabilityHandlerFnPtr;
use vsomeip_sys::glue::{
    make_application_wrapper, make_message_wrapper, make_payload_wrapper, make_runtime_wrapper,
};
use vsomeip_sys::safe_glue::{
    get_pinned_application, get_pinned_message_base, get_pinned_payload, get_pinned_runtime,
    offer_single_event_safe, register_availability_handler_fn_ptr_safe, set_data_safe,
};
use vsomeip_sys::vsomeip;
use vsomeip_sys::vsomeip::{instance_t, runtime, service_t};

const SAMPLE_SERVICE_ID: u16 = 0x1234;
// const SAMPLE_INSTANCE_ID: u16 = 0x5678;
const SAMPLE_INSTANCE_ID: u16 = 1;
const SAMPLE_METHOD_ID: u16 = 0x0421;

const SAMPLE_EVENTGROUP_ID: u16 = 0x4465;
const SAMPLE_EVENT_ID: u16 = 0x4465;
const SAMPLE_CLIENT_ID: u16 = 257;

fn start_app() {
    let my_runtime = runtime::get();
    let runtime_wrapper = make_runtime_wrapper(my_runtime);

    let_cxx_string!(my_app_str = "Publisher");
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

    let_cxx_string!(my_app_str = "Publisher");

    let mut app_wrapper =
        make_application_wrapper(get_pinned_runtime(&runtime_wrapper).get_application(&my_app_str));

    get_pinned_application(&app_wrapper).offer_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        vsomeip::ANY_MAJOR,
        vsomeip::ANY_MINOR,
    );

    offer_single_event_safe(
        &mut app_wrapper,
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENT_ID,
        SAMPLE_EVENTGROUP_ID,
    );

    loop {
        let payload_wrapper =
            make_payload_wrapper(get_pinned_runtime(&runtime_wrapper).create_payload());

        let mut payload_data: Vec<u8> = Vec::new();
        for i in 0..10 {
            payload_data.push((i as u16 % 256) as u8);
        }

        set_data_safe(get_pinned_payload(&payload_wrapper), &payload_data);

        println!("attempting notify...");
        // get_pinned_application(&app_wrapper).notify_one(
        //     SAMPLE_SERVICE_ID,
        //     SAMPLE_INSTANCE_ID,
        //     SAMPLE_EVENT_ID,
        //     payload_wrapper.get_shared_ptr(),
        //     SAMPLE_CLIENT_ID,
        //     true,
        // );
        get_pinned_application(&app_wrapper).notify(
            SAMPLE_SERVICE_ID,
            SAMPLE_INSTANCE_ID,
            SAMPLE_EVENT_ID,
            payload_wrapper.get_shared_ptr(),
            true,
        );

        sleep(Duration::from_millis(2000));
    }
}
