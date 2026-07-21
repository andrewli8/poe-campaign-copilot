#!/usr/bin/env python3
"""Extract zone layout notes and images from poelayouts.docx into
content/layouts/. Stdlib only. Deterministic.

Usage: python3 tools/extract_poelayouts.py /path/to/poelayouts.docx
"""
import json
import re
import sys
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "content" / "layouts"
AREAS_JSON = REPO / "vendor" / "exile-leveling" / "data" / "areas.json"
OVERRIDES_JSON = Path(__file__).resolve().parent / "poelayouts-mapping-overrides.json"

ACT_NUM = {
    "Act One": 1, "Act Two": 2, "Act Three": 3, "Act Four": 4, "Act Five": 5,
    "Act Six": 6, "Act Seven": 7, "Act Eight": 8, "Act Nine": 9, "Act Ten": 10,
}

SOURCE = {
    "document": "PoE Map Layouts community compilation (poelayouts.docx)",
    "images_author": "Engineering Eternity",
    "notes_author": "community compilation author",
}


def text_of(fragment):
    return "".join(re.findall(r"<w:t[^>]*>([^<]*)</w:t>", fragment))


def images_of(fragment, rid_to_file):
    rids = re.findall(r'<a:blip[^>]*r:embed="([^"]+)"', fragment)
    return [rid_to_file[r] for r in rids if r in rid_to_file]


def norm(name):
    name = name.replace("’", "'").lower().strip()
    name = re.sub(r"^the ", "", name)
    return re.sub(r"\s+", " ", name)


def parse_docx(docx_path):
    """Yield zone dicts {act, heading, texts_desc, texts_notes, images} in doc order."""
    with zipfile.ZipFile(docx_path) as zf:
        xml = zf.read("word/document.xml").decode("utf-8")
        rels = zf.read("word/_rels/document.xml.rels").decode("utf-8")
    rid_to_file = dict(
        re.findall(r'Id="(rId\d+)"[^>]*Target="media/([^"]+)"', rels)
    )
    body = xml[xml.find("<w:body>"):]
    elems = re.findall(
        r"(<w:p [^>]*>.*?</w:p>|<w:p/>|<w:p>.*?</w:p>|<w:tbl>.*?</w:tbl>)",
        body,
        re.S,
    )

    current_act = None
    zones = []
    for e in elems:
        if e.startswith("<w:tbl>"):
            if current_act is None:
                continue  # preamble tables (none expected, but be safe)
            rows = re.findall(r"<w:tr[ >].*?</w:tr>", e, re.S)
            if not rows:
                continue
            header = [
                text_of(c).strip()
                for c in re.findall(r"<w:tc>.*?</w:tc>", rows[0], re.S)
            ]
            cols = [
                {"act": current_act, "heading": h, "descriptions": [],
                 "notes": [], "images": []}
                for h in header
            ]
            for row_index, r in enumerate(rows[1:], start=1):
                cells = re.findall(r"<w:tc>.*?</w:tc>", r, re.S)
                for i, c in enumerate(cells[: len(cols)]):
                    cols[i]["images"] += images_of(c, rid_to_file)
                    t = text_of(c).strip()
                    if t:
                        key = "notes" if t.startswith("Note:") else "descriptions"
                        cols[i][key].append(t)
            zones.extend(c for c in cols if c["heading"])
        else:
            t = text_of(e).strip()
            if t in ACT_NUM:
                current_act = ACT_NUM[t]
    return zones, rid_to_file


def build_area_index(areas):
    by_act = {}
    for a in areas.values():
        by_act.setdefault(a["act"], {}).setdefault(norm(a["name"]), []).append(a["id"])
    return by_act


def resolve_area_ids(zone, by_act, overrides):
    key = f"{zone['act']}|{zone['heading']}"
    if key in overrides:
        v = overrides[key]
        return v if isinstance(v, list) else [v]
    n = norm(zone["heading"])
    candidates = by_act.get(zone["act"], {}).get(n)
    if not candidates:
        m = re.match(r"^(.*?) (\d)$", n)
        if m:
            candidates = by_act.get(zone["act"], {}).get(
                f"{m.group(1)} level {m.group(2)}"
            )
    if candidates and len(candidates) == 1:
        return [candidates[0]]
    raise SystemExit(
        f"UNMAPPED docx heading: act {zone['act']} {zone['heading']!r} "
        f"(candidates: {candidates}) — add it to {OVERRIDES_JSON.name}"
    )


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    docx_path = Path(sys.argv[1])
    areas = json.loads(AREAS_JSON.read_text())
    overrides = json.loads(OVERRIDES_JSON.read_text())
    by_act = build_area_index(areas)

    zones, _ = parse_docx(docx_path)

    # Merge zone entries per area id.
    entries = {}
    mapping = {}
    for zone in zones:
        for area_id in resolve_area_ids(zone, by_act, overrides):
            area = areas[area_id]
            mapping.setdefault(f"{zone['act']}|{zone['heading']}", []).append(area_id)
            e = entries.setdefault(
                area_id,
                {
                    "area_id": area_id,
                    "act": area["act"],
                    "display_name": area["name"],
                    "docx_headings": [],
                    "descriptions": [],
                    "notes": [],
                    "images": [],
                    "source": SOURCE,
                    "audit": {
                        "status": "unaudited",
                        "verified_patch": None,
                        "correction": None,
                    },
                },
            )
            if zone["heading"] not in e["docx_headings"]:
                e["docx_headings"].append(zone["heading"])
            e["descriptions"] += [d for d in zone["descriptions"] if d not in e["descriptions"]]
            e["notes"] += [n for n in zone["notes"] if n not in e["notes"]]
            e["images"] += [i for i in zone["images"] if i not in e["images"]]

    # Write entries.
    used_images = set()
    for area_id in sorted(entries):
        e = entries[area_id]
        used_images.update(e["images"])
        act_dir = OUT / f"act-{e['act']}"
        act_dir.mkdir(parents=True, exist_ok=True)
        (act_dir / f"{area_id}.json").write_text(
            json.dumps(e, indent=2, ensure_ascii=False) + "\n"
        )

    # Copy referenced images.
    assets = OUT / "assets"
    assets.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(docx_path) as zf:
        for name in sorted(used_images):
            (assets / name).write_bytes(zf.read(f"word/media/{name}"))

    (OUT / "mapping.json").write_text(
        json.dumps(mapping, indent=2, ensure_ascii=False, sort_keys=True) + "\n"
    )

    with_images = sum(1 for e in entries.values() if e["images"])
    acts = sorted({e["act"] for e in entries.values()})
    print(f"zone slots parsed: {len(zones)}")
    print(f"area entries written: {len(entries)}")
    print(f"entries with images: {with_images}")
    print(f"images copied: {len(used_images)}")
    print(f"acts covered: {acts}")


if __name__ == "__main__":
    main()
