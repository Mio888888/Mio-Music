import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:flutter_rust_bridge_hooks/flutter_rust_bridge_hooks.dart';

void main(List<String> args) async {
  await build(args, (input, output) async {
    // HarmonyOS links the prebuilt library in ohos/entry/libs; pub.dev's
    // code_assets cannot parse the `ohos` target, so stop before reading it.
    if (_targetsOhos(input.json)) return;
    final environment = <String, String>{'IPHONEOS_DEPLOYMENT_TARGET': '15.0'};
    if (input.config.buildCodeAssets &&
        input.config.code.targetOS == OS.android) {
      final sdk =
          Platform.environment['ANDROID_HOME'] ??
          Platform.environment['ANDROID_SDK_ROOT'];
      final ndk =
          Platform.environment['ANDROID_NDK_HOME'] ??
          (sdk == null ? null : '$sdk/ndk/28.2.13676358');
      if (ndk == null) {
        throw StateError('Source tools/mobile-env.sh before building Android.');
      }
      // The NDK ships one prebuilt toolchain per host (Apple Silicon uses the x86_64 one too).
      final host = Platform.isMacOS
          ? 'darwin-x86_64'
          : Platform.isWindows
          ? 'windows-x86_64'
          : 'linux-x86_64';
      final bin = '$ndk/toolchains/llvm/prebuilt/$host/bin';
      final triple = switch (input.config.code.targetArchitecture) {
        Architecture.arm64 => 'aarch64-linux-android',
        Architecture.arm => 'armv7-linux-androideabi',
        final architecture => throw UnsupportedError(
          'Unsupported Android ABI: $architecture',
        ),
      };
      final compilerTriple = switch (triple) {
        'armv7-linux-androideabi' => 'armv7a-linux-androideabi',
        'aarch64-linux-android' => 'aarch64-linux-android',
        _ => throw UnsupportedError('Unsupported Android ABI: $triple'),
      };
      final key = triple.replaceAll('-', '_');
      // native_toolchain_rust defaults to API 35. Override the actual native
      // compiler/linker as well as bindgen to preserve the app's API 26 floor.
      environment.addAll({
        'CC_$key': '$bin/${compilerTriple}26-clang',
        'CXX_$key': '$bin/${compilerTriple}26-clang++',
        'AR_$key': '$bin/llvm-ar',
        'CARGO_TARGET_${key.toUpperCase()}_LINKER':
            '$bin/${compilerTriple}26-clang',
        'BINDGEN_EXTRA_CLANG_ARGS_$key':
            '--sysroot=$ndk/toolchains/llvm/prebuilt/$host/sysroot --target=${compilerTriple}26',
      });
    }
    await FlutterRustBridgeNativeAssetsBuilder(
      cratePath: '../../crates/mio-mobile-bridge',
      extraCargoBuildArgs: ['--locked'],
      extraCargoEnvironmentVariables: environment,
    ).run(input: input, output: output);
  });
}

// Flutter OHOS reports `linux` to hooks and only gives it away through the
// OpenHarmony compiler it passes in.
bool _targetsOhos(Object? json) => switch (json) {
  Map() =>
    json['target_os'] == 'ohos' ||
        (json['cc'] is String &&
            (json['cc'] as String).contains('/openharmony/native/')) ||
        json.values.any(_targetsOhos),
  List() => json.any(_targetsOhos),
  _ => false,
};
