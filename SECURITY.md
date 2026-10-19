# Security Policy

**English** · [简体中文](SECURITY.zh-CN.md)

## Supported versions

Only the latest release receives security fixes. The `0.x` releases belong to the previous Tauri app and are no longer maintained here.

| Version | Supported |
|---|---|
| 1.0.x (latest) | Yes |
| 0.x | No |

## Reporting a vulnerability

**Please do not open a public issue for a security problem.** Report it privately instead:

1. Go to the [Security tab](https://github.com/Mio888888/Mio-Music/security) and choose **Report a vulnerability** ([direct link](https://github.com/Mio888888/Mio-Music/security/advisories/new)).
2. If that option is not available, open an issue titled "Security contact" that contains **no details**, and ask for a private channel.

Please include the affected version and platform, the steps to reproduce, and what an attacker could do. A proof of concept helps; do not include other people's data.

This is a volunteer project. We will acknowledge a report as soon as we can, usually within a week, keep you informed while we work on it, and credit you in the release notes if you wish. Please give us reasonable time to release a fix before you disclose the problem publicly.

## What is in scope

- Escaping the plugin sandbox, or a plugin reaching files, processes or modules it is not given.
- Bypassing the signature check of the auto-updater, or installing an unsigned update.
- Weaknesses in backup encryption or in how credentials and tokens are stored or sent.
- Reading or writing files outside the folders the app is allowed to use, for example through the file pickers or the audio import staging.
- Crashes or memory-safety problems reachable from untrusted input such as audio files, lyrics, plugin responses or backup files.

## What is out of scope

- The legality of content that third-party plugins or servers provide.
- Problems in third-party servers or plugins that are not part of this repository.
- Findings that need a rooted or jailbroken device, or physical access to an unlocked one.
- Plugin configuration, including passwords, is stored in `list-plugins.json` on the device and is not encrypted by the app. This is a known design limit; protect the device with the operating system's disk encryption.
- Unsigned development builds, and the experimental HarmonyOS branch.
