use cxx::{let_cxx_string, SharedPtr};
use std::thread;
use std::thread::{park, sleep};
use std::time::Duration;
use vsomeip_sys::glue::{
    make_application_wrapper, make_message_wrapper, make_runtime_wrapper, MessageHandlerFnPtr,
    SubscriptionStatusHandlerFnPtr,
};
use vsomeip_sys::vsomeip;
use vsomeip_sys::vsomeip::{message, runtime, ANY_MAJOR, ANY_MINOR};

const SAMPLE_SERVICE_ID: u16 = 0x1234;
// const SAMPLE_INSTANCE_ID: u16 = 0x5678;
const SAMPLE_INSTANCE_ID: u16 = 1;

const SAMPLE_EVENTGROUP_ID: u16 = 0x4465;
const SAMPLE_EVENT_ID: u16 = 0x4465;
const APP_NAME: &str = "Subscriber";

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

    let_cxx_string!(my_app_str = "Subscriber");

    let Some(app_wrapper) =
        make_application_wrapper(runtime_wrapper.get_pinned().get_application(&my_app_str))
    else {
        panic!("Application does not exist app_name: {APP_NAME}");
    };

    let client_id = app_wrapper.get_pinned().get_client();
    println!("client_id: {client_id}");

    app_wrapper.get_pinned().request_service(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        ANY_MAJOR,
        ANY_MINOR,
    );

    app_wrapper.request_single_event_safe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENT_ID,
        SAMPLE_EVENTGROUP_ID,
    );

    extern "C" fn subscription_status_listener(
        service: vsomeip::service_t,
        instance: vsomeip::instance_t,
        eventgroup: vsomeip::eventgroup_t,
        _event: vsomeip::event_t,
        status: u16,
    ) {
        println!("Subscription status changed:\n service: {} instance: {} eventgroup: {} event: {} status: {}",
                 service, instance, eventgroup, instance, status);
    }

    let subscription_status_handler_fn_ptr =
        SubscriptionStatusHandlerFnPtr(subscription_status_listener);

    app_wrapper.register_subscription_status_handler_fn_ptr_safe(
        vsomeip::ANY_SERVICE,
        vsomeip::ANY_INSTANCE,
        vsomeip::ANY_EVENTGROUP,
        vsomeip::ANY_EVENT,
        subscription_status_handler_fn_ptr,
        true,
    );

    app_wrapper.get_pinned().subscribe(
        SAMPLE_SERVICE_ID,
        SAMPLE_INSTANCE_ID,
        SAMPLE_EVENTGROUP_ID,
        ANY_MAJOR,
        SAMPLE_EVENT_ID,
    );

    extern "C" fn my_msg_handler(msg: &SharedPtr<message>) {
        println!("received event!");

        let cloned_msg = msg.clone();
        let msg_wrapper = make_message_wrapper(cloned_msg);

        let msg_type = msg_wrapper.get_message_base_pinned().get_message_type();
        println!("message_type_e: {msg_type:?}");

        let Some(payload_wrapper) = msg_wrapper.get_message_payload() else {
            panic!("Unable to get PayloadWrapper from MessageWrapper");
        };
        let payload = payload_wrapper.get_data_safe();

        println!("payload:\n{payload:?}")
    }
    let my_callback = MessageHandlerFnPtr(my_msg_handler);

    app_wrapper.register_message_handler_fn_ptr_safe(
        SAMPLE_SERVICE_ID,
        vsomeip::ANY_INSTANCE,
        SAMPLE_EVENT_ID,
        my_callback,
    );

    park();
}
