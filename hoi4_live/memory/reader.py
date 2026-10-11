"""A bounded snapshot of verified, player-visible Windows memory fields.

This does not discover addresses or infer missing offsets. Structural witnesses
are read again at the end; a changed root, object, array or played identity rejects
the whole sample. The verified hour counter brackets collection, but numeric
values can still change within that hour; this is not an atomic game tick.
"""

import struct

from .layout import UnsupportedBuildError, get_layout
from .winproc import InvalidMemoryError, MemoryReadError, checked_range


MAX_COUNTRIES = 8192
MAX_DIVISIONS = 16384
# Operational read bound, not a claimed maximum allowed by the game/mods.
MAX_RESEARCH_SLOTS = 64
MAX_STRING_BYTES = 1024
MAX_SNAPSHOT_BYTES = 2 << 20
MAX_SNAPSHOT_READS = 100000


class TornReadError(MemoryReadError):
    """The structural witnesses changed while the snapshot was read."""


class PlayedCountryUnavailable(MemoryReadError):
    """The played country cannot be determined without a guess."""


LIMITATIONS = (
    "Only fields verified by the Windows handoff or static PE evidence are available.",
    "Numeric fields are sampled across reads; no atomic game-tick guarantee.",
    "The statically verified date decoder has not been validated against a live game.",
    "Stability and war support are base fractions, not the final modified UI values.",
)
UNAVAILABLE = {
    "speed": "display-speed conversion from verified internal speed is unverified",
    "research_progress": "Windows research progress semantics are unverified",
    "research_technology_ids": "Windows technology identifier fields are unverified",
    "focus": "Windows focus fields are unverified",
    "construction": "Windows construction fields are unverified",
    "deployment": "Windows deployment fields are unverified",
    "equipment": "Windows equipment fields are unverified",
    "wars": "Windows war fields are unverified",
    "supply": "Windows supply fields are unverified",
    "events": "Windows event fields are unverified",
    "country_tag": "Windows country tag lookup is unverified",
    "production_lines": "Windows production line fields are unverified",
}

DATE_EPOCH_HOURS = 5000 * 365 * 24
MONTH_LENGTHS = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)


def decode_date(raw):
    """Decode CDate's signed int32 hours using the game's 365-day calendar.

    Static evidence: formatter RVA 0x526f20 reads CDate+8 and subtracts
    0x29c55c0; date getters 0x177140/0x176db0 divide by 24/365.
    """
    if type(raw) is not int or not -(1 << 31) <= raw < (1 << 31):
        raise InvalidMemoryError("invalid encoded game date")
    hours = raw - DATE_EPOCH_HOURS
    day_number, hour = divmod(hours, 24)
    year, day_of_year = divmod(day_number, 365)
    if not 1 <= year <= 9999:
        raise InvalidMemoryError("encoded game date year is out of bounds")
    remaining = day_of_year
    for month, length in enumerate(MONTH_LENGTHS, 1):
        if remaining < length:
            day = remaining + 1
            break
        remaining -= length
    return {"year": year, "month": month, "day": day, "hour": hour,
            "day_number": day_number, "date": "%d.%d.%d.%d" % (year, month, day, hour)}


