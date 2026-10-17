"""Offline ownership and hostile-memory tests; never open a real process."""

import dataclasses
import struct
import unittest
from unittest.mock import patch

import tests.helpers  # noqa: F401
from hoi4_live.memory.layout import VERIFIED_BUILD_KEY, UnsupportedBuildError, get_layout
from hoi4_live.memory.reader import DATE_EPOCH_HOURS, MemoryReader, TornReadError, decode_date
from hoi4_live.memory.winproc import (
    MAX_READ_BYTES, MAX_USER_ADDRESS, PROCESS_QUERY_INFORMATION, READ_ONLY_RIGHTS,
    InvalidMemoryError, MemoryReadError, ModuleInfo, OwnershipError, ProcessIdentity,
    ReadOnlyProcess, capture_launched_identity,
)


IDENTITY = ProcessIdentity(731, 133456789012345678, r"C:\game\hoi4.exe")
BASE = 0x140000000
MODULE_SIZE = 0x4000000
STATE, IDLER, SESSION, COUNTRY = 0x200000, 0x210000, 0x220000, 0x230000
PRODUCTION, POLITICAL, DIVISION = 0x240000, 0x250000, 0x260000
COUNTRIES, TAGS, DIVISIONS = 0x300000, 0x310000, 0x320000
TECHNOLOGY_STATUS, IDLE_SLOT, BUSY_SLOT, TECHNOLOGY = 0x270000, 0x280000, 0x281000, 0x290000
RESEARCH_SLOTS = 0x330000
CLASSES = ("CCurrentGameState", "CInGameIdler", "CSession", "CCountry",
           "CProductionStatus", "CPoliticalStatus", "CArmy", "CTechnologyStatus", "CResearchSlot", "CTechnology")
RTTI = {".?AV%s@@" % name: [(0x1000 + index * 0x100, 0)] for index, name in enumerate(CLASSES)}


class FakeAPI:
    def __init__(self):
        self.identity = IDENTITY
        self.opens = []
        self.closed = []
        self.memory = {}
        self.reads = []
        self.identity_queries = 0
        self.before_read = None
        self.short = False
        self.fail = False

    def open_process(self, pid, rights):
        self.opens.append((pid, rights))
        return 99

    def get_process_identity(self, handle, pid):
        self.identity_queries += 1
        if self.fail:
            raise OSError("exited")
        return self.identity

    def module_info(self, handle, pid):
        return ModuleInfo(BASE, MODULE_SIZE, self.identity.executable_path)

    def close(self, handle):
        self.closed.append(handle)

    def read(self, handle, address, size):
        self.reads.append((address, size))
        if self.before_read:
            self.before_read(address, size)
        try:
            data = bytes(self.memory[address + index] for index in range(size))
        except KeyError:
            raise OSError("unmapped fake address") from None
        return data[:-1] if self.short else data

    def put(self, address, data):
        self.memory.update({address + index: byte for index, byte in enumerate(data)})

    def pack(self, address, format_, *values):
        self.put(address, struct.pack(format_, *values))

    def object(self, address, name):
        self.pack(address, "<Q", BASE + RTTI[".?AV%s@@" % name][0][0])

    def array(self, address, data, capacity, size):
        self.pack(address, "<QiiQ", data, capacity, size, 0)

    def string(self, address, value):
        raw = value.encode("utf-8")
        if len(raw) < 16:
            self.put(address, raw.ljust(16, b"\0") + struct.pack("<QQ", len(raw), 15))
        else:
            pointer = 0x400000
            self.pack(address, "<Q8xQQ", pointer, len(raw), len(raw))
            self.put(pointer, raw + b"\0")


