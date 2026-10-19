import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart' show TextInputAction;

import '../core_controller.dart';
import '../src/rust/api.dart' as api;
import 'controls.dart';
import 'resources.dart';

class PlaylistMetadataForm extends StatefulWidget {
  final String? playlistId;
  final MioPalette palette;
  final CoreController core;
  final VoidCallback onClose;
  final VoidCallback onSaved;
  const PlaylistMetadataForm({
    super.key,
    this.playlistId,
    required this.palette,
    required this.core,
    required this.onClose,
    required this.onSaved,
  });
  @override
  State<PlaylistMetadataForm> createState() => _PlaylistMetadataFormState();
}

class _PlaylistMetadataFormState extends State<PlaylistMetadataForm> {
  final _name = TextEditingController();
  final _description = TextEditingController();
  final _nameFocus = FocusNode();
  final _descriptionFocus = FocusNode();
  api.PlaylistEditor? _editor;
  bool _loading = true;
  bool _saving = false;
  String? _error;
  MioPalette get p => widget.palette;

  @override
  void initState() {
    super.initState();
    unawaited(_open());
  }

  Future<void> _open() async {
    api.PlaylistEditor? editor;
    try {
      editor = await api.beginPlaylistEditor(id: widget.playlistId);
      final draft = await editor.draft();
      if (!mounted) {
        editor.dispose();
        return;
      }
      _editor = editor;
      _name.text = draft.name;
      _description.text = draft.description;
      setState(() => _loading = false);
      _nameFocus.requestFocus();
    } catch (_) {
      editor?.dispose();
      if (mounted) {
        setState(() {
          _loading = false;
          _error = p.english ? 'Unable to open playlist.' : '无法打开歌单';
        });
      }
    }
  }

  Future<void> _save() async {
    final editor = _editor;
    if (_saving || _loading || editor == null) return;
    final name = _name.text.trim();
    final description = _description.text.trim();
    // Rust remains authoritative for desktop-compatible Unicode limits.
    if (name.isEmpty) {
      setState(() => _error = p.text('music.songlist.playlistNamePlaceholder'));
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    api.PlaylistInfo? saved;
    await widget.core.command(() async {
      saved = await editor.save(name: name, description: description);
    });
    if (!mounted) return;
    if (saved != null) {
      widget.onSaved();
    } else {
      setState(() {
        _saving = false;
        _error = p.english
            ? 'Save failed. Check the name (50) and description (200) limits. If the playlist changed, reopen this form.'
            : '保存失败。请检查名称（50）与描述（200）的长度限制；歌单已变更时请重新打开表单。';
      });
    }
  }

  @override
  void dispose() {
    _editor?.dispose();
    _name.dispose();
    _description.dispose();
    _nameFocus.dispose();
    _descriptionFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => SingleChildScrollView(
    padding: const EdgeInsets.symmetric(vertical: 12),
    child: Align(
      alignment: Alignment.topLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              p.text(
                widget.playlistId == null
                    ? 'music.songlist.createNewPlaylist'
                    : 'music.songlist.editPlaylistInfo',
              ),
            ),
            const SizedBox(height: 16),
            if (_loading) Text(p.english ? 'Loading…' : '加载中…'),
            if (!_loading && _editor != null) ...[
              Text('${p.text('music.songlist.playlistName')} · 50'),
              const SizedBox(height: 8),
              MioInput(
                palette: p,
                controller: _name,
                focusNode: _nameFocus,
                placeholder: p.text('music.songlist.playlistNamePlaceholder'),
                readOnly: _saving,
                height: 44,
                textInputAction: TextInputAction.next,
                onSubmitted: (_) => _descriptionFocus.requestFocus(),
              ),
              const SizedBox(height: 16),
              Text('${p.text('music.songlist.playlistDesc')} · 200'),
              const SizedBox(height: 8),
              MioInput(
                palette: p,
                controller: _description,
                focusNode: _descriptionFocus,
                placeholder: p.text('music.songlist.playlistDescPlaceholder'),
                readOnly: _saving,
                maxLines: 4,
                height: 112,
                keyboardType: TextInputType.multiline,
                textInputAction: TextInputAction.newline,
              ),
              const SizedBox(height: 16),
            ],
            if (_error != null) ...[
              Text(
                _error!,
                style: TextStyle(color: p.color('--td-error-color')),
              ),
              const SizedBox(height: 12),
            ],
            Wrap(
              spacing: 12,
              runSpacing: 8,
              children: [
                MioButton(
                  palette: p,
                  label: p.text('common.cancel'),
                  onPressed: _saving ? null : widget.onClose,
                  child: Text(p.text('common.cancel')),
                ),
                MioButton(
                  palette: p,
                  label: p.text('common.save'),
                  onPressed: _loading || _saving || _editor == null
                      ? null
                      : () => unawaited(_save()),
                  background: p.brand.withValues(alpha: .12),
                  child: Text(
                    _saving
                        ? (p.english ? 'Saving…' : '保存中…')
                        : p.text('common.save'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    ),
  );
}
