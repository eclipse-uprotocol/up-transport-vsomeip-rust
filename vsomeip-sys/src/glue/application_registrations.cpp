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

#include "application_registrations.h"

#include <set>
#include <iostream>

namespace glue {

void request_single_event(ApplicationWrapper* application_wrapper,
                          vsomeip_v3::service_t _service,
                          vsomeip_v3::instance_t _instance,
                          vsomeip_v3::event_t _notifier,
                          vsomeip_v3::eventgroup_t _eventgroup) {

    std::set<vsomeip_v3::eventgroup_t> eventgroups = {_eventgroup};

    application_wrapper->get_shared_ptr()->request_event(_service, _instance, _notifier, eventgroups);
}

void offer_single_event(ApplicationWrapper* application_wrapper,
                        vsomeip_v3::service_t _service,
                        vsomeip_v3::instance_t _instance,
                        vsomeip_v3::event_t _notifier,
                        vsomeip_v3::eventgroup_t _eventgroup) {

    std::set<vsomeip_v3::eventgroup_t> eventgroups = {_eventgroup};

    application_wrapper->get_shared_ptr()->offer_event(_service, _instance, _notifier, eventgroups);
}

void register_message_handler_fn_ptr(ApplicationWrapper* application_wrapper,
                                     vsomeip_v3::service_t _service,
                                     vsomeip_v3::instance_t _instance, vsomeip_v3::method_t _method,
                                     message_handler_fn_ptr _fn_ptr_handler) {

    auto _handler = vsomeip_v3::message_handler_t(_fn_ptr_handler);

    application_wrapper->get_shared_ptr()->register_message_handler(_service, _instance, _method, _handler);
}

void register_availability_handler_fn_ptr(ApplicationWrapper* application_wrapper, vsomeip_v3::service_t _service,
                                          vsomeip_v3::instance_t _instance,
                                          availability_handler_fn_ptr _fn_ptr_handler,
                                          vsomeip_v3::major_version_t _major, vsomeip_v3::minor_version_t _minor) {

    vsomeip_v3::availability_state_handler_t _handler = [=](vsomeip_v3::service_t service, vsomeip_v3::instance_t instance, vsomeip_v3::availability_state_e availability_state) {
        bool available = !(availability_state == vsomeip_v3::availability_state_e::AS_UNAVAILABLE);

        _fn_ptr_handler(service, instance, available);
    };

    application_wrapper->get_shared_ptr()->register_availability_handler(_service, _instance, _handler, _major, _minor);
}

void register_subscription_status_handler_fn_ptr(ApplicationWrapper* application_wrapper, vsomeip_v3::service_t _service,
                                          vsomeip_v3::instance_t _instance, vsomeip_v3::eventgroup_t _eventgroup, vsomeip_v3::event_t _event,
                                          subscription_status_handler_fn_ptr _fn_ptr_handler, bool _is_selective) {

    vsomeip_v3::subscription_status_handler_t _handler = [=](vsomeip_v3::service_t _service, vsomeip_v3::instance_t _instance,
                                                             vsomeip_v3::eventgroup_t _eventgroup, vsomeip_v3::event_t _event,
                                                             uint16_t _status) {

        _fn_ptr_handler(_service, _instance, _eventgroup, _event, _status);
    };

    application_wrapper->get_shared_ptr()->register_subscription_status_handler(_service, _instance, _eventgroup, _event,
                                                                                _handler, _is_selective);
}

void register_state_handler_fn_ptr(ApplicationWrapper* application_wrapper, state_handler_fn_ptr _fn_ptr_handler) {

    auto _handler = vsomeip_v3::state_handler_t(_fn_ptr_handler);

    application_wrapper->get_shared_ptr()->register_state_handler(_handler);
}

}

