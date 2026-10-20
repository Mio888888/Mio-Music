import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mio_music/src/rust/api.dart' as api;
import 'package:mio_music/src/rust/frb_generated.dart';
import 'package:mio_music/ui/eq_response.dart';
import 'package:mio_music/ui/player_spectrum.dart';
import 'package:mio_music/ui/resources.dart';
import 'package:mio_music/ui/visual_policy.dart';

api.SpectrumSnapshot spectrumFrame({
  int generation = 7,
  int age = 0,
  double db = -20,
}) => api.SpectrumSnapshot(
  ageMs: age,
  generation: generation,
  playing: true,
  fresh: age <= 200,
  levels: Float64List.fromList(List.filled(128, db)),
);

class SpectrumSession extends Fake implements api.PlayerSpectrumSession {
  Future<api.SpectrumSnapshot> Function() read = () async => spectrumFrame();
  int reads = 0;
  bool closed = false, disposed = false;

  @override
  Future<api.SpectrumSnapshot> frame() {
    reads++;
    return read();
  }

  @override
  void close() => closed = true;

  @override
  void dispose() => disposed = true;
}

class SpectrumApi extends Fake implements RustLibApi {
  late Future<api.PlayerSpectrumSession> Function(int) open;
  int opens = 0;

  @override
  Future<api.PlayerSpectrumSession> crateApiBeginPlayerSpectrum({
    required int generation,
  }) {
    opens++;
    return open(generation);
  }
}

class EqualizerSession extends Fake implements api.EqualizerSession {
  int reads = 0;
  bool active = false;

  @override
  void spectrumActive({required bool active}) => this.active = active;

  @override
  Future<api.SpectrumSnapshot> spectrum() async {
    reads++;
    if (reads == 1) throw StateError('busy: command queue is full');
    return spectrumFrame();
  }
}

class Palette extends Fake implements MioPalette {
  @override
  bool get english => true;
  @override
  Color color(String key) => const Color(0xff888888);
  @override
  Color get secondary => color('secondary');
  @override
  Color get border => color('border');
  @override
  Color get brand => color('brand');
}

Widget host(Widget child, {bool enabled = true}) => MediaQuery(
  data: const MediaQueryData(),
  child: Directionality(
    textDirection: TextDirection.ltr,
    child: MioVisualPolicy(
      settings: null,
      child: TickerMode(enabled: enabled, child: child),
    ),
  ),
);

Widget player(List<double> bass, {bool enabled = true, bool playing = true}) =>
    host(
      PlayerSpectrum(
        generation: 7,
        playing: playing,
        wide: false,
        accent: const Color(0xffffffff),
        backgroundResponse: true,
        onLowFrequency: bass.add,
      ),
      enabled: enabled,
    );

Future<void> remove(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox.shrink());
  await tester.pump();
}

