"""Bounded Mach-O file identity and address mappings; no game field offsets.

The format follows Apple's xnu EXTERNAL_HEADERS/mach-o/{loader,fat}.h. Parsing
an image is not live layout validation. No Mac HOI4 build is validated here.
"""

import hashlib
import os
import struct
import uuid
from dataclasses import dataclass

from ..errors import CapabilityUnavailable


MAX_IMAGE_BYTES = 1 << 30
MAX_COMMAND_BYTES = 16 << 20
MAX_COMMANDS = 65536
MAX_SLICES = 32
MAX_SEGMENTS = 256
MAX_U64 = (1 << 64) - 1
ARCHITECTURES = {0x01000007: "x86_64", 0x0100000C: "arm64"}


class MachOError(ValueError):
    """Malformed, unsupported, or ambiguous Mach-O image."""


@dataclass(frozen=True)
class Segment:
    name: str
    vm_address: int
    vm_size: int
    file_offset: int  # Relative to the selected Mach-O slice, not the fat file.
    file_size: int
    max_protection: int
    initial_protection: int


@dataclass(frozen=True)
class Slice:
    architecture: str
    cpu_type: int
    cpu_subtype: int
    offset: int
    size: int


def _bounded(offset, size, limit, message):
    if offset < 0 or size < 0 or offset > limit or size > limit - offset:
        raise MachOError(message)


