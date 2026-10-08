#!/usr/bin/env python3
"""Portable architecture and boundary checks, in addition to the real CI tests."""
from pathlib import Path

root = Path(__file__).resolve().parents[1]
required = [
    "PRD.md", "TECHNICAL_DESIGN.md", "IMPLEMENTATION_PLAN.md",
    "Cargo.toml", "contracts/src/lib.rs", "runtime/src/lib.rs",
    "migrations/0001_core.sql", "migrations/0002_indexes.sql",
    "tests/schema_smoke.sql", ".github/workflows/ci.yml",
]
for rel in required:
    assert (root / rel).is_file(), f"Missing {rel}"

schema = (root / "migrations/0001_core.sql").read_text()
for table in ("users", "players", "characters", "items", "engines",
              "games", "worlds", "world_instances", "capabilities",
              "types", "components", "entities", "locations",
              "authority_leases", "engine_modules", "engine_bindings"):
    assert f"CREATE TABLE {table} (" in schema, table

runtime = (root / "runtime/src/lib.rs").read_text()
contract = (root / "contracts/src/lib.rs").read_text()
for name in ("NativeModule", "WorldPort", "RenderProvider",
             "GameAdapter", "InteractionReceiver"):
    assert f"trait {name}" in contract
for engine_name in ("Doom", "DOOM", "Cave Story", "Terraria", "Minecraft"):
    assert engine_name not in runtime, "Runtime must not know specific games"

print("Architecture boundary checks passed")
