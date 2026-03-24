#!/usr/bin/env python3
"""Generate JSON reference data from WAD files using omgifol.

Usage: python3 generate_reference.py <wad_path> <map_name> <output_dir>

Produces:
  <output_dir>/<map_name>_vertexes.json
  <output_dir>/<map_name>_linedefs.json
  <output_dir>/<map_name>_sidedefs.json
  <output_dir>/<map_name>_sectors.json
  <output_dir>/<map_name>_things.json
"""
import json
import os
import sys

import omg


def dump_map(wad_path, map_name, output_dir):
    w = omg.WAD(wad_path)
    if map_name not in w.maps:
        print(f"Map {map_name} not found in {wad_path}")
        sys.exit(1)

    e = omg.MapEditor(w.maps[map_name])
    os.makedirs(output_dir, exist_ok=True)
    wad_base = os.path.splitext(os.path.basename(wad_path))[0].lower()
    prefix = f"{wad_base}_{map_name.lower()}"

    # Vertexes
    verts = [{"x": v.x, "y": v.y} for v in e.vertexes]
    with open(os.path.join(output_dir, f"{prefix}_vertexes.json"), "w") as f:
        json.dump(verts, f, indent=1)
    print(f"  {len(verts)} vertexes")

    # Linedefs
    lines = []
    for l in e.linedefs:
        lines.append({
            "vx_a": l.vx_a,
            "vx_b": l.vx_b,
            "front": l.front,
            "back": l.back,
            "flags": l.flags,
            "action": l.action,
            "tag": l.tag,
        })
    with open(os.path.join(output_dir, f"{prefix}_linedefs.json"), "w") as f:
        json.dump(lines, f, indent=1)
    print(f"  {len(lines)} linedefs")

    # Sidedefs
    sides = []
    for s in e.sidedefs:
        sides.append({
            "off_x": s.off_x,
            "off_y": s.off_y,
            "tx_up": s.tx_up,
            "tx_low": s.tx_low,
            "tx_mid": s.tx_mid,
            "sector": s.sector,
        })
    with open(os.path.join(output_dir, f"{prefix}_sidedefs.json"), "w") as f:
        json.dump(sides, f, indent=1)
    print(f"  {len(sides)} sidedefs")

    # Sectors
    sects = []
    for sec in e.sectors:
        sects.append({
            "z_floor": sec.z_floor,
            "z_ceil": sec.z_ceil,
            "tx_floor": sec.tx_floor,
            "tx_ceil": sec.tx_ceil,
            "light": sec.light,
            "type": sec.type,
            "tag": sec.tag,
        })
    with open(os.path.join(output_dir, f"{prefix}_sectors.json"), "w") as f:
        json.dump(sects, f, indent=1)
    print(f"  {len(sects)} sectors")

    # Things
    things = []
    for t in e.things:
        things.append({
            "x": t.x,
            "y": t.y,
            "angle": t.angle,
            "type": t.type,
            "flags": t.flags,
        })
    with open(os.path.join(output_dir, f"{prefix}_things.json"), "w") as f:
        json.dump(things, f, indent=1)
    print(f"  {len(things)} things")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        print(f"Usage: {sys.argv[0]} <wad_path> <map_name> <output_dir>")
        sys.exit(1)
    wad_path, map_name, output_dir = sys.argv[1], sys.argv[2].upper(), sys.argv[3]
    print(f"Generating reference data for {map_name} from {os.path.basename(wad_path)}")
    dump_map(wad_path, map_name, output_dir)
    print("Done")