def fixture():
    api = FakeAPI()
    layout = get_layout(VERIFIED_BUILD_KEY)
    api.put(BASE, b"MZ" + b"\0" * 62)
    api.pack(BASE + 0x3C, "<I", 0x100)
    api.put(BASE + 0x100, b"PE\0\0" + b"\0" * 84)
    api.pack(BASE + 0x104, "<H", 0x8664)
    api.pack(BASE + 0x108, "<I", 0x6A427FB9)
    api.pack(BASE + 0x114, "<H", 240)
    api.pack(BASE + 0x118, "<H", 0x20B)
    api.pack(BASE + 0x150, "<I", MODULE_SIZE)
    for rva, pointer, name in ((layout.game_state_rva, STATE, "CCurrentGameState"),
                               (layout.idler_rva, IDLER, "CInGameIdler"),
                               (layout.session_rva, SESSION, "CSession")):
        api.pack(BASE + rva, "<Q", pointer)
        api.object(pointer, name)
    for address, name in ((COUNTRY, "CCountry"), (PRODUCTION, "CProductionStatus"),
                          (POLITICAL, "CPoliticalStatus"), (DIVISION, "CArmy")):
        api.object(address, name)
    api.pack(STATE + layout.played_index, "<i", 1)
    api.pack(STATE + layout.date_raw, "<i", 60759360)
    api.pack(STATE + layout.speed_internal, "<i", 2)
    api.pack(IDLER + layout.idler_paused, "<B", 1)
    api.array(STATE + layout.countries, COUNTRIES, 2, 2)
    # The enemy entry is deliberately invalid: the reader must not follow it.
    api.pack(COUNTRIES, "<QQ", 0xDEADBEEF, COUNTRY)
    api.array(STATE + layout.tag_to_country, TAGS, 2, 2)
    api.pack(TAGS, "<ii", 0, 1)
    api.pack(COUNTRY + layout.country_id, "<i", 1)
    api.string(COUNTRY + layout.country_name, "German Reich")
    api.pack(COUNTRY + layout.country_stability_base, "<q", 70000)
    api.pack(COUNTRY + layout.country_war_support_base, "<q", 30000)
    api.pack(COUNTRY + layout.country_production, "<Q", PRODUCTION)
    api.pack(COUNTRY + layout.country_political, "<Q", POLITICAL)
    api.pack(COUNTRY + layout.country_technology, "<Q", 0)
    api.pack(PRODUCTION + layout.military_factories, "<i", 28)
    api.pack(PRODUCTION + layout.dockyards, "<i", 10)
    api.pack(PRODUCTION + layout.civilian_factories, "<i", 35)
    api.pack(POLITICAL + layout.political_power, "<q", 132400)
    api.array(COUNTRY + layout.country_divisions, DIVISIONS, 1, 1)
    api.pack(DIVISIONS, "<Q", DIVISION)
    api.pack(DIVISION + layout.division_handle, "<II", 51, 77)
    api.pack(DIVISION + layout.division_strength, "<q", 22720000)
    api.pack(DIVISION + layout.division_organisation, "<q", 5345000)
    api.pack(DIVISION + layout.division_experience, "<q", 12500)
    return api, layout


def make_reader(api):
    process = ReadOnlyProcess(IDENTITY, owned=True, api=api)
    return process, MemoryReader(process, BASE, VERIFIED_BUILD_KEY, RTTI, module_size=MODULE_SIZE)


def research_fixture():
    api, layout = fixture()
    api.pack(COUNTRY + layout.country_technology, "<Q", TECHNOLOGY_STATUS)
    api.object(TECHNOLOGY_STATUS, "CTechnologyStatus")
    api.array(TECHNOLOGY_STATUS + layout.research_slots, RESEARCH_SLOTS, 2, 2)
    api.pack(RESEARCH_SLOTS, "<QQ", IDLE_SLOT, BUSY_SLOT)
    for pointer, technology in ((IDLE_SLOT, 0), (BUSY_SLOT, TECHNOLOGY)):
        api.object(pointer, "CResearchSlot")
        api.pack(pointer + layout.research_slot_owner, "<Q", TECHNOLOGY_STATUS)
        api.pack(pointer + layout.research_slot_technology, "<Q", technology)
    api.object(TECHNOLOGY, "CTechnology")
    return api, layout


