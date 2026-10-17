"""Offline HOI4 PE layout evidence; never opens or queries a process.

Requires capstone and numpy (the existing PE reader's dependency). Example:
  python scripts/memory_spike/static_layout.py --xref 0x3316e90 --contains 0x520
  python scripts/memory_spike/static_layout.py --function 0x123456
All displayed addresses are RVAs. This is research tooling, not a layout detector.
"""
import argparse
import bisect
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from hoi4_live.memory.pe import Image
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86 import X86_OP_MEM, X86_REG_RIP

DEFAULT_EXE = r"C:/Program Files (x86)/Steam/steamapps/common/Hearts of Iron IV/hoi4.exe"


class OfflineEvidence:
    def __init__(self, path):
        self.image = Image(path)
        self.starts, self.ends = self.image.function_table()
        self.decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        self.decoder.detail = True

    def owner(self, rva):
        i = bisect.bisect_right(self.starts, rva) - 1
        if i >= 0 and rva < self.ends[i]:
            return int(self.starts[i]), int(self.ends[i])
        return None

    def instructions(self, start, end):
        return list(self.decoder.disasm(self.image.code(start, end - start), start))

    def rip_xrefs(self, target):
        """Candidate 64-bit RIP mov/lea/store sites, verified with Capstone.

        Deliberately not a complete xref index: other opcode forms are omitted.
        """
        _, va, size, raw, raw_size = self.image.section(".text")
        code = self.image.data[raw:raw + min(size, raw_size)]
        pattern = rb"(?=([\x40-\x4f][\x8b\x8d\x89][\x05\x0d\x15\x1d\x25\x2d\x35\x3d].{4}))"
        sites = []
        for match in re.finditer(pattern, code, re.DOTALL):
            candidate = va + match.start()
            if candidate + 7 + struct.unpack_from("<i", match.group(1), 3)[0] != target:
                continue
            ins = next(self.decoder.disasm(match.group(1), candidate), None)
            if ins and any(op.type == X86_OP_MEM and op.mem.base == X86_REG_RIP
                           and ins.address + ins.size + op.mem.disp == target
                           for op in ins.operands):
                owner = self.owner(candidate)
                # Verify this really lies on a decoded instruction boundary.
                if owner and any(i.address == candidate for i in self.instructions(*owner)):
                    sites.append(candidate)
        return sites

    @staticmethod
    def format_instruction(ins):
        return f"{ins.address:08x}: {ins.mnemonic:8s} {ins.op_str}"

    def metadata(self):
        return {"path": str(self.image.path), "sha256": hashlib.sha256(self.image.data).hexdigest(),
                "size": self.image.size, "timestamp": hex(self.image.timestamp),
                "build_key": self.image.build_key, "image_base": hex(self.image.image_base)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", default=DEFAULT_EXE)
    parser.add_argument("--xref", type=lambda v: int(v, 0))
    parser.add_argument("--function", type=lambda v: int(v, 0))
    parser.add_argument("--length", type=lambda v: int(v, 0), help="Explicit byte range from --function; spans split unwind fragments")
    parser.add_argument("--contains", type=lambda v: int(v, 0), help="Filter functions by a memory displacement")
    parser.add_argument("--limit", type=int, default=30)
    parser.add_argument("--context", type=int, help="Only show this many instructions around --contains matches")
    parser.add_argument("--shortest-first", action="store_true", help="Inspect small unwind fragments first")
    args = parser.parse_args()
    evidence = OfflineEvidence(args.exe)
    print(json.dumps(evidence.metadata(), indent=2))
    functions = []
    if args.function is not None:
        owner = (args.function, args.function + args.length) if args.length else evidence.owner(args.function)
        if owner is None:
            parser.error("RVA has no .pdata function owner")
        functions.append(owner)
    if args.xref is not None:
        sites = evidence.rip_xrefs(args.xref)
        print(f"Verified RIP mov/lea/store references to {args.xref:#x}: {len(sites)}")
        functions.extend(sorted(set(evidence.owner(site) for site in sites)))
    count = 0
    if args.shortest_first:
        functions.sort(key=lambda bounds: bounds[1] - bounds[0])
    for start, end in dict.fromkeys(functions):
        insns = evidence.instructions(start, end)
        if args.contains is not None and not any(op.type == X86_OP_MEM and op.mem.disp == args.contains
                                                for ins in insns for op in ins.operands):
            continue
        count += 1
        print(f"\nFUNCTION {start:#x}..{end:#x} ({len(insns)} decoded instructions)")
        selected = set(range(len(insns)))
        if args.context is not None and args.contains is not None:
            selected = set()
            for index, ins in enumerate(insns):
                if any(op.type == X86_OP_MEM and op.mem.disp == args.contains for op in ins.operands):
                    selected.update(range(max(0, index - args.context), min(len(insns), index + args.context + 1)))
        previous = -1
        for index in sorted(selected):
            if previous != index - 1:
                print("...")
            print(evidence.format_instruction(insns[index]))
            previous = index
        if count >= args.limit:
            break
    print(f"\nDisplayed {count} function(s). Static bytes only; no process access.")


if __name__ == "__main__":
    main()
