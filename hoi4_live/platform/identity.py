"""Portable immutable process identity; native readers must prove all fields."""
from dataclasses import dataclass


@dataclass(frozen=True)
class ProcessIdentity:
    pid: int
    creation_time: int
    executable_path: str