void main() {
  late SpectrumApi bridge;
  setUp(() {
    bridge = SpectrumApi();
    RustLib.initMock(api: bridge);
  });
  tearDown(RustLib.dispose);

  for (final reason in [
    'busy: command queue is full',
    'busy: too many retained catalog snapshots',
  ]) {
    testWidgets('opening recovers from $reason without a track change', (
      tester,
    ) async {
      final session = SpectrumSession();
      final bass = <double>[];
      bridge.open = (_) async {
        if (bridge.opens == 1) throw StateError(reason);
        return session;
      };
      await tester.pumpWidget(player(bass));
      await tester.pump();
      expect(bridge.opens, 1);
      expect(bass.where((value) => value > 0), isEmpty);
      await tester.pump(const Duration(milliseconds: 125));
      expect(bridge.opens, 2);
      expect(bass.last, .75);
      await remove(tester);
      expect(session.disposed, isTrue);
    });
  }

  for (final reason in [
    'busy: command queue is full',
    'busy: spectrum worker',
  ]) {
    testWidgets('sampling recovers from $reason using the same session', (
      tester,
    ) async {
      final session = SpectrumSession();
      final bass = <double>[];
      session.read = () async {
        if (session.reads == 1) throw StateError(reason);
        return spectrumFrame();
      };
      bridge.open = (_) async => session;
      await tester.pumpWidget(player(bass));
      await tester.pump();
      expect(session.reads, 1);
      await tester.pump(const Duration(milliseconds: 125));
      expect(session.reads, 2);
      expect(bridge.opens, 1);
      expect(session.closed, isFalse);
      expect(bass.last, .75);
      await remove(tester);
    });
  }

  testWidgets('repeated opening failures back off and stop when hidden', (
    tester,
  ) async {
    final bass = <double>[];
    bridge.open = (_) async => throw StateError('busy: command queue is full');
    await tester.pumpWidget(player(bass));
    await tester.pump();
    expect(bridge.opens, 1);
    for (final delay in [125, 250, 500, 1000, 1000]) {
      final attempts = bridge.opens;
      await tester.pump(Duration(milliseconds: delay - 1));
      expect(bridge.opens, attempts);
      await tester.pump(const Duration(milliseconds: 1));
      expect(bridge.opens, attempts + 1);
    }
    await tester.pumpWidget(player(bass, enabled: false));
    final attempts = bridge.opens;
    await tester.pump(const Duration(seconds: 5));
    expect(bridge.opens, attempts);
    final session = SpectrumSession();
    bridge.open = (_) async => session;
    await tester.pumpWidget(player(bass));
    await tester.pump();
    expect(bass.last, .75);
    await remove(tester);
    final reads = session.reads;
    await tester.pump(const Duration(seconds: 5));
    expect(session.reads, reads);
  });

  testWidgets(
    'long background stay rejects a late frame and reopens on resume',
    (tester) async {
      final pending = Completer<api.SpectrumSnapshot>();
      final old = SpectrumSession()..read = () => pending.future;
      final current = SpectrumSession();
      final bass = <double>[];
      bridge.open = (_) async => bridge.opens == 1 ? old : current;
      await tester.pumpWidget(player(bass));
      await tester.pump();
      expect(old.reads, 1);
      for (final state in [
        AppLifecycleState.inactive,
        AppLifecycleState.hidden,
        AppLifecycleState.paused,
      ]) {
        tester.binding.handleAppLifecycleStateChanged(state);
      }
      await tester.pump(const Duration(hours: 12));
      expect(old.closed, isTrue);
      expect(bridge.opens, 1);
      for (final state in [
        AppLifecycleState.hidden,
        AppLifecycleState.inactive,
        AppLifecycleState.resumed,
      ]) {
        tester.binding.handleAppLifecycleStateChanged(state);
      }
      await tester.pump();
      expect(bridge.opens, 1);
      pending.complete(spectrumFrame(db: 0));
      await tester.pump();
      expect(old.disposed, isTrue);
      expect(bridge.opens, 2);
      expect(current.reads, 1);
      expect(bass.where((value) => value > 0), [.75]);
      await remove(tester);
    },
  );

  testWidgets('late opening is disposed before the resumed session starts', (
    tester,
  ) async {
    final pending = Completer<api.PlayerSpectrumSession>();
    final old = SpectrumSession();
    final current = SpectrumSession();
    final bass = <double>[];
    bridge.open = (_) =>
        bridge.opens == 1 ? pending.future : Future.value(current);
    await tester.pumpWidget(player(bass));
    await tester.pump();
    await tester.pumpWidget(player(bass, enabled: false));
    await tester.pumpWidget(player(bass));
    pending.complete(old);
    await tester.pump();
    expect(old.closed, isTrue);
    expect(old.disposed, isTrue);
    expect(old.reads, 0);
    expect(current.reads, 1);
    expect(bass.last, .75);
    await remove(tester);
  });

  testWidgets('expired frames and terminal session errors are not retried', (
    tester,
  ) async {
    final session = SpectrumSession();
    final bass = <double>[];
    session.read = () async {
      if (session.reads == 1) return spectrumFrame(age: 201);
      throw StateError('player_changed');
    };
    bridge.open = (_) async => session;
    await tester.pumpWidget(player(bass));
    await tester.pump();
    expect(bass.where((value) => value > 0), isEmpty);
    await tester.pump(const Duration(milliseconds: 34));
    expect(session.closed, isTrue);
    await tester.pump(const Duration(seconds: 5));
    expect(session.reads, 2);
    expect(bridge.opens, 1);
    await remove(tester);
  });

  testWidgets(
    'equalizer keeps polling after temporary command queue congestion',
    (tester) async {
      final session = EqualizerSession();
      await tester.pumpWidget(
        host(
          EqResponsePlot(
            session: session,
            active: true,
            playing: true,
            generation: 7,
            value: const api.EqSnapshot(
              revision: 1,
              enabled: false,
              globalGain: 0,
              response: [],
              referenceRate: 44100,
              bands: [],
              currentPresetId: '',
              presets: [],
            ),
            palette: Palette(),
            wide: false,
            selected: null,
          ),
        ),
      );
      await tester.pump(Duration.zero);
      expect(session.reads, 1);
      await tester.pump(const Duration(milliseconds: 125));
      expect(session.reads, 2);
      expect(session.active, isTrue);
      expect(find.textContaining('Reopen this page'), findsNothing);
      await remove(tester);
      expect(session.active, isFalse);
    },
  );
}
