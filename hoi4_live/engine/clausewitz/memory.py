"""Portable typed reads derived from smerss/Clausewitz-MCP calling/mem.py.

Upstream commit: 97c85437081904d5ad07bae1623ec58d1c0a5b9e.
Copyright (c) 2026 smerss. MIT license: see the adjacent LICENSE file.

Local adaptation: extract typed primitives from Proc. Platforms implement exact,
bounded, ownership-checked read(); this layer neither discovers processes nor
opens native handles. No write, injector or ABI-specific string decoder is exposed.
"""
import struct


class ReadPrimitives:
    """Clausewitz typed-read API shared by Windows, macOS and Linux adapters."""

    def read(self, address, size):
        raise NotImplementedError("the platform must supply an exact guarded read")

    def u8(self, a): return self.read(a, 1)[0]
    def u16(self, a): return struct.unpack("<H", self.read(a, 2))[0]
    def u32(self, a): return struct.unpack("<I", self.read(a, 4))[0]
    def i32(self, a): return struct.unpack("<i", self.read(a, 4))[0]
    def u64(self, a): return struct.unpack("<Q", self.read(a, 8))[0]
    def i64(self, a): return struct.unpack("<q", self.read(a, 8))[0]
    def f32(self, a): return struct.unpack("<f", self.read(a, 4))[0]
    def f64(self, a): return struct.unpack("<d", self.read(a, 8))[0]
