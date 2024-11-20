# vsomeip-sys

## What is it?

A fairly basic wrapper around the essentials needed to make a uProtocol uTransport implementation for SOME/IP based on top of the C++ vsomeip library.

## Compatible vsomeip version

We currently support vsomeip **3.4.10** as released [here](https://github.com/COVESA/vsomeip/releases/tag/3.4.10).

## How do I build it?

1. Ensure you have a Rust toolchain installed
2. Ensure you have the vsomeip library installed (optional, see features in `Cargo.toml`)
3. Ensure that you have the [requirements](https://github.com/COVESA/vsomeip?tab=readme-ov-file#build-instructions-for-linux) of the vsomeip project install
4. Run env_setup.sh in parent directory

``` bash
source ../build/env_setup.sh
```

Then,

```bash
VSOMEIP_INSTALL_PATH=<path/to/where/to/install/vsomeip> cargo build
```

## Running a binary built with vsomeip-sys

You will need to ensure that `LD_LIBRARY_PATH` includes the path to your vsomeip library install by:

```bash
LD_LIBRARY_PATH=$LD_LIBRARY_PATH:<path/to/vsomeip/lib> ./your_binary_built_with_vsomeip-sys
```

or by permanently modifying your `LD_LIBRARY_PATH` to include the path to your vsomeip library install.

## Allowing to build vsomeip for you (default)

The `bundled` feature will compile vsomeip for you and is enabled by default.

### vsomeip install path (optional)

You may additionally set:

```bash
VSOMEIP_INSTALL_PATH=/path/to/install/vsomeip
```

if you wish to choose where to install vsomeip. You can after-the-fact move this to some desired location on your system.

In any case you should put `${VSOMEIP_INSTALL_PATH}/lib` in your `LD_LIBRARY_PATH`.

## Supplying your own vsomeip includes (optional)

You may disable the `bundled` feature if you wish to provide your own vsomeip library.

The `VSOMEIP_INSTALL_PATH` then becomes _required_ and must point to the install location of your vsomeip build.

You may do the following (note `--no-default-features`):
```bash
VSOMEIP_INSTALL_PATH=<path/to/where/to/install/vsomeip> cargo build --no-default-features
```

## My build and deployment environments differ

This is fine. Simply make sure that the path to the vsomeip shared libraries is present on the `LD_LIBRARY_PATH` on your deployment environment.
