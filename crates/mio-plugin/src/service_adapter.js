// Legacy Mio service CommonJS contract, executed only in the existing bounded host.
function __adaptLegacyService(plugin, fallback) {
  if (plugin && plugin.listPlugin) return plugin;
  if (!plugin || typeof plugin !== 'object' ||
      !(Array.isArray(plugin.configSchema) || plugin.pluginType === 'service' ||
        (typeof plugin.getPlaylists === 'function' && typeof plugin.getPlaylistSongs === 'function'))) {
    throw Error('Not a list or legacy service plugin');
  }
  const info = Object.assign({}, fallback, plugin.pluginInfo || {});
  if (!info.name) throw Error('Legacy service plugin has no name');
  // A stable fallback ID for services without a declared source; updates keep song identity.
  let hash = 2166136261;
  for (let i = 0; i < info.name.length; i++) hash = Math.imul(hash ^ info.name.charCodeAt(i), 16777619);
  const fallbackSource = 'service_' + (hash >>> 0).toString(16).padStart(8, '0');
  const declared = plugin.sources && typeof plugin.sources === 'object' ? plugin.sources : {};
  const sourceIds = Object.keys(declared);
  if (!sourceIds.length) sourceIds.push(fallbackSource);
  const sources = {};
  const has = method => typeof plugin[method] === 'function';
  const canImport = has('getPlaylists') && has('getPlaylistSongs');
  for (const id of sourceIds) {
    const source = declared[id] || {};
    const capabilities = [];
    if (canImport) capabilities.push('userPlaylists', 'playlistDetail');
    if (has('testConnection') || has('ping')) capabilities.push('testConnection');
    if (has('musicUrl')) capabilities.push('play', 'download');
    sources[id] = {name: source.name || info.name, icon: source.icon || info.icon, capabilities,
      qualities: source.qualitys || source.qualities || ['128k', '320k', 'flac']};
  }
  // Original Worker accepts sync, Promise and Node-style callback methods.
  function invoke(method, args) {
    const fn = plugin[method];
    if (typeof fn !== 'function') return Promise.reject(Error('Missing service method'));
    return new Promise((resolve, reject) => {
      const callback = (error, result) => error ? reject(Error('Service method failed')) : resolve(result);
      const expectsCallback = fn.length > args.length;
      try {
        const result = expectsCallback
          ? fn.call({cerumusic}, ...args, callback) : fn.call({cerumusic}, ...args);
        if (result && typeof result.then === 'function') result.then(resolve, reject);
        else if (!expectsCallback || result !== undefined) resolve(result);
      } catch (error) { reject(error); }
    });
  }
  function succeeded(result) {
    if (result && typeof result === 'object') {
      if (typeof result.success === 'boolean') return result.success;
      if (result.error) return false;
    }
    return !!result;
  }
  async function connection() {
    const result = await invoke(has('testConnection') ? 'testConnection' : 'ping', [cerumusic.config]);
    if (!succeeded(result)) throw Error('Service connection failed');
    return {success: true};
  }
  // The native host recreates JS contexts. Restore config-dependent closure/login state per call.
  async function prepare() {
    await plugin.ready;
    if (has('testConnection') || has('ping')) await connection();
  }
  function array(result, key) {
    if (result && result.success === false) throw Error('Service request failed');
    if (Array.isArray(result)) return result;
    if (result && Array.isArray(result[key])) return result[key];
    throw Error('Invalid service response');
  }
  function id(value) {
    if ((typeof value === 'string' && value) || typeof value === 'number' && Number.isFinite(value)) return value;
    throw Error('Invalid service ID');
  }
  function text(value) { return typeof value === 'string' ? value : ''; }
  const listPlugin = Object.assign({}, info, {
    __legacyService: true,
    sources, config: plugin.configSchema || [],
    defaults: {play: has('musicUrl'), download: has('musicUrl')},
    testConnection: connection,
    async getPlaylists(source, {page, limit}) {
      await prepare();
      const all = array(await invoke('getPlaylists', []), 'playlists');
      if (all.length > 10000) throw Error('Too many service playlists');
      const list = all.map(p => ({
        id: id(p.id), name: text(p.name), img: text(p.coverImg), source,
        desc: text(p.description), total: Number.isFinite(p.songCount) ? p.songCount : 0,
      }));
      return {list, allPage: 1, limit: all.length, total: all.length, source};
    },
    async importSongs(source, {id: playlistId}) {
      await prepare();
      const all = array(await invoke('getPlaylistSongs', [playlistId]), 'songs');
      if (all.length > 40000) throw Error('Too many service songs');
      return all.map(song => {
        if (!song || typeof song !== 'object') throw Error('Invalid service song');
        const result = Object.assign({}, song, {songmid: id(song.songmid),
          name: text(song.name), singer: text(song.singer), albumName: text(song.albumName), img: text(song.img)});
        delete result._mioGpuiSource; // Never accept internal server/session identities from a script.
        if (has('musicUrl')) {
          // typeUrl survives the MusicItem cache/download round trip; unknown top-level keys do not.
          result.typeUrl = Object.assign({}, song.typeUrl && typeof song.typeUrl === 'object' ? song.typeUrl : {},
            {_legacyServiceSource: text(song.source) || source});
          result.source = source;
        } else {
          result.source = text(song.source);
        }
        return result;
      });
    },
    async playlistDetail(source, {id: playlistId, page, limit}) {
      await prepare();
      const all = array(await invoke('getPlaylistSongs', [playlistId]), 'songs');
      if (all.length > 40000) throw Error('Too many service songs');
      const list = all.slice((page - 1) * limit, page * limit).map(song => {
        if (!song || typeof song !== 'object') throw Error('Invalid service song');
        return Object.assign({}, song, {songmid: id(song.songmid), source,
          name: text(song.name), singer: text(song.singer), albumName: text(song.albumName), img: text(song.img)});
      });
      return {list, info: {}, allPage: Math.ceil(all.length / limit), limit, total: all.length, source};
    },
    async musicUrl(source, {song, quality}) {
      await prepare();
      const originalSource = text(song.typeUrl && song.typeUrl._legacyServiceSource) || source;
      const result = await invoke('musicUrl', [originalSource, Object.assign({}, song, {source: originalSource}), quality]);
      if (result && result.error) throw Error('Service playback failed');
      return typeof result === 'string' ? result : result && result.url;
    },
  });
  listPlugin.downloadUrl = listPlugin.musicUrl;
  return {ready: plugin.ready, listPlugin};
}
