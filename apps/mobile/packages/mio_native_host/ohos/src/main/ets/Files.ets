import { common } from '@kit.AbilityKit';
import { fileIo, fileUri, picker } from '@kit.CoreFileKit';
import { ReadOptions } from '@ohos.file.fs';
import { photoAccessHelper } from '@kit.MediaLibraryKit';
import { util } from '@kit.ArkTS';
import { MethodCall, MethodCallHandler, MethodResult } from '@ohos/flutter_ohos';

// Mirrors the Android picker's limits and the staging layout Rust's imports.rs reads.
const OWNER = 'mio-audio-staging-v1';
const AUDIO = ['mp3', 'flac', 'wav', 'ogg', 'm4a', 'aac', 'wma', 'opus', 'ape', 'alac'];
const PLUGIN_LIMIT = 1024 * 1024;
const EQ_LIMIT = 512 * 1024;
const PLAYLIST_LIMIT = 32 * 1024 * 1024;
const QUEUE_LIMIT = 64 * 1024 * 1024;
const COVER_LIMIT = 5 * 1024 * 1024;
const AUDIO_LIMIT = 256 * 1024 * 1024;
const RECOGNITION_LIMIT = 128 * 1024 * 1024;
const BATCH_LIMIT = 512 * 1024 * 1024;

class FileError extends Error {
  code: string;

  constructor(code: string) {
    super(code);
    this.code = code;
  }
}

class Request {
  token: number;
  result: MethodResult;
  cancelled = false;
  stage: string | null = null;

  constructor(token: number, result: MethodResult) {
    this.token = token;
    this.result = result;
  }
}

/** One chooser at a time; no picker URI or path reaches Dart or Rust. */
export class Files implements MethodCallHandler {
  private context: common.Context;
  private ability: common.UIAbilityContext | null = null;
  private pending: Request | null = null;
  private handed: Set<string> = new Set();
  // A stage from an earlier process can be removed; one from this process may be in use.
  private process = Files.hex();

  constructor(context: common.Context) {
    this.context = context;
  }

  attach(ability: common.UIAbilityContext | null): void {
    this.ability = ability;
    if (ability === null && this.pending !== null) {
      this.complete(this.pending, null, null);
    }
  }

  onMethodCall(call: MethodCall, result: MethodResult): void {
    switch (call.method) {
      case 'pickPlugin':
        this.start(call.args, result, () => this.readOne([], PLUGIN_LIMIT, null));
        break;
      case 'pickEq':
        this.start(call.args, result, () => this.readOne([], EQ_LIMIT, null));
        break;
      case 'pickPlaylist':
        this.start(call.args, result, () => this.readOne(['Playlist|.json,.cpl,.cmpl'], PLAYLIST_LIMIT,
          ['json', 'cpl', 'cmpl']));
        break;
      case 'pickQueue':
        this.start(call.args, result, () => this.readOne(['Queue|.cpl,.cmpl'], QUEUE_LIMIT, ['cpl', 'cmpl']));
        break;
      case 'pickCover':
        this.start(call.args, result, () => this.readCover());
        break;
      case 'pickAudio':
        this.start(call.args, result, (request: Request) => this.stageAudio(request, false));
        break;
      case 'pickRecognitionAudio':
        this.start(call.args, result, (request: Request) => this.stageAudio(request, true));
        break;
      case 'saveEq': {
        const bytes = call.argument('bytes') as Uint8Array | null;
        if (bytes === null || bytes.byteLength === 0 || bytes.byteLength > EQ_LIMIT) {
          result.error('invalid_input', 'Invalid export', null);
        } else {
          this.start(call.argument('token'), result, () => this.save('mio-eq-config.json', bytes));
        }
        break;
      }
      case 'saveQueue': {
        const bytes = call.argument('bytes') as Uint8Array | null;
        const name = call.argument('name') as string | null;
        if (bytes === null || bytes.byteLength === 0 || bytes.byteLength > QUEUE_LIMIT || name === null ||
          !new RegExp('^cerumusic-playlist-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.cmpl$').test(name)) {
          result.error('invalid_input', 'Invalid export', null);
        } else {
          this.start(call.argument('token'), result, () => this.save(name, bytes));
        }
        break;
      }
      case 'discardAudio': {
        const ticket = call.args as string;
        if (this.handed.has(ticket)) {
          this.handed.delete(ticket);
          Files.removeStage(this.stageRoot() + '/' + ticket);
        }
        result.success(null);
        break;
      }
      case 'cancel':
        if (this.pending !== null && this.pending.token === (call.args as number)) {
          this.complete(this.pending, null, null);
        }
        result.success(null);
        break;
      default:
        result.notImplemented();
    }
  }

