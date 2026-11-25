"""Run scripts/make-demo-vault.py with a pinned clock, so the golden vault is reproducible.

    python3 rust/fixtures/tools/make-vault.py <out_dir> [<today YYYY-MM-DD>]

The generator copies 14.09 under «today's Kyiv date» (its note-23 demo day). With the
real clock that date changes every day and so would the golden files; here «now» is
pinned (default 2026-10-26, the Monday of the 2026 week when Europe has left summer
time and the US has not — a DST-mismatch week, see docs/rust/TEST_PLAN.md).
Everything else the generator does is already deterministic (seeded random).
"""
from __future__ import annotations

import datetime as _datetime
import runpy
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
out = Path(sys.argv[1]).resolve()
today = sys.argv[2] if len(sys.argv) > 2 else "2026-10-26"
# 06:00 UTC = 09:00 Kyiv; the generator adds 3 h and keeps only the date.
pinned = _datetime.datetime.fromisoformat(f"{today}T06:00:00+00:00")


class _PinnedDateTime(_datetime.datetime):
    @classmethod
    def now(cls, tz=None):  # noqa: D102 - same signature as datetime.now
        return pinned if tz is not None else pinned.replace(tzinfo=None)


_datetime.datetime = _PinnedDateTime  # the generator does `from datetime import datetime`

# Start from an empty folder: the generator never deletes days it no longer makes.
if out.exists():
    shutil.rmtree(out)
sys.argv = [str(ROOT / "scripts" / "make-demo-vault.py"), str(out)]
runpy.run_path(sys.argv[0], run_name="__main__")