class MemoryReader:
    def __init__(self, process, module_base, build_key, rtti, *, module_size=None):
        self.layout = get_layout(build_key)  # reject unknown builds before any read
        checked_range(module_base, 1)
        if module_size is not None:
            checked_range(module_base, module_size, 1 << 30)
        self.process = process
        self.module_base = module_base
        self.module_size = module_size
        self.build_key = build_key
        self._vtables = {}
        for name in ("CCurrentGameState", "CInGameIdler", "CSession", "CCountry",
                     "CProductionStatus", "CPoliticalStatus", "CArmy",
                     "CTechnologyStatus", "CResearchSlot", "CTechnology"):
            entries = rtti.get(".?AV%s@@" % name, ())
            valid = set()
            for entry in entries:
                if not isinstance(entry, (tuple, list)) or len(entry) != 2:
                    raise InvalidMemoryError("malformed RTTI entry")
                rva, offset = entry
                if type(rva) is not int or type(offset) is not int or rva < 0 or rva % 8:
                    raise InvalidMemoryError("malformed RTTI vtable RVA")
                if module_size is not None and rva + 8 > module_size:
                    raise InvalidMemoryError("RTTI vtable lies outside executable module")
                if offset == 0:
                    checked_range(module_base + rva, 8)
                    valid.add(module_base + rva)
            if not valid:
                raise InvalidMemoryError("primary RTTI vtable missing for %s" % name)
            self._vtables[name] = valid

    def snapshot(self, *, divisions=False):
        """Read country totals; opt into division rows with ``divisions=True``.

        Malformed/torn data raises and never returns a partial sample. The default
        counts divisions from the guarded array header without following them.
        """
        if type(divisions) is not bool:
            raise ValueError("divisions must be a boolean")
        return self._snapshot(clock_only=False, divisions=divisions)

    def clock_snapshot(self):
        """Read only root identity and statically verified date/pause/internal speed."""
        return self._snapshot(clock_only=True, divisions=False)

    def _snapshot(self, clock_only, divisions):
        sample = _Sample(self)
        result = {"backend": "memory", "build_key": self.build_key,
                  "availability": {"available": [], "missing": dict(UNAVAILABLE)},
                  "limitations": list(LIMITATIONS), "country": None}
        if clock_only:
            result.pop("country")
            result["scope"] = "clock"
        try:
            sample.collect(result, clock_only=clock_only, include_divisions=divisions)
        except PlayedCountryUnavailable as exc:
            result["availability"]["missing"]["played_country"] = str(exc)
            # Missing player identity means none of its dependent fields may escape.
            if not clock_only:
                result["country"] = None
            if "date" not in result:
                result["availability"]["missing"]["date"] = "a verified loaded-game root is unavailable"
            result["availability"]["available"] = [field for field in ("date", "day", "hour", "paused", "speed_internal")
                                                    if field in result]
        sample.finish()
        return result