  private start(token: Object | null, result: MethodResult, work: (request: Request) => Promise<Object | null>): void {
    if (typeof token !== 'number' || token <= 0) {
      result.error('invalid_input', 'Invalid picker request', null);
      return;
    }
    if (this.ability === null) {
      result.error('picker_unavailable', 'File picker unavailable', null);
      return;
    }
    if (this.pending !== null) {
      result.error('picker_busy', 'File picker busy', null);
      return;
    }
    const request = new Request(token, result);
    this.pending = request;
    work(request)
      .then((value: Object | null) => this.complete(request, value, null))
      .catch((error: Error) => this.complete(request, null, error instanceof FileError ? error.code : 'file_unreadable'));
  }

  private complete(request: Request, value: Object | null, error: string | null): void {
    if (this.pending !== request) {
      // Cancelled earlier: a late selection must not survive its request.
      if (request.stage !== null) {
        Files.removeStage(request.stage);
      }
      return;
    }
    this.pending = null;
    request.cancelled = true;
    if (error !== null) {
      if (request.stage !== null) {
        Files.removeStage(request.stage);
      }
      request.result.error(error, 'Selected files unavailable or exceed import limits', null);
      return;
    }
    if (typeof value === 'string') {
      this.handed.add(value);
    }
    request.result.success(value);
  }

  private async select(filters: string[], count: number): Promise<string[]> {
    const options = new picker.DocumentSelectOptions();
    options.maxSelectNumber = count;
    if (filters.length > 0) {
      options.fileSuffixFilters = filters;
    }
    return new picker.DocumentViewPicker(this.ability as common.UIAbilityContext).select(options);
  }

  /** `extensions` set: the result carries the validated name (playlist/queue/cover). */
  private async readOne(filters: string[], limit: number, extensions: string[] | null): Promise<Object | null> {
    const uris = await this.select(filters, 1);
    if (uris.length === 0) {
      return null;
    }
    if (extensions === null) {
      return Files.read(uris[0], limit);
    }
    return Files.named(uris[0], limit, extensions);
  }

  private async readCover(): Promise<Object | null> {
    const options = new photoAccessHelper.PhotoSelectOptions();
    options.MIMEType = photoAccessHelper.PhotoViewMIMETypes.IMAGE_TYPE;
    options.maxSelectNumber = 1;
    const selection = await new photoAccessHelper.PhotoViewPicker().select(options);
    if (selection.photoUris.length === 0) {
      return null;
    }
    return Files.named(selection.photoUris[0], COVER_LIMIT, ['png', 'jpg', 'jpeg', 'gif', 'webp']);
  }

  private async save(name: string, bytes: Uint8Array): Promise<Object | null> {
    const options = new picker.DocumentSaveOptions();
    options.newFileNames = [name];
    const uris = await new picker.DocumentViewPicker(this.ability as common.UIAbilityContext).save(options);
    if (uris.length === 0) {
      return null;
    }
    const file = await fileIo.open(uris[0], fileIo.OpenMode.WRITE_ONLY | fileIo.OpenMode.TRUNC);
    try {
      const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
      if (await fileIo.write(file.fd, buffer) !== bytes.byteLength) {
        throw new FileError('file_unwritable');
      }
      await fileIo.fsync(file.fd);
    } catch (error) {
      throw new FileError('file_unwritable');
    } finally {
      await fileIo.close(file);
    }
    return true;
  }

