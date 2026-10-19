import { bundleManager, common, OpenLinkOptions, Want, wantConstant } from '@kit.AbilityKit';
import { fileIo, fileUri } from '@kit.CoreFileKit';
import { MethodCall, MethodCallHandler, MethodResult } from '@ohos/flutter_ohos';

const ABOUT_LINKS = new Map<string, string>([
  ['original', 'https://github.com/timeshiftsauce/CeruMusic'],
  ['docs', 'https://ceru.docs.shiqianjiang.cn/'],
  ['eqVault', 'https://mio888888.github.io/EQVault/'],
]);
const BACKUP_PROVIDERS = new Map<string, string>([
  ['hi168', 'https://www.hi168.com'],
  ['cstcloud', 'https://data.cstcloud.cn/'],
]);
const AUDIO_TYPES = new Map<string, string>([
  ['mp3', 'audio/mpeg'], ['flac', 'audio/flac'], ['wav', 'audio/wav'], ['ogg', 'audio/ogg'],
  ['opus', 'audio/ogg'], ['m4a', 'audio/mp4'], ['alac', 'audio/mp4'], ['aac', 'audio/aac'],
  ['wma', 'audio/x-ms-wma'], ['ape', 'audio/x-ape'],
]);

/** Shared by the handlers below: the ability that opens links and other apps. */
export class AbilityHolder {
  ability: common.UIAbilityContext | null = null;
}

/** `mio/host`: the same fixed links and app facts the Android activity serves. */
export class HostHandler implements MethodCallHandler {
  private holder: AbilityHolder;

  constructor(holder: AbilityHolder) {
    this.holder = holder;
  }

  onMethodCall(call: MethodCall, result: MethodResult): void {
    switch (call.method) {
      case 'appVersion':
        bundleManager.getBundleInfoForSelf(bundleManager.BundleFlag.GET_BUNDLE_INFO_DEFAULT)
          .then((info) => info.versionName.length > 0 ? result.success(info.versionName) :
            result.error('invalid_version', 'Package version unavailable', null))
          .catch(() => result.error('invalid_version', 'Package version unavailable', null));
        break;
      case 'openAboutLink':
        this.open(ABOUT_LINKS.get(call.args as string), 'link_invalid', 'open_failed', result);
        break;
      case 'openBackupProvider':
        this.open(BACKUP_PROVIDERS.get(call.args as string), 'provider_link_invalid', 'provider_open_failed', result);
        break;
      case 'openRelease': {
        const tag = call.args as string | null;
        const valid = tag !== null && new RegExp('^[A-Za-z0-9.+-]{1,128}$').test(tag);
        this.open(valid ? 'https://github.com/Mio888888/Mio-Music/releases/tag/' + encodeURIComponent(tag) : undefined,
          'release_link_invalid', 'release_open_failed', result);
        break;
      }
      case 'lyricOverlayState': {
        // A floating window needs a system permission phones do not grant to apps.
        const state = new Map<string, Object>();
        state.set('supported', false);
        state.set('permission', false);
        state.set('enabled', false);
        state.set('locked', false);
        state.set('scale', 1.0);
        result.success(state);
        break;
      }
      case 'configureLyricOverlay':
      case 'requestLyricOverlayPermission':
        result.error('overlay_unavailable', 'Floating lyrics are unavailable on HarmonyOS', null);
        break;
      default:
        result.notImplemented();
    }
  }

  private open(address: string | undefined, invalid: string, failed: string, result: MethodResult): void {
    const ability = this.holder.ability;
    if (address === undefined || ability === null) {
      result.error(invalid, 'Link unavailable', null);
      return;
    }
    const options: OpenLinkOptions = { appLinkingOnly: false };
    ability.openLink(address, options)
      .then(() => result.success(null))
      .catch(() => result.error(failed, 'No browser could open the link', null));
  }
}

/** `mio/device`: the screen lease exists only for Android TV; there is none here. */
export class DeviceHandler implements MethodCallHandler {
  onMethodCall(call: MethodCall, result: MethodResult): void {
    switch (call.method) {
      case 'screenEpoch':
        result.success(null);
        break;
      case 'keepScreenOn':
        if (call.argument('active') === true) {
          result.error('activity_changed', 'Screen lease requires a television', null);
        } else {
          result.success(null);
        }
        break;
      default:
        result.notImplemented();
    }
  }
}

/** `mio/downloads`: opens a completed download, by basename, in another app. */
export class DownloadsHandler implements MethodCallHandler {
  private holder: AbilityHolder;
  private context: common.Context;

  constructor(holder: AbilityHolder, context: common.Context) {
    this.holder = holder;
    this.context = context;
  }

  onMethodCall(call: MethodCall, result: MethodResult): void {
    if (call.method !== 'openCompleted') {
      result.notImplemented();
      return;
    }
    const name = call.args as string | null;
    if (name === null || name.length === 0 || name.length > 1024 || name === '.' || name === '..' ||
      new RegExp('[\\u0000-\\u001f\\u007f/\\\\:]').test(name)) {
      result.error('invalid_input', 'Invalid download file', null);
      return;
    }
    const ability = this.holder.ability;
    if (ability === null) {
      result.error('open_unavailable', 'File opening unavailable', null);
      return;
    }
    // Dart keeps downloads in getApplicationDocumentsDirectory()/Downloads.
    const path = this.context.filesDir + '/Downloads/' + name;
    try {
      const stat = fileIo.lstatSync(path);
      if (!stat.isFile() || stat.size <= 0) {
        result.error('file_unavailable', 'Download file unavailable', null);
        return;
      }
    } catch (error) {
      result.error('file_unavailable', 'Download file unavailable', null);
      return;
    }
    const dot = name.lastIndexOf('.');
    const type = dot < 0 ? undefined : AUDIO_TYPES.get(name.substring(dot + 1).toLowerCase());
    if (type === undefined) {
      result.error('unsupported_file', 'Audio file type unsupported', null);
      return;
    }
    const want: Want = {
      action: 'ohos.want.action.viewData',
      uri: fileUri.getUriFromPath(path),
      type: type,
      flags: wantConstant.Flags.FLAG_AUTH_READ_URI_PERMISSION,
    };
    ability.startAbility(want)
      .then(() => result.success(null))
      .catch(() => result.error('no_handler', 'No application can open this audio file', null));
  }
}