class _Sample:
    def __init__(self, reader):
        self.reader = reader
        self.layout = reader.layout
        self.witnesses = {}
        self.read_bytes = 0
        self.read_count = 0

    def read(self, address, size, *, witness=False):
        checked_range(address, size)
        self.read_bytes += size
        self.read_count += 1
        if self.read_bytes > MAX_SNAPSHOT_BYTES or self.read_count > MAX_SNAPSHOT_READS:
            raise InvalidMemoryError("snapshot read budget exceeded")
        data = self.reader.process.read(address, size)
        if not isinstance(data, bytes) or len(data) != size:
            raise MemoryReadError("partial process memory read rejected")
        if witness:
            key = (address, size)
            old = self.witnesses.setdefault(key, data)
            if old != data:
                raise TornReadError("structural witness changed during snapshot")
        return data

    def i32(self, address, *, witness=False):
        return struct.unpack("<i", self.read(address, 4, witness=witness))[0]

    def u64(self, address, *, witness=False):
        return struct.unpack("<Q", self.read(address, 8, witness=witness))[0]

    def fixed(self, address):
        return struct.unpack("<q", self.read(address, 8))[0] / 100000.0

    def object(self, address, class_name):
        checked_range(address, 8)
        if address % 8:
            raise InvalidMemoryError("unaligned object pointer")
        vtable = self.u64(address, witness=True)
        if vtable not in self.reader._vtables[class_name]:
            raise InvalidMemoryError("object vtable does not match %s" % class_name)
        return address

    def array(self, address, element_size, max_items):
        header = self.read(address, 24, witness=True)
        data, capacity, size, _allocator = struct.unpack("<QiiQ", header)
        if capacity < 0 or size < 0 or size > capacity or capacity > max_items:
            raise InvalidMemoryError("invalid or oversized CPdxArray header")
        if data:
            if data % element_size:
                raise InvalidMemoryError("unaligned array data pointer")
            checked_range(data, max(1, capacity * element_size))
        elif capacity or size:
            raise InvalidMemoryError("null nonempty array pointer")
        return data, size

    def string(self, address):
        header = self.read(address, 32, witness=True)
        size, capacity = struct.unpack_from("<QQ", header, 16)
        if size > capacity or size > MAX_STRING_BYTES or capacity > MAX_STRING_BYTES:
            raise InvalidMemoryError("invalid or oversized MSVC string")
        if capacity < 16:
            if capacity != 15:
                raise InvalidMemoryError("invalid MSVC small-string capacity")
            raw = header[:size]
            if header[size] != 0:
                raise InvalidMemoryError("unterminated MSVC small string")
        else:
            pointer = struct.unpack_from("<Q", header)[0]
            checked_range(pointer, size + 1)
            raw = self.read(pointer, size + 1, witness=True)
            if raw[-1] != 0:
                raise InvalidMemoryError("unterminated MSVC string")
            raw = raw[:-1]
        if b"\0" in raw:
            raise InvalidMemoryError("embedded NUL in country name")
        try:
            return raw.decode("utf-8")
        except UnicodeDecodeError as exc:
            raise InvalidMemoryError("invalid UTF-8 country name") from exc

    def collect(self, result, *, clock_only=False, include_divisions=False):
        layout = self.layout
        self.verify_image()
        roots = {}
        root_fields = [(layout.game_state_rva, "CCurrentGameState"), (layout.idler_rva, "CInGameIdler")]
        if not clock_only:
            root_fields.append((layout.session_rva, "CSession"))
        for field, name in root_fields:
            if field + 8 > self.image_size:
                raise InvalidMemoryError("root RVA outside executable module")
            pointer = self.u64(self.reader.module_base + field, witness=True)
            roots[name] = self.object(pointer, name) if pointer else None
        if any(pointer is None for pointer in roots.values()):
            raise PlayedCountryUnavailable("a verified loaded-game root is unavailable")
        state = roots["CCurrentGameState"]
        raw_date = self.i32(state + layout.date_raw, witness=True)
        date = decode_date(raw_date)
        result.update(date=date["date"], day=date["day_number"], hour=date["hour"], date_raw=raw_date)
        paused = self.read(roots["CInGameIdler"] + layout.idler_paused, 1, witness=True)[0]
        if paused not in (0, 1):
            raise InvalidMemoryError("invalid paused flag")
        speed = self.i32(state + layout.speed_internal, witness=True)
        if not 0 <= speed <= 4:
            raise InvalidMemoryError("invalid internal game speed")
        result.update(paused=bool(paused), speed_internal=speed)
        result["availability"]["available"] = ["date", "day", "hour", "paused", "speed_internal"]
        if clock_only:
            return
        played = self.i32(state + layout.played_index, witness=True)
        countries, country_count = self.array(state + layout.countries, 8, MAX_COUNTRIES)
        tags, tag_count = self.array(state + layout.tag_to_country, 4, MAX_COUNTRIES)
        # Offline caller 0x1372438 chooses +0x520 only when positive; +0x524
        # is an unverified alternate field and must never be used as a fallback.
        if not 0 < played < tag_count:
            raise PlayedCountryUnavailable("played tag index is out of bounds")
        country_index = self.i32(tags + played * 4, witness=True)
        if not 0 <= country_index < country_count:
            raise PlayedCountryUnavailable("played country mapping is out of bounds")
        # Offline 0xba6240 maps tag through +0x340, then 0xba56a0 indexes
        # countries at +0x310. Direct indexing by the tag would read another
        # country's data after tag/country index spaces diverge.
        country = self.object(self.u64(countries + country_index * 8, witness=True), "CCountry")
        country_id = self.i32(country + layout.country_id, witness=True)
        if country_id != country_index:
            raise PlayedCountryUnavailable("played country ID disagrees with its array index")
        visible = {"id": country_id, "played_tag_index": played,
                   "name": self.string(country + layout.country_name),
                   "stability_base": self.fixed(country + layout.country_stability_base),
                   "war_support_base": self.fixed(country + layout.country_war_support_base)}
        for field in ("stability_base", "war_support_base"):
            # Base fractions can exceed one through modifiers, but absurd values
            # signal a stale layout, not an informative measurement.
            if not -10 <= visible[field] <= 10:
                raise InvalidMemoryError("implausible base fraction")
        production_ptr = self.u64(country + layout.country_production, witness=True)
        if production_ptr:
            production = self.object(production_ptr, "CProductionStatus")
            factories = {}
            for field, offset in (("military", layout.military_factories), ("dockyards", layout.dockyards),
                                  ("civilian", layout.civilian_factories)):
                value = self.i32(production + offset)
                if not 0 <= value <= 1000000:
                    raise InvalidMemoryError("invalid factory count")
                factories[field] = value
            visible["factories"] = factories
        else:
            result["availability"]["missing"]["factories"] = "production status is unavailable"
        political_ptr = self.u64(country + layout.country_political, witness=True)
        if political_ptr:
            political = self.object(political_ptr, "CPoliticalStatus")
            power = self.fixed(political + layout.political_power)
            if not -1000000 <= power <= 1000000:
                raise InvalidMemoryError("implausible political power")
            visible["political_power"] = power
        else:
            result["availability"]["missing"]["political_power"] = "political status is unavailable"
        technology_ptr = self.u64(country + layout.country_technology, witness=True)
        if technology_ptr:
            visible["research"] = self.research(technology_ptr)
        else:
            result["availability"]["missing"]["research"] = "technology status is unavailable"
        divisions, count = self.array(country + layout.country_divisions, 8, MAX_DIVISIONS)
        rows = []
        for index in range(count if include_divisions else 0):
            division = self.object(self.u64(divisions + index * 8, witness=True), "CArmy")
            kind, handle_id = struct.unpack("<II", self.read(division + layout.division_handle, 8, witness=True))
            if kind != 51:
                raise InvalidMemoryError("division handle has the wrong type")
            row = {"id": handle_id, "strength": self.fixed(division + layout.division_strength),
                   "organisation": self.fixed(division + layout.division_organisation),
                   "experience": self.fixed(division + layout.division_experience)}
            if any(not 0 <= row[field] <= 1000000 for field in ("strength", "organisation", "experience")):
                raise InvalidMemoryError("implausible division measurement")
            rows.append(row)
        visible["division_count"] = count
        if include_divisions:
            visible["divisions"] = rows
        else:
            result["availability"]["missing"]["divisions"] = "division details were not requested"
        result["country"] = visible
        result["availability"]["available"].extend("country." + field for field in visible)

    def research(self, status_pointer):
        """Read only proven slot structure; progress and tech IDs stay unknown."""
        status = self.object(status_pointer, "CTechnologyStatus")
        slots, count = self.array(status + self.layout.research_slots, 8, MAX_RESEARCH_SLOTS)
        busy = 0
        seen = set()
        for index in range(count):
            slot_pointer = self.u64(slots + index * 8, witness=True)
            if slot_pointer in seen:
                raise InvalidMemoryError("duplicate research slot pointer")
            seen.add(slot_pointer)
            slot = self.object(slot_pointer, "CResearchSlot")
            owner = self.u64(slot + self.layout.research_slot_owner, witness=True)
            if owner != status:
                raise InvalidMemoryError("research slot does not belong to the played country's technology status")
            technology = self.u64(slot + self.layout.research_slot_technology, witness=True)
            if technology:
                self.object(technology, "CTechnology")
                busy += 1
        return {"total": count, "busy": busy, "idle": count - busy}

    def verify_image(self):
        """Check bounded, loaded PE headers as well as the offline build key."""
        base = self.reader.module_base
        if self.reader.module_size is not None and self.reader.module_size < 64:
            raise InvalidMemoryError("loaded executable module is smaller than its DOS header")
        dos = self.read(base, 64, witness=True)
        if dos[:2] != b"MZ":
            raise InvalidMemoryError("loaded module has no DOS header")
        pe_offset = struct.unpack_from("<I", dos, 0x3C)[0]
        if not 64 <= pe_offset <= 0x1000:
            raise InvalidMemoryError("loaded PE header offset is out of bounds")
        if self.reader.module_size is not None and pe_offset + 88 > self.reader.module_size:
            raise InvalidMemoryError("loaded PE header extends beyond the module")
        header = self.read(base + pe_offset, 88, witness=True)
        if header[:4] != b"PE\0\0" or struct.unpack_from("<H", header, 4)[0] != 0x8664:
            raise InvalidMemoryError("loaded module is not an x64 PE executable")
        if struct.unpack_from("<H", header, 20)[0] < 64 or struct.unpack_from("<H", header, 24)[0] != 0x20B:
            raise InvalidMemoryError("loaded module has an invalid PE32+ optional header")
        timestamp = struct.unpack_from("<I", header, 8)[0]
        expected_timestamp = int(self.reader.build_key.split("_", 1)[0], 16)
        if timestamp != expected_timestamp:
            raise UnsupportedBuildError("loaded executable timestamp disagrees with verified build")
        self.image_size = struct.unpack_from("<I", header, 24 + 56)[0]
        checked_range(base, self.image_size, 1 << 30)
        if self.reader.module_size is not None and self.image_size != self.reader.module_size:
            raise InvalidMemoryError("loaded PE size disagrees with owned executable module")
        if pe_offset + 88 > self.image_size:
            raise InvalidMemoryError("loaded PE header extends beyond the module")
        for vtables in self.reader._vtables.values():
            if any(not base <= pointer <= base + self.image_size - 8 for pointer in vtables):
                raise InvalidMemoryError("RTTI vtable lies outside loaded executable module")

    def finish(self):
        # Check every structural dependency, including roots even for unavailable
        # samples, so a menu/load transition cannot produce a misleading result.
        # Recheck roots last, after dependent object and array witnesses.
        for (address, size), expected in reversed(tuple(self.witnesses.items())):
            if self.read(address, size) != expected:
                raise TornReadError("structural witness changed before snapshot completed")