  private async stageAudio(request: Request, single: boolean): Promise<Object | null> {
    const selected = await this.select(['Audio|.' + AUDIO.join(',.')], single ? 1 : 100);
    const uris = Array.from(new Set(selected));
    if (uris.length === 0 || request.cancelled) {
      return null;
    }
    const root = this.stageRoot();
    if (!fileIo.accessSync(root)) {
      fileIo.mkdirSync(root, true);
    }
    for (const entry of fileIo.listFileSync(root)) {
      if (!entry.startsWith(this.process + '-') && new RegExp('^[0-9a-f]{32}-[0-9a-f]{32}$').test(entry)) {
        Files.removeStage(root + '/' + entry);
      }
    }
    const ticket = this.process + '-' + Files.hex();
    const stage = root + '/' + ticket;
    fileIo.mkdirSync(stage);
    request.stage = stage;
    Files.writeText(stage + '/.owner', OWNER);
    const limit = single ? RECOGNITION_LIMIT : AUDIO_LIMIT;
    const rows: string[] = [];
    let total = 0;
    for (let index = 0; index < uris.length; index++) {
      if (request.cancelled) {
        throw new FileError('cancelled');
      }
      const raw = new fileUri.FileUri(uris[index]).name;
      const dot = raw.lastIndexOf('.');
      const extension = dot < 0 ? '' : raw.substring(dot + 1).toLowerCase();
      if (raw.length > 1024 || !AUDIO.includes(extension)) {
        throw new FileError('file_unreadable');
      }
      let stem = '';
      for (const character of raw.substring(0, dot)) {
        stem += character === '/' || character === '\\' || character.charCodeAt(0) < 32 ? '_' : character;
      }
      stem = Array.from(stem).slice(0, 64).join('').trim();
      const name = (stem.length > 0 ? stem : 'audio') + '.' + extension;
      const relative = index + '.' + extension;
      const source = await fileIo.open(uris[index], fileIo.OpenMode.READ_ONLY);
      try {
        const size = (await fileIo.stat(source.fd)).size;
        total += size;
        if (size > limit || total > BATCH_LIMIT) {
          throw new FileError('file_too_large');
        }
        if (size === 0) {
          throw new FileError('file_unreadable');
        }
        await fileIo.copyFile(source.fd, stage + '/' + relative);
      } finally {
        await fileIo.close(source);
      }
      rows.push(JSON.stringify(new StagedFile(relative, name)));
    }
    if (request.cancelled) {
      throw new FileError('cancelled');
    }
    Files.writeText(stage + '/manifest.json', '{"version":1,"files":[' + rows.join(',') + ']}');
    return ticket;
  }

  private stageRoot(): string {
    // Rust looks for stages under the directory Dart's getTemporaryDirectory returns.
    return this.context.tempDir + '/mio-audio-staging';
  }

  private static async named(uri: string, limit: number, extensions: string[]): Promise<Object> {
    const name = new fileUri.FileUri(uri).name;
    const dot = name.lastIndexOf('.');
    if (name.length === 0 || name.length > 1024 || dot < 0 ||
      !extensions.includes(name.substring(dot + 1).toLowerCase())) {
      throw new FileError('file_unreadable');
    }
    const value = new Map<string, Object>();
    value.set('name', name);
    value.set('bytes', await Files.read(uri, limit));
    return value;
  }

  private static async read(uri: string, limit: number): Promise<Uint8Array> {
    const file = await fileIo.open(uri, fileIo.OpenMode.READ_ONLY);
    try {
      const size = (await fileIo.stat(file.fd)).size;
      if (size > limit) {
        throw new FileError('file_too_large');
      }
      if (size === 0) {
        throw new FileError('file_unreadable');
      }
      // `offset` is the file position; every read lands at the start of `chunk`.
      const bytes = new Uint8Array(size);
      const chunk = new ArrayBuffer(Math.min(size, 1024 * 1024));
      let offset = 0;
      while (offset < size) {
        const options: ReadOptions = { offset: offset, length: Math.min(chunk.byteLength, size - offset) };
        const count = await fileIo.read(file.fd, chunk, options);
        if (count <= 0) {
          break;
        }
        bytes.set(new Uint8Array(chunk, 0, count), offset);
        offset += count;
      }
      if (offset !== size) {
        throw new FileError('file_unreadable');
      }
      return bytes;
    } finally {
      await fileIo.close(file);
    }
  }

  private static writeText(path: string, text: string): void {
    const file = fileIo.openSync(path, fileIo.OpenMode.CREATE | fileIo.OpenMode.WRITE_ONLY | fileIo.OpenMode.TRUNC);
    try {
      fileIo.writeSync(file.fd, text);
      fileIo.fsyncSync(file.fd);
    } finally {
      fileIo.closeSync(file);
    }
  }

  /** Deletes only a stage this host wrote, and only the entries it writes. */
  private static removeStage(stage: string): void {
    try {
      const marker = stage + '/.owner';
      if (!fileIo.lstatSync(stage).isDirectory() || !fileIo.lstatSync(marker).isFile() ||
        fileIo.readTextSync(marker) !== OWNER) {
        return;
      }
      const entries = fileIo.listFileSync(stage);
      for (const entry of entries) {
        if (entry !== '.owner' && entry !== 'manifest.json' && !new RegExp('^[0-9]{1,2}\\.[a-z0-9]{2,5}$').test(entry)) {
          return;
        }
      }
      for (const entry of entries) {
        if (entry !== '.owner') {
          fileIo.unlinkSync(stage + '/' + entry);
        }
      }
      fileIo.unlinkSync(marker);
      fileIo.rmdirSync(stage);
    } catch (error) {
      // Cleanup is best effort; the next process removes leftovers.
    }
  }

  private static hex(): string {
    return util.generateRandomUUID(false).split('-').join('').toLowerCase();
  }
}

class StagedFile {
  path: string;
  name: string;

  constructor(path: string, name: string) {
    this.path = path;
    this.name = name;
  }
}