class ProcessAccessTests(unittest.TestCase):
    def test_explicit_ownership_required_without_open(self):
        api = FakeAPI()
        for kwargs in ({}, {"owned": False}):
            with self.assertRaises(OwnershipError):
                ReadOnlyProcess(IDENTITY, api=api, **kwargs)
        self.assertEqual(api.opens, [])

    def test_metadata_capture_uses_no_vm_read_and_closes(self):
        api = FakeAPI()
        self.assertEqual(capture_launched_identity(IDENTITY.pid, IDENTITY.executable_path, api), IDENTITY)
        self.assertEqual(api.opens, [(IDENTITY.pid, PROCESS_QUERY_INFORMATION)])
        self.assertEqual(api.reads, [])
        self.assertEqual(api.closed, [99])

    def test_capture_path_mismatch_closes(self):
        api = FakeAPI()
        with self.assertRaises(OwnershipError):
            capture_launched_identity(IDENTITY.pid, r"C:\other\hoi4.exe", api)
        self.assertEqual(api.closed, [99])

    def test_only_read_query_rights_and_context_close(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as process:
            self.assertEqual(process.module_info(), ModuleInfo(BASE, MODULE_SIZE, IDENTITY.executable_path))
        process.close()
        self.assertEqual(api.opens, [(IDENTITY.pid, READ_ONLY_RIGHTS)])
        self.assertEqual(api.closed, [99])

    def test_pid_reuse_rejected_before_first_read_and_closes(self):
        api = FakeAPI()
        api.identity = dataclasses.replace(IDENTITY, creation_time=IDENTITY.creation_time + 1)
        with self.assertRaises(OwnershipError):
            ReadOnlyProcess(IDENTITY, owned=True, api=api)
        self.assertEqual(api.closed, [99])
        self.assertEqual(api.reads, [])

    def test_malformed_identity_rejected_before_open(self):
        api = FakeAPI()
        for identity in (None, dataclasses.replace(IDENTITY, creation_time=True),
                         dataclasses.replace(IDENTITY, executable_path="hoi4.exe"),
                         dataclasses.replace(IDENTITY, executable_path=r"C:\game\other.exe")):
            with self.assertRaises(OwnershipError):
                ReadOnlyProcess(identity, owned=True, api=api)
        self.assertEqual(api.opens, [])

    def test_exit_and_identity_change_during_lifetime_close(self):
        for mode in ("exit", "path", "pid", "creation"):
            with self.subTest(mode=mode):
                api = FakeAPI()
                process = ReadOnlyProcess(IDENTITY, owned=True, api=api)
                if mode == "exit":
                    api.fail = True
                elif mode == "path":
                    api.identity = dataclasses.replace(IDENTITY, executable_path=r"C:\other\hoi4.exe")
                elif mode == "pid":
                    api.identity = dataclasses.replace(IDENTITY, pid=999)
                else:
                    api.identity = dataclasses.replace(IDENTITY, creation_time=1)
                with self.assertRaises((OwnershipError, OSError)):
                    process.read(0x200000, 8)
                self.assertEqual(api.reads, [])
                self.assertEqual(api.closed, [99])

    def test_partial_read_rejected(self):
        api = FakeAPI()
        api.put(0x200000, b"12345678")
        api.short = True
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as process:
            with self.assertRaises(MemoryReadError):
                process.read(0x200000, 8)

    def test_bounds_rejected_before_identity_or_read(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as process:
            queries = api.identity_queries
            for address, size in ((0, 8), (-1, 8), (True, 8), (0x10000, 0),
                                  (MAX_USER_ADDRESS, 8), (0x10000, MAX_READ_BYTES + 1)):
                with self.subTest(address=address, size=size), self.assertRaises(InvalidMemoryError):
                    process.read(address, size)
            self.assertEqual(api.identity_queries, queries)
            self.assertEqual(api.reads, [])


class GuardedReaderTests(unittest.TestCase):
    def test_verified_snapshot_never_follows_enemy_pointer(self):
        api, _layout = fixture()
        process, reader = make_reader(api)
        with process:
            snapshot = reader.snapshot(divisions=True)
        country = snapshot["country"]
        self.assertEqual(country["id"], 1)
        self.assertEqual(country["name"], "German Reich")
        self.assertEqual(country["factories"], {"military": 28, "civilian": 35, "dockyards": 10})
        self.assertEqual(country["political_power"], 1.324)
        self.assertEqual(country["divisions"][0]["strength"], 227.2)
        self.assertEqual(country["stability_base"], 0.7)
        self.assertEqual(snapshot["date"], "1936.1.1.0")
        self.assertEqual(snapshot["day"], 1936 * 365)
        self.assertNotIn("date", country)
        self.assertNotIn(0xDEADBEEF, [address for address, _ in api.reads])

    def test_unknown_build_rejected_without_read(self):
        api = FakeAPI()
        process = ReadOnlyProcess(IDENTITY, owned=True, api=api)
        with process, self.assertRaises(UnsupportedBuildError):
            MemoryReader(process, BASE, "unverified", RTTI)
        self.assertEqual(api.reads, [])

    def test_clock_snapshot_is_bounded_and_does_not_access_country(self):
        api, _layout = fixture()
        process, reader = make_reader(api)
        with process:
            snapshot = reader.clock_snapshot()
        self.assertEqual(snapshot["date"], "1936.1.1.0")
        self.assertTrue(snapshot["paused"])
        self.assertEqual(snapshot["speed_internal"], 2)
        self.assertNotIn("country", snapshot)
        self.assertLessEqual(len(api.reads), 20)
        for forbidden in (COUNTRY, SESSION, COUNTRIES, TAGS, DIVISIONS, DIVISION):
            self.assertNotIn(forbidden, [address for address, _ in api.reads])

    def test_invalid_pause_and_speed_reject_clock_sample(self):
        for address, format_, value in ((IDLER + 0x6C1, "<B", 2), (STATE + 0x4BC, "<i", -1),
                                        (STATE + 0x4BC, "<i", 5)):
            api, _layout = fixture()
            api.pack(address, format_, value)
            process, reader = make_reader(api)
            with process, self.assertRaises(InvalidMemoryError):
                reader.clock_snapshot()

    def test_clock_tick_change_rejects_sample(self):
        api, layout = fixture()
        def mutate(address, size):
            if address == STATE + layout.speed_internal:
                api.pack(STATE + layout.date_raw, "<i", 60759361)
        api.before_read = mutate
        process, reader = make_reader(api)
        with process, self.assertRaises(TornReadError):
            reader.clock_snapshot()

    def test_missing_primary_rtti_and_out_of_module_vtables_rejected(self):
        api = FakeAPI()
        with ReadOnlyProcess(IDENTITY, owned=True, api=api) as process:
            for table in ({}, {**RTTI, ".?AVCArmy@@": [(MODULE_SIZE, 0)]},
                          {**RTTI, ".?AVCArmy@@": [(0x1600, 8)]}):
                with self.assertRaises(InvalidMemoryError):
                    MemoryReader(process, BASE, VERIFIED_BUILD_KEY, table, module_size=MODULE_SIZE)
        self.assertEqual(api.reads, [])

    def test_loaded_image_header_guards(self):
        targets = ((BASE, "<H", 0), (BASE + 0x3C, "<I", 0xFFFFFF00),
                   (BASE + 0x104, "<H", 0x14C), (BASE + 0x118, "<H", 0x10B),
                   (BASE + 0x150, "<I", MODULE_SIZE - 4096))
        for address, format_, value in targets:
            with self.subTest(address=address):
                api, _layout = fixture()
                api.pack(address, format_, value)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot(divisions=True)

    def test_loaded_executable_unknown_timestamp_rejected(self):
        api, _layout = fixture()
        api.pack(BASE + 0x108, "<I", 0x12345678)
        process, reader = make_reader(api)
        with process, self.assertRaises(UnsupportedBuildError):
            reader.snapshot()
        self.assertNotIn(STATE, [address for address, _ in api.reads])

    def test_every_root_and_object_vtable_checked(self):
        for pointer in (STATE, IDLER, SESSION, COUNTRY, PRODUCTION, POLITICAL, DIVISION):
            with self.subTest(pointer=pointer):
                api, _layout = fixture()
                api.pack(pointer, "<Q", BASE + 0x6660)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot(divisions=True)

    def test_no_loaded_game_is_explicitly_unavailable(self):
        api, layout = fixture()
        api.pack(BASE + layout.game_state_rva, "<Q", 0)
        process, reader = make_reader(api)
        with process:
            snapshot = reader.snapshot()
        self.assertIsNone(snapshot["country"])
        self.assertIn("played_country", snapshot["availability"]["missing"])

    def test_tag_and_country_indices_differ_and_only_mapped_country_is_read(self):
        api, _layout = fixture()
        api.pack(TAGS + 4, "<i", 0)
        api.pack(COUNTRIES, "<Q", COUNTRY)
        api.pack(COUNTRIES + 8, "<Q", 0xDEADBEEF)
        api.pack(COUNTRY + 8, "<i", 0)
        process, reader = make_reader(api)
        with process:
            snapshot = reader.snapshot()
        self.assertEqual(snapshot["country"]["id"], 0)
        self.assertEqual(snapshot["country"]["played_tag_index"], 1)
        self.assertNotIn(COUNTRIES + 8, [address for address, _ in api.reads])
        self.assertNotIn(0xDEADBEEF, [address for address, _ in api.reads])

    def test_played_zero_unavailable_and_no_fallback(self):
        api, layout = fixture()
        api.pack(STATE + layout.played_index, "<i", 0)
        api.pack(COUNTRIES, "<Q", COUNTRY)
        api.pack(COUNTRY + layout.country_id, "<i", 0)
        # The neighboring unverified field is deliberately absent.
        process, reader = make_reader(api)
        with process:
            self.assertIsNone(reader.snapshot()["country"])
        self.assertNotIn(COUNTRY, [address for address, _ in api.reads])
        self.assertNotIn(STATE + layout.played_index + 4, [address for address, _ in api.reads])

    def test_date_decoder_boundaries_and_365_day_calendar(self):
        for year, month, day, day_of_year in ((1936, 1, 1, 0), (1936, 2, 28, 58),
                                            (1936, 3, 1, 59), (1936, 12, 31, 364), (1937, 1, 1, 0)):
            raw = DATE_EPOCH_HOURS + (year * 365 + day_of_year) * 24 + 23
            date = decode_date(raw)
            self.assertEqual((date["year"], date["month"], date["day"], date["hour"]),
                             (year, month, day, 23))
        for raw in (0, -1, 1 << 31, True, DATE_EPOCH_HOURS):
            with self.assertRaises(InvalidMemoryError):
                decode_date(raw)

    def test_invalid_player_indices_and_id_are_unavailable(self):
        for kind, value in (("played", -1), ("played", 2), ("mapped", -1), ("mapped", 2), ("id", 42)):
            with self.subTest(kind=kind, value=value):
                api, layout = fixture()
                address = {"played": STATE + layout.played_index, "mapped": TAGS + 4,
                           "id": COUNTRY + layout.country_id}[kind]
                api.pack(address, "<i", value)
                process, reader = make_reader(api)
                with process:
                    self.assertIsNone(reader.snapshot()["country"])

    def test_hostile_array_bounds_null_pointer_and_misalignment(self):
        for data, capacity, size in ((COUNTRIES, -1, 1), (COUNTRIES, 1, 2),
                                     (COUNTRIES, 100000000, 1), (0, 2, 2),
                                     (COUNTRIES + 1, 2, 2), (MAX_USER_ADDRESS - 7, 2, 2)):
            with self.subTest(data=data, capacity=capacity, size=size):
                api, layout = fixture()
                api.array(STATE + layout.countries, data, capacity, size)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot()

    def test_malformed_country_string_and_long_utf8_string(self):
        api, layout = fixture()
        api.string(COUNTRY + layout.country_name, "A long country name")
        process, reader = make_reader(api)
        with process:
            self.assertEqual(reader.snapshot()["country"]["name"], "A long country name")
        for raw in (b"x" * 16 + struct.pack("<QQ", 1, 15),
                    b"\xff\0".ljust(16, b"\0") + struct.pack("<QQ", 1, 15),
                    b"\0" * 16 + struct.pack("<QQ", 1025, 1025),
                    b"\0" * 16 + struct.pack("<QQ", 5, 3)):
            with self.subTest(raw=raw):
                api, layout = fixture()
                api.put(COUNTRY + layout.country_name, raw)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot()

    def test_malformed_pointer_and_division_handle(self):
        for address, format_, value in ((COUNTRIES + 8, "<Q", 0), (COUNTRIES + 8, "<Q", COUNTRY + 1),
                                        (DIVISION + 0x18, "<I", 73)):
            with self.subTest(address=address, value=value):
                api, _layout = fixture()
                api.pack(address, format_, value)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot(divisions=True)

    def test_default_country_totals_do_not_read_division_objects(self):
        api, _layout = fixture()
        # The count-only path remains useful even when a division is changing.
        api.pack(DIVISIONS, "<Q", 0xDEADBEEF)
        process, reader = make_reader(api)
        with process:
            snapshot = reader.snapshot()
            before = len(api.reads)
            with self.assertRaises(ValueError):
                reader.snapshot(divisions="yes")
            self.assertEqual(len(api.reads), before)
        self.assertEqual(snapshot["country"]["division_count"], 1)
        self.assertNotIn("divisions", snapshot["country"])
        self.assertIn("divisions", snapshot["availability"]["missing"])
        for address in (DIVISIONS, DIVISION, 0xDEADBEEF):
            self.assertNotIn(address, [pointer for pointer, _ in api.reads])

    def test_verified_research_counts_without_unverified_identifiers(self):
        api, _layout = research_fixture()
        process, reader = make_reader(api)
        with process:
            snapshot = reader.snapshot()
        self.assertEqual(snapshot["country"]["research"], {"total": 2, "busy": 1, "idle": 1})
        self.assertIn("research_progress", snapshot["availability"]["missing"])
        self.assertIn("research_technology_ids", snapshot["availability"]["missing"])
        # Fake tech memory contains only its vtable, so guessed names, IDs or
        # progress accesses would fail this successful sample.
        self.assertTrue(all(address == TECHNOLOGY for address, _ in api.reads if TECHNOLOGY <= address < TECHNOLOGY + 0x1000))

    def test_empty_and_all_idle_research_arrays(self):
        for count in (0, 2):
            api, layout = research_fixture()
            if count == 0:
                api.array(TECHNOLOGY_STATUS + layout.research_slots, 0, 0, 0)
            else:
                api.pack(BUSY_SLOT + layout.research_slot_technology, "<Q", 0)
            process, reader = make_reader(api)
            with process:
                self.assertEqual(reader.snapshot()["country"]["research"], {"total": count, "busy": 0, "idle": count})

    def test_research_status_slot_and_technology_vtables(self):
        for pointer in (TECHNOLOGY_STATUS, IDLE_SLOT, BUSY_SLOT, TECHNOLOGY):
            with self.subTest(pointer=pointer):
                api, _layout = research_fixture()
                api.pack(pointer, "<Q", BASE + 0x6660)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot()

    def test_research_slot_owner_and_duplicate_null_pointers_rejected(self):
        for address, replacement in ((IDLE_SLOT + 0x10, POLITICAL), (RESEARCH_SLOTS, 0),
                                     (RESEARCH_SLOTS + 8, IDLE_SLOT)):
            with self.subTest(address=address):
                api, _layout = research_fixture()
                api.pack(address, "<Q", replacement)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot()

    def test_oversized_and_malformed_research_arrays(self):
        for data, capacity, size in ((RESEARCH_SLOTS, 65, 1), (RESEARCH_SLOTS, 1, 2),
                                     (0, 1, 1), (RESEARCH_SLOTS + 1, 2, 2)):
            with self.subTest(data=data, capacity=capacity, size=size):
                api, layout = research_fixture()
                api.array(TECHNOLOGY_STATUS + layout.research_slots, data, capacity, size)
                process, reader = make_reader(api)
                with process, self.assertRaises(InvalidMemoryError):
                    reader.snapshot()

    def test_research_assignment_change_rejects_whole_sample(self):
        for address, replacement in ((BUSY_SLOT + 0x18, 0), (BUSY_SLOT + 0x10, POLITICAL),
                                     (RESEARCH_SLOTS + 8, IDLE_SLOT)):
            with self.subTest(address=address):
                api, layout = research_fixture()
                def mutate(pointer, size):
                    if pointer == COUNTRY + layout.country_divisions:
                        api.pack(address, "<Q", replacement)
                api.before_read = mutate
                process, reader = make_reader(api)
                with process, self.assertRaises(TornReadError):
                    reader.snapshot()

    def test_optional_status_missing_without_guesses(self):
        api, layout = fixture()
        api.pack(COUNTRY + layout.country_production, "<Q", 0)
        api.pack(COUNTRY + layout.country_political, "<Q", 0)
        process, reader = make_reader(api)
        with process:
            snapshot = reader.snapshot()
        self.assertNotIn("factories", snapshot["country"])
        self.assertNotIn("political_power", snapshot["country"])
        self.assertIn("factories", snapshot["availability"]["missing"])

    def test_root_played_mapping_array_and_vtable_changes_reject_sample(self):
        api, layout = fixture()
        targets = [(BASE + layout.game_state_rva, "<Q", STATE + 0x10000),
                   (STATE + layout.date_raw, "<i", 60759361),
                   (STATE + layout.played_index, "<i", 0), (TAGS + 4, "<i", 0),
                   (COUNTRY + layout.country_divisions + 12, "<i", 0),
                   (COUNTRY, "<Q", BASE + 0x6660)]
        for target, format_, replacement in targets:
            with self.subTest(target=target):
                api, _layout = fixture()
                mutated = [False]
                def mutate(address, size):
                    if address == DIVISION + layout.division_experience and not mutated[0]:
                        api.pack(target, format_, replacement)
                        mutated[0] = True
                api.before_read = mutate
                process, reader = make_reader(api)
                with process, self.assertRaises(TornReadError):
                    reader.snapshot(divisions=True)

    def test_read_failure_never_returns_partial_snapshot(self):
        api, layout = fixture()
        del api.memory[POLITICAL + layout.political_power]
        process, reader = make_reader(api)
        with process, self.assertRaises(MemoryReadError):
            reader.snapshot()

    def test_snapshot_budget_limits_abort_without_partial_result(self):
        for key, limit in (("MAX_SNAPSHOT_BYTES", 64), ("MAX_SNAPSHOT_READS", 2)):
            with self.subTest(limit=key):
                api, _layout = fixture()
                process, reader = make_reader(api)
                with process, patch("hoi4_live.memory.reader." + key, limit), self.assertRaises(InvalidMemoryError):
                    reader.snapshot()


if __name__ == "__main__":
    unittest.main()
