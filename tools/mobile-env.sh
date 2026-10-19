#!/usr/bin/env bash
# Source this file before native mobile builds. Keep host toolchains untouched.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"
export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
export IPHONEOS_DEPLOYMENT_TARGET=15.0
export LIBCLANG_PATH="${LIBCLANG_PATH:-/opt/homebrew/opt/llvm@22/lib}"
mio_ndk_bin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
export CC_aarch64_linux_android="$mio_ndk_bin/aarch64-linux-android26-clang"
export CC_armv7_linux_androideabi="$mio_ndk_bin/armv7a-linux-androideabi26-clang"
export AR_aarch64_linux_android="$mio_ndk_bin/llvm-ar"
export AR_armv7_linux_androideabi="$mio_ndk_bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$CC_armv7_linux_androideabi"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--sysroot=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/sysroot --target=aarch64-linux-android26"
export BINDGEN_EXTRA_CLANG_ARGS_armv7_linux_androideabi="--sysroot=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/sysroot --target=armv7a-linux-androideabi26"
unset mio_ndk_bin
