#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Proteen Das
"""ImagineCAD plugin: holes equally spaced on a pitch circle.

Reads the request on standard input and answers with edit operations on
standard output (see docs/PLUGINS.md).
"""
import json
import math
import sys

req = json.load(sys.stdin)
if req.get("protocol") != 1:
    sys.exit("unsupported protocol")

inp = req["inputs"]
cx, cy = inp["center"]
pcd = float(inp["pcd"])
n = max(1, int(round(float(inp["count"]))))
hole = float(inp["hole"])
start = math.radians(float(inp["start"]))

actions = [
    {"op": "add", "layer": "CENTER", "shape": {"type": "circle", "c": [cx, cy], "r": pcd / 2}},
]
for i in range(n):
    a = start + 2 * math.pi * i / n
    x, y = cx + pcd / 2 * math.cos(a), cy + pcd / 2 * math.sin(a)
    actions.append({"op": "add", "layer": "OUTLINE", "shape": {"type": "circle", "c": [x, y], "r": hole / 2}})

# A note beside the first hole.
a = start
tip = [cx + (pcd / 2 + hole / 2) * math.cos(a), cy + (pcd / 2 + hole / 2) * math.sin(a)]
actions.append({
    "op": "add",
    "layer": "DIMENSIONS",
    "shape": {"type": "leader", "pts": [tip, [tip[0] + pcd * 0.35, tip[1] + pcd * 0.25]],
              "text": f"{n}× Ø{hole:g} EQ SP ON Ø{pcd:g} PCD"},
})

print(json.dumps({"message": f"{n} holes Ø{hole:g} on Ø{pcd:g}.", "actions": actions}))
