import { common } from '@kit.AbilityKit';
import {
  AbilityAware,
  AbilityPluginBinding,
  FlutterPlugin,
  FlutterPluginBinding,
  MethodCall,
  MethodCallHandler,
  MethodChannel,
  MethodResult,
} from '@ohos/flutter_ohos';
import { Files } from './Files';
import { AbilityHolder, DeviceHandler, DownloadsHandler, HostHandler } from './Host';
import { Microphone } from './Microphone';
import { SystemMedia } from './SystemMedia';

/**
 * HarmonyOS side of mio_native_host. It serves the channels the Android plugin and
 * activity serve, plus path_provider's default channel, which has no OHOS
 * implementation on pub.dev and would otherwise stop the app at startup.
 */
export default class MioNativeHostPlugin implements FlutterPlugin, AbilityAware {
  private holder = new AbilityHolder();
  private media: SystemMedia | null = null;
  private files: Files | null = null;
  private microphone = new Microphone();
  private channels: MethodChannel[] = [];

  getUniqueClassName(): string {
    return 'MioNativeHostPlugin';
  }

  onAttachedToEngine(binding: FlutterPluginBinding): void {
    const context = binding.getApplicationContext();
    const messenger = binding.getBinaryMessenger();
    const mediaChannel = new MethodChannel(messenger, 'mio/system_media');
    this.media = new SystemMedia(mediaChannel, context);
    this.files = new Files(context);
    this.listen(mediaChannel, new MediaHandler(this.media));
    this.listen(new MethodChannel(messenger, 'mio/files'), this.files);
    this.listen(new MethodChannel(messenger, 'mio/microphone'), this.microphone);
    this.listen(new MethodChannel(messenger, 'mio/host'), new HostHandler(this.holder));
    this.listen(new MethodChannel(messenger, 'mio/device'), new DeviceHandler());
    this.listen(new MethodChannel(messenger, 'mio/downloads'), new DownloadsHandler(this.holder, context));
    this.listen(new MethodChannel(messenger, 'plugins.flutter.io/path_provider'), new PathHandler(context));
  }

  onDetachedFromEngine(binding: FlutterPluginBinding): void {
    for (const channel of this.channels) {
      channel.setMethodCallHandler(null);
    }
    this.channels = [];
    this.attach(null);
    this.media?.close();
    this.media = null;
    this.files = null;
  }

  onAttachedToAbility(binding: AbilityPluginBinding): void {
    this.attach(binding.getAbility().context);
  }

  onDetachedFromAbility(): void {
    this.attach(null);
  }

  private listen(channel: MethodChannel, handler: MethodCallHandler): void {
    channel.setMethodCallHandler(handler);
    this.channels.push(channel);
  }

  private attach(ability: common.UIAbilityContext | null): void {
    this.holder.ability = ability;
    this.media?.attach(ability);
    this.files?.attach(ability);
    this.microphone.attach(ability);
  }
}

class MediaHandler implements MethodCallHandler {
  private media: SystemMedia;

  constructor(media: SystemMedia) {
    this.media = media;
  }

  onMethodCall(call: MethodCall, result: MethodResult): void {
    let work: Promise<string | null>;
    switch (call.method) {
      case 'initialize':
        work = this.media.initialize().then(() => null);
        break;
      case 'update':
        work = this.media.update(call).then(() => null);
        break;
      case 'clear':
        work = this.media.clear().then(() => null);
        break;
      case 'appVersion':
        work = this.media.appVersion();
        break;
      default:
        result.notImplemented();
        return;
    }
    work.then((value) => result.success(value))
      .catch((error: Error) => result.error('system_media_failed', error.message, null));
  }
}

class PathHandler implements MethodCallHandler {
  private context: common.Context;

  constructor(context: common.Context) {
    this.context = context;
  }

  onMethodCall(call: MethodCall, result: MethodResult): void {
    switch (call.method) {
      case 'getTemporaryDirectory':
        result.success(this.context.tempDir);
        break;
      case 'getApplicationSupportDirectory':
      case 'getApplicationDocumentsDirectory':
        result.success(this.context.filesDir);
        break;
      case 'getApplicationCacheDirectory':
        result.success(this.context.cacheDir);
        break;
      default:
        result.notImplemented();
    }
  }
}
