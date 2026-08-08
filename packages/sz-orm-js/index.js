/* eslint-disable @typescript-eslint/no-var-requires */
const { existsSync } = require('fs')
const { join } = require('path')

function loadNative() {
  const candidates = [
    join(__dirname, 'core.node'),
    join(__dirname, 'index.node'),
    join(__dirname, 'sz_orm_js.node'),
  ]

  for (const path of candidates) {
    if (existsSync(path)) {
      return require(path)
    }
  }

  try {
    require('@sz-orm/core-win32-x64-msvc')
  } catch {}
  try {
    require('@sz-orm/core-linux-x64-gnu')
  } catch {}
  try {
    require('@sz-orm/core-darwin-x64')
  } catch {}

  throw new Error('Cannot find native module. Please run `napi build` first.')
}

module.exports = loadNative()