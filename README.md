# Eclipse uProtocol Rust vsomeip Client

## Overview

This library implements a uTransport client for vsomeip in Rust following the uProtocol [uTransport Specifications](https://github.com/eclipse-uprotocol/uprotocol-spec/blob/main/up-l1/README.adoc).

## Getting Started

### Building the Library

To build the library, run `cargo build` in the project root directory. This library leverages the [up-rust](https://github.com/eclipse-uprotocol/up-rust/tree/main) library for data types and models specified by uProtocol.

### Running the Tests

To run the tests, run
```bash
LD_LIBRARY_PATH=<YOUR-PATH-TO-VSOMEIP-SHARED-LIB> VSOMEIP_LIB_DIR==<YOUR-PATH-TO-VSOMEIP-SHARED-LIB> cargo test -- --test-threads 1
```

Breaking this down:
* `LD_LIBRARY_PATH` will, at run-time, tell where you have installed the vsomeip shared libraries (i.e. where `vsomeip3.so` is located)
* `VSOMEIP_LIB_DIR` will, at compile-time, tell where you have installed the vsomeip shared libraries
* We need to pass in `-- --test-threads 1` because the tests refer to the same configurations and will fall over if they are run simultaneously. So we instruct to use a single thread, i.e. run the tests in serial.

### Using the Library

The library contains the following modules:

Package | [uProtocol spec](https://github.com/eclipse-uprotocol/uprotocol-spec) | Purpose
---|---|---
transport | [uP-L1 Specifications](https://github.com/eclipse-uprotocol/uprotocol-spec/blob/main/up-l1/README.adoc) | Implementation of vsomeip uTransport client used for bidirectional point-2-point communication between uEs.
