#!/usr/bin/env python3
"""Restore isolated Android acceptance data and verify it on the device.

After restoring, cold boot the AVD without launching Mio and run --verify-only
against the same archive. A successful pre-shutdown read is not durable proof.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import shlex
import subprocess
import tarfile
import tempfile
import time


def manifest(raw):
    result = {}
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or path.parts[0] != 'files':
                raise ValueError('Archive path is outside files/')
            if not (member.isdir() or member.isfile()):
                raise ValueError('Archive contains a link or special file')
            if member.isfile():
                if len(path.parts) < 2:
                    raise ValueError('Regular file would replace files/')
                if member.name in result:
                    raise ValueError('Archive contains duplicate files')
                result[member.name] = {
                    'size': member.size,
                    'sha256': hashlib.sha256(archive.extractfile(member).read()).hexdigest(),
                }
    if not result:
        raise ValueError('Archive contains no regular files')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--report', required=True, type=Path)
    parser.add_argument('--verify-only', action='store_true')
    args = parser.parse_args()
    package = 'com.vant.mio_music'

    def adb(*words):
        return subprocess.run(
            [args.adb, '-s', args.serial, *words], check=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        ).stdout

    def app(command):
        return adb('shell', 'run-as ' + package + ' ' + command)

    raw = args.archive.read_bytes()
    expected = manifest(raw)
    # Never read or restore files while the application can write them.
    adb('shell', 'am force-stop ' + package)
    report = {
        'serial': args.serial, 'archive': str(args.archive.resolve()),
        'boot_id': adb('shell', 'cat /proc/sys/kernel/random/boot_id').decode().strip(),
        'archive_sha256': hashlib.sha256(raw).hexdigest(),
        'mode': 'verify-only' if args.verify_only else 'restore-and-sync',
        'durability': 'Requires a separate cold-boot verification before completion',
    }
    before_raw = adb('exec-out', 'run-as', package, 'tar', '-cf', '-', 'files')
    before = manifest(before_raw)
    if not args.verify_only:
        # Backups can contain provider credentials; keep them outside evidence/docs.
        before_path = Path(tempfile.mkdtemp(prefix='mio-mobile-restore-')) / 'files.tar'
        before_path.write_bytes(before_raw)
        report['before_archive'] = str(before_path.resolve())
        remote = '/data/local/tmp/mio-restore-' + str(time.time_ns()) + '.tar'
        adb('push', str(args.archive.resolve()), remote)
        try:
            adb('shell', 'chmod 644 ' + remote)
            report['removed_generated_files'] = sorted(set(before) - set(expected))
            for name in report['removed_generated_files']:
                app('rm -- ' + shlex.quote(name))
            app('tar -xf ' + remote)
            # Both the filesystem and the emulator disk must see the restored bytes.
            adb('shell', 'sync')
        finally:
            adb('shell', 'rm -- ' + remote)
    actual = manifest(adb('exec-out', 'run-as', package, 'tar', '-cf', '-', 'files'))
    checks = []
    for name, entry in expected.items():
        # Check device-side hashing independently of tar serialization/readback.
        digest = app('sha256sum ' + shlex.quote(name)).decode().split()[0]
        checks.append({
            'path': name, **entry,
            'matches': actual.get(name) == entry and digest == entry['sha256'],
        })
    report['checks'] = checks
    report['unexpected_regular_files'] = sorted(set(actual) - set(expected))
    report['all_match'] = (
        all(check['matches'] for check in checks)
        and not report['unexpected_regular_files']
    )
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    if not report['all_match']:
        raise SystemExit('Device files do not match the backup; inspect ' + str(args.report))
    print(f'{args.serial}: {len(checks)} files match; {report["mode"]}')


if __name__ == '__main__':
    main()
