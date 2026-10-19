import { abilityAccessCtrl, common } from '@kit.AbilityKit';
import { audio } from '@kit.AudioKit';
import { MethodCall, MethodCallHandler, MethodResult } from '@ohos/flutter_ohos';

/**
 * Permission only. Rust records through OHAudio; Dart hands it the token this grants,
 * and leaving the foreground revokes that grant (see ohos.rs in the bridge).
 */
export class Microphone implements MethodCallHandler {
  private ability: common.UIAbilityContext | null = null;
  private requesting = false;

  attach(ability: common.UIAbilityContext | null): void {
    this.ability = ability;
  }

  onMethodCall(call: MethodCall, result: MethodResult): void {
    switch (call.method) {
      case 'available':
        result.success(Microphone.present());
        break;
      case 'request':
        this.request(call.args, result);
        break;
      case 'release':
        result.success(null);
        break;
      default:
        result.notImplemented();
    }
  }

  private request(token: Object | null, result: MethodResult): void {
    const ability = this.ability;
    if (typeof token !== 'number' || token <= 0 || ability === null || this.requesting) {
      result.error('microphone_busy', 'Microphone unavailable in this view', null);
      return;
    }
    if (!Microphone.present()) {
      result.error('microphone_unavailable', 'No microphone available', null);
      return;
    }
    this.requesting = true;
    abilityAccessCtrl.createAtManager().requestPermissionsFromUser(ability, ['ohos.permission.MICROPHONE'])
      .then((grant) => {
        if (grant.authResults.length === 1 && grant.authResults[0] === 0) {
          result.success(null);
        } else {
          result.error('microphone_denied', 'Microphone permission unavailable', null);
        }
      })
      .catch(() => result.error('microphone_unavailable', 'Microphone permission unavailable', null))
      .finally(() => {
        this.requesting = false;
      });
  }

  private static present(): boolean {
    try {
      return audio.getAudioManager().getRoutingManager()
        .getDevicesSync(audio.DeviceFlag.INPUT_DEVICES_FLAG).length > 0;
    } catch (error) {
      return false;
    }
  }
}