class Image:
    """Static file parser; a fat image requires an explicit target architecture.

    Never select using the helper's own architecture: Rosetta targets may be
    x86_64 while the Python helper is arm64. Whole-file SHA256 plus UUID/arch and
    CPU subtype identify a build; UUID alone is insufficient trust evidence.
    """

    def __init__(self, path, architecture=None):
        self.path = os.fspath(path)
        with open(path, "rb") as handle:
            size = os.fstat(handle.fileno()).st_size
            if not 0 < size <= MAX_IMAGE_BYTES:
                raise MachOError("image exceeds the file size bound")
            data = handle.read(size + 1)
            if len(data) != size:
                raise MachOError("image size changed while reading")
        self._parse(data, architecture)

    @classmethod
    def from_bytes(cls, data, architecture=None):
        image = cls.__new__(cls)
        image.path = None
        image._parse(data, architecture)
        return image

    def _parse(self, data, architecture):
        if not isinstance(data, bytes) or not 32 <= len(data) <= MAX_IMAGE_BYTES:
            raise MachOError("invalid or oversized image")
        if architecture is not None and architecture not in ARCHITECTURES.values():
            raise MachOError("unsupported target architecture")
        self.size = len(data)
        self.sha256 = hashlib.sha256(data).hexdigest()
        self.slices = self._slices(data)
        choices = [entry for entry in self.slices
                   if architecture is None or entry.architecture == architecture]
        if len(choices) != 1:
            raise MachOError("select one unambiguous target architecture")
        selected = choices[0]
        if selected.cpu_type not in ARCHITECTURES:
            raise MachOError("unsupported Mach-O CPU type")
        self.architecture = selected.architecture
        self.cpu_subtype = selected.cpu_subtype
        self.slice_offset, self.slice_size = selected.offset, selected.size
        self._data = data
        start = selected.offset
        magic, cpu, subtype, file_type, ncmds, command_bytes, _flags, _reserved = struct.unpack_from(
            "<8I", data, start)
        if magic != 0xFEEDFACF or cpu != selected.cpu_type or subtype != selected.cpu_subtype:
            raise MachOError("slice architecture disagrees with its Mach-O header")
        if file_type != 2:  # MH_EXECUTE: do not treat dylibs or dumps as the game image.
            raise MachOError("expected a Mach-O executable")
        if not 0 < ncmds <= MAX_COMMANDS or not ncmds * 8 <= command_bytes <= MAX_COMMAND_BYTES:
            raise MachOError("invalid load-command count or size")
        _bounded(32, command_bytes, selected.size, "load commands exceed slice bounds")
        command_end = start + 32 + command_bytes
        cursor = start + 32
        segments = []
        image_uuid = None
        for _ in range(ncmds):
            _bounded(cursor, 8, command_end, "truncated load command")
            command, size = struct.unpack_from("<II", data, cursor)
            if size < 8 or size % 8:
                raise MachOError("invalid 64-bit load-command size")
            _bounded(cursor, size, command_end, "load command exceeds declared command bytes")
            if command == 0x1B:  # LC_UUID
                if size != 24 or image_uuid is not None:
                    raise MachOError("invalid or duplicate UUID command")
                image_uuid = str(uuid.UUID(bytes=data[cursor + 8:cursor + 24]))
            elif command == 0x19:  # LC_SEGMENT_64
                if len(segments) >= MAX_SEGMENTS:
                    raise MachOError("segment count exceeds the parser bound")
                if size < 72:
                    raise MachOError("truncated segment command")
                name, vmaddr, vmsize, fileoff, filesize, maxprot, initprot, nsects, _ = struct.unpack_from(
                    "<16sQQQQiiII", data, cursor + 8)
                if size != 72 + nsects * 80:
                    raise MachOError("invalid segment section bounds")
                _bounded(vmaddr, vmsize, MAX_U64 + 1, "segment virtual range overflows")
                _bounded(fileoff, filesize, selected.size, "segment file range exceeds slice")
                if filesize > vmsize:
                    raise MachOError("segment file size exceeds virtual size")
                if maxprot & ~7 or initprot & ~7 or initprot & ~maxprot:
                    raise MachOError("invalid segment protections")
                try:
                    decoded = name.split(b"\0", 1)[0].decode("ascii")
                except UnicodeDecodeError as exc:
                    raise MachOError("invalid segment name") from exc
                segment = Segment(decoded, vmaddr, vmsize, fileoff, filesize, maxprot, initprot)
                for previous in segments:
                    if vmsize and previous.vm_size and (vmaddr < previous.vm_address + previous.vm_size
                                                        and previous.vm_address < vmaddr + vmsize):
                        raise MachOError("overlapping virtual segments")
                    if filesize and previous.file_size and (fileoff < previous.file_offset + previous.file_size
                                                            and previous.file_offset < fileoff + filesize):
                        raise MachOError("overlapping file segments")
                segments.append(segment)
            cursor += size
        if cursor != command_end:
            raise MachOError("load-command size does not match header")
        if image_uuid is None or image_uuid == str(uuid.UUID(int=0)):
            raise MachOError("a nonzero image UUID is required")
        if not any(s.file_offset == 0 and s.file_size >= command_end - start
                   and s.initial_protection & 4 for s in segments):
            raise MachOError("executable header segment is missing")
        self.uuid = image_uuid
        self.segments = tuple(segments)
        self.build_key = "macho:%s:%x:%s:%s" % (self.architecture, self.cpu_subtype, self.uuid, self.sha256)

    @staticmethod
    def _slices(data):
        magic = data[:4]
        fat_formats = {b"\xca\xfe\xba\xbe": (">", False), b"\xbe\xba\xfe\xca": ("<", False),
                       b"\xca\xfe\xba\xbf": (">", True), b"\xbf\xba\xfe\xca": ("<", True)}
        if magic not in fat_formats:
            if magic != b"\xcf\xfa\xed\xfe":
                raise MachOError("only little-endian 64-bit Mach-O targets are supported")
            cpu, subtype = struct.unpack_from("<II", data, 4)
            if cpu not in ARCHITECTURES:
                raise MachOError("unsupported Mach-O CPU type")
            return (Slice(ARCHITECTURES[cpu], cpu, subtype, 0, len(data)),)
        endian, wide = fat_formats[magic]
        count = struct.unpack_from(endian + "I", data, 4)[0]
        if not 0 < count <= MAX_SLICES:
            raise MachOError("invalid fat slice count")
        entry_size = 32 if wide else 20
        table_end = 8 + count * entry_size
        _bounded(8, count * entry_size, len(data), "truncated fat slice table")
        slices = []
        for index in range(count):
            values = struct.unpack_from(endian + ("IIQQII" if wide else "IIIII"), data,
                                        8 + index * entry_size)
            cpu, subtype, offset, size, alignment = values[:5]
            if wide and values[5] != 0:
                raise MachOError("fat slice reserved field is nonzero")
            if alignment > 31 or offset % (1 << alignment) or offset < table_end or size < 32:
                raise MachOError("invalid fat slice alignment or range")
            _bounded(offset, size, len(data), "fat slice exceeds file bounds")
            for previous in slices:
                if offset < previous.offset + previous.size and previous.offset < offset + size:
                    raise MachOError("overlapping fat slices")
            slices.append(Slice(ARCHITECTURES.get(cpu, "unknown:%x" % cpu), cpu, subtype, offset, size))
        return tuple(slices)

    def vm_to_file_offset(self, address, size=1):
        if type(address) is not int or type(size) is not int or size <= 0:
            raise MachOError("invalid mapped range")
        for segment in self.segments:
            delta = address - segment.vm_address
            if 0 <= delta < segment.file_size and size <= segment.file_size - delta:
                return self.slice_offset + segment.file_offset + delta
        raise MachOError("range is not entirely file-backed in one segment")

    def require_validated_build(self, validated_build_keys=()):
        """Gate field decoding on a separately maintained, live-validated registry.

        The default registry is intentionally empty. File parsing alone never
        authorizes field reads; session ownership and freshness are also required.
        """
        if self.build_key not in validated_build_keys:
            raise CapabilityUnavailable("memory layout", "macos", "this Mach-O build has no validated layout",
                                        remedy="Continue with observational telemetry and UI.")
        return self.build_key
