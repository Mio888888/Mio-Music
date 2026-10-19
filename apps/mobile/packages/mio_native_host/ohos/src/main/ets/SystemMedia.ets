import { avSession } from '@kit.AVSessionKit';
import { backgroundTaskManager } from '@kit.BackgroundTasksKit';
import { bundleManager, common, wantAgent, WantAgent } from '@kit.AbilityKit';
import { image } from '@kit.ImageKit';
import { MethodCall, MethodChannel } from '@ohos/flutter_ohos';

// Same command numbers the Android host sends to Rust's mio_media_command.
const PLAY = 1;
const PAUSE = 2;
const NEXT = 3;
const PREVIOUS = 4;
const STOP = 5;
const SEEK = 6;
const SEEK_RELATIVE = 12;

/**
 * AVSession (lock screen, control centre, Bluetooth keys) plus the audio-playback
 * background task, which HarmonyOS only grants to an app with an active session.
 * Rust stays the player: Dart mirrors its snapshot here and receives commands back.
 */
export class SystemMedia {
  private session: avSession.AVSession | null = null;
  private ability: common.UIAbilityContext | null = null;
  private launcher: WantAgent | null = null;
  private artwork: image.PixelMap | null = null;
  private background = false;
  private channel: MethodChannel;
  private context: common.Context;

  constructor(channel: MethodChannel, context: common.Context) {
    this.channel = channel;
    this.context = context;
  }

  attach(ability: common.UIAbilityContext | null): void {
    this.ability = ability;
    this.launcher = null;
  }

  async initialize(): Promise<void> {
    if (this.session !== null) {
      return;
    }
    const session = await avSession.createAVSession(this.ability ?? this.context, 'Mio Music', 'audio');
    session.on('play', () => this.command(PLAY, 0));
    session.on('pause', () => this.command(PAUSE, 0));
    session.on('stop', () => this.command(STOP, 0));
    session.on('playNext', () => this.command(NEXT, 0));
    session.on('playPrevious', () => this.command(PREVIOUS, 0));
    session.on('seek', (time: number) => this.command(SEEK, time / 1000));
    session.on('fastForward', (time?: number) => this.command(SEEK_RELATIVE, (time ?? 10000) / 1000));
    session.on('rewind', (time?: number) => this.command(SEEK_RELATIVE, -(time ?? 10000) / 1000));
    const launcher = await this.launchAgent();
    if (launcher !== null) {
      await session.setLaunchAbility(launcher);
    }
    await session.activate();
    this.session = session;
  }

  async update(call: MethodCall): Promise<void> {
    const session = this.session;
    if (session === null) {
      throw new Error('not_initialized');
    }
    if (call.hasArgument('artwork')) {
      await this.replaceArtwork(call.argument('artwork') as Uint8Array | null);
    }
    const status = call.argument('status') as string;
    const duration = call.argument('duration') as number;
    const metadata: avSession.AVMetadata = {
      assetId: call.argument('assetId') as string,
      title: call.argument('title') as string,
      artist: (call.argument('artist') as string | null) ?? '',
      album: (call.argument('album') as string | null) ?? '',
      duration: duration,
    };
    if (this.artwork !== null) {
      metadata.mediaImage = this.artwork;
    }
    await session.setAVMetadata(metadata);
    const state: avSession.AVPlaybackState = {
      state: SystemMedia.playbackState(status),
      speed: 1,
      duration: duration,
      position: { elapsedTime: call.argument('position') as number, updateTime: Date.now() },
    };
    await session.setAVPlaybackState(state);
    // Keep running through a pause, like Android's foreground service; give it up
    // once nothing can resume.
    await this.keepRunning(status === 'playing' || status === 'loading' || (this.background && status === 'paused'));
  }

  async clear(): Promise<void> {
    await this.keepRunning(false);
    await this.replaceArtwork(null);
    const state: avSession.AVPlaybackState = { state: avSession.PlaybackState.PLAYBACK_STATE_IDLE };
    await this.session?.setAVPlaybackState(state);
  }

  async appVersion(): Promise<string> {
    const info = await bundleManager.getBundleInfoForSelf(bundleManager.BundleFlag.GET_BUNDLE_INFO_DEFAULT);
    if (info.versionName.length === 0) {
      throw new Error('invalid_version');
    }
    return info.versionName;
  }

  async close(): Promise<void> {
    await this.keepRunning(false);
    await this.replaceArtwork(null);
    const session = this.session;
    this.session = null;
    await session?.destroy();
  }

  private static playbackState(status: string): avSession.PlaybackState {
    switch (status) {
      case 'playing':
        return avSession.PlaybackState.PLAYBACK_STATE_PLAY;
      case 'loading':
        return avSession.PlaybackState.PLAYBACK_STATE_BUFFERING;
      case 'paused':
        return avSession.PlaybackState.PLAYBACK_STATE_PAUSE;
      case 'ended':
        return avSession.PlaybackState.PLAYBACK_STATE_COMPLETED;
      case 'failed':
        return avSession.PlaybackState.PLAYBACK_STATE_ERROR;
      default:
        return avSession.PlaybackState.PLAYBACK_STATE_IDLE;
    }
  }

  private command(command: number, value: number): void {
    const args = new Map<string, number>();
    args.set('command', command);
    args.set('value', value);
    this.channel.invokeMethod('command', args);
  }

  private async replaceArtwork(bytes: Uint8Array | null): Promise<void> {
    const previous = this.artwork;
    this.artwork = null;
    if (bytes !== null && bytes.byteLength > 0) {
      const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      const source = image.createImageSource(buffer);
      try {
        this.artwork = await source.createPixelMap();
      } catch (_) {
        // Artwork is optional; a bad image must not block the playback state.
        this.artwork = null;
      } finally {
        await source.release();
      }
    }
    await previous?.release();
  }

  private async keepRunning(running: boolean): Promise<void> {
    const ability = this.ability;
    if (running === this.background || ability === null) {
      return;
    }
    if (running) {
      const launcher = await this.launchAgent();
      if (launcher === null) {
        return;
      }
      try {
        await backgroundTaskManager.startBackgroundRunning(ability,
          backgroundTaskManager.BackgroundMode.AUDIO_PLAYBACK, launcher);
        this.background = true;
      } catch (_) {
        // Refused while the app is already in the background; the next state
        // change retries once the user brings it forward.
      }
    } else {
      this.background = false;
      try {
        await backgroundTaskManager.stopBackgroundRunning(ability);
      } catch (_) {
        // Nothing was running.
      }
    }
  }

  private async launchAgent(): Promise<WantAgent | null> {
    const ability = this.ability;
    if (ability === null) {
      return null;
    }
    if (this.launcher === null) {
      const info: wantAgent.WantAgentInfo = {
        wants: [{ bundleName: ability.abilityInfo.bundleName, abilityName: ability.abilityInfo.name }],
        actionType: wantAgent.OperationType.START_ABILITY,
        requestCode: 0,
        actionFlags: [wantAgent.WantAgentFlags.UPDATE_PRESENT_FLAG],
      };
      this.launcher = await wantAgent.getWantAgent(info);
    }
    return this.launcher;
  }
}
