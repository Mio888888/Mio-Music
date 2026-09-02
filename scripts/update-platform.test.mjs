import test from 'node:test'
import assert from 'node:assert/strict'
import { isDesktopUpdatePlatform } from '../src/utils/updatePlatform.ts'

test('only desktop platforms support the Tauri updater', () => {
  assert.equal(isDesktopUpdatePlatform('linux'), true)
  assert.equal(isDesktopUpdatePlatform('macos'), true)
  assert.equal(isDesktopUpdatePlatform('windows'), true)
  assert.equal(isDesktopUpdatePlatform('android'), false)
  assert.equal(isDesktopUpdatePlatform('ios'), false)
  assert.equal(isDesktopUpdatePlatform(null), false)
})
