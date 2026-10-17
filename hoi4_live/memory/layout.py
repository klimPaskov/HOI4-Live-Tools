"""Verified Windows offsets, deliberately restricted to the handoff build.

Evidence: docs/handoff_2026-10-11_redesign.md and offline PE disassembly recorded
in the redesign evidence. Unverified Linux fields are not imported. The played
field is a tag index which must pass through tag_to_country before countries.
"""

from dataclasses import dataclass

from .winproc import MemoryReadError


class UnsupportedBuildError(MemoryReadError):
    pass


@dataclass(frozen=True)
class Layout:
    build_key: str
    game_state_rva: int = 0x3316E90
    idler_rva: int = 0x33172E0
    session_rva: int = 0x3438E48
    countries: int = 0x310
    tag_to_country: int = 0x340
    played_index: int = 0x520
    date_raw: int = 0x468
    speed_internal: int = 0x4BC
    idler_paused: int = 0x6C1
    country_id: int = 0x08
    country_name: int = 0x10
    country_divisions: int = 0x290
    country_technology: int = 0xF60
    country_production: int = 0xF68
    country_political: int = 0xF90
    country_stability_base: int = 0x10D0
    country_war_support_base: int = 0x10D8
    military_factories: int = 0x2B8
    dockyards: int = 0x318
    civilian_factories: int = 0x378
    political_power: int = 0xE0
    division_handle: int = 0x18
    division_strength: int = 0x420
    division_organisation: int = 0x428
    division_experience: int = 0x430
    research_slots: int = 0xA0
    research_slot_owner: int = 0x10
    research_slot_technology: int = 0x18


VERIFIED_BUILD_KEY = "6a427fb9_3519e78"
_LAYOUTS = {VERIFIED_BUILD_KEY: Layout(VERIFIED_BUILD_KEY)}


def get_layout(build_key):
    try:
        return _LAYOUTS[build_key]
    except (KeyError, TypeError):
        raise UnsupportedBuildError("no verified Windows memory layout for build %r" % (build_key,)) from None
