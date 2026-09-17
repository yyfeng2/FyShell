// 生成占位应用图标：32x32 深蓝底色 PNG，封装进 ICO（Vista+ 支持 PNG-in-ICO）
// 用法：node scripts/gen-icon.js
import { deflateSync } from 'node:zlib'
import { writeFileSync, mkdirSync } from 'node:fs'

// ---------- CRC32 ----------
const CRC_TABLE = (() => {
  const t = new Uint32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    t[n] = c >>> 0
  }
  return t
})()

function crc32(buf) {
  let c = 0xffffffff
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}

// ---------- PNG ----------
function chunk(type, data) {
  const len = Buffer.alloc(4)
  len.writeUInt32BE(data.length)
  const body = Buffer.concat([Buffer.from(type), data])
  const crc = Buffer.alloc(4)
  crc.writeUInt32BE(crc32(body))
  return Buffer.concat([len, body, crc])
}

const SIZE = 32
// RGBA 像素：深蓝底 (#1e3a5f) + 中心浅色方块，简单占位设计
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1)) // 每行前有 filter 字节
for (let y = 0; y < SIZE; y++) {
  const rowStart = y * (SIZE * 4 + 1)
  raw[rowStart] = 0 // filter: none
  for (let x = 0; x < SIZE; x++) {
    const i = rowStart + 1 + x * 4
    const inCenter = x >= 8 && x < 24 && y >= 8 && y < 24
    const inInner = x >= 12 && x < 20 && y >= 12 && y < 20
    raw[i] = inCenter ? 0xf0 : 0x1e // R
    raw[i + 1] = inInner ? 0xff : 0x3a // G
    raw[i + 2] = inInner ? 0xff : 0x5f // B
    raw[i + 3] = 0xff // A
  }
}

const ihdr = Buffer.alloc(13)
ihdr.writeUInt32BE(SIZE, 0)
ihdr.writeUInt32BE(SIZE, 4)
ihdr[8] = 8 // bit depth
ihdr[9] = 6 // color type: RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw)),
  chunk('IEND', Buffer.alloc(0)),
])

// ---------- ICO（内嵌单张 PNG） ----------
const icondir = Buffer.alloc(6)
icondir.writeUInt16LE(0, 0) // reserved
icondir.writeUInt16LE(1, 2) // type: icon
icondir.writeUInt16LE(1, 4) // count

const entry = Buffer.alloc(16)
entry[0] = SIZE // width
entry[1] = SIZE // height
entry[2] = 0 // colors
entry[3] = 0 // reserved
entry.writeUInt16LE(1, 4) // planes
entry.writeUInt16LE(32, 6) // bit count
entry.writeUInt32LE(png.length, 8) // bytes in resource
entry.writeUInt32LE(6 + 16, 12) // image offset

mkdirSync('icons', { recursive: true })
writeFileSync('icons/icon.ico', Buffer.concat([icondir, entry, png]))
writeFileSync('icons/icon.png', png)
console.log('icons/icon.ico 与 icons/icon.png 已生成')
