#!/bin/sh
set -eu

project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PROJECT_ROOT="$project_root" python3 - <<'PY'
import json
import os
import re
from pathlib import Path

root = Path(os.environ["PROJECT_ROOT"])
strings_dir = root / "ios" / "Wml2Viewer"

def strings_keys(path: Path) -> set[str]:
    keys = set()
    pattern = re.compile(r'^"((?:\\.|[^"\\])*)"\s*=')
    for line in path.read_text(encoding="utf-8").splitlines():
        match = pattern.match(line)
        if match:
            keys.add(match.group(1))
    return keys

english = strings_keys(strings_dir / "en.lproj" / "Localizable.strings")
japanese = strings_keys(strings_dir / "ja.lproj" / "Localizable.strings")
if english != japanese:
    print("English/Japanese .strings key sets differ", flush=True)
    print("missing in English:", sorted(japanese - english), flush=True)
    print("missing in Japanese:", sorted(english - japanese), flush=True)
    raise SystemExit(1)

catalog = json.loads((strings_dir / "Localizable.xcstrings").read_text(encoding="utf-8"))
catalog_keys = set(catalog.get("strings", {}))
if not english <= catalog_keys:
    print(".strings keys missing from String Catalog:", sorted(english - catalog_keys), flush=True)
    raise SystemExit(1)

missing_japanese = sorted(
    key for key, value in catalog.get("strings", {}).items()
    if "ja" not in value.get("localizations", {})
)
if missing_japanese:
    print("String Catalog keys missing Japanese localization:", missing_japanese, flush=True)
    raise SystemExit(1)

print(f"localization keys: en/ja={len(english)}, catalog={len(catalog_keys)}")
PY
