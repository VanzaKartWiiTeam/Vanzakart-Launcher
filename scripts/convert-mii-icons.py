"""
Converte le icone dei tratti Mii in `src/lib/mii/icons.json`.

Le icone di partenza sono file `.axaml` (`MiiFace.axaml`, `MiiHair.axaml`...):
ogni `DrawingImage` è un gruppo di `GeometryDrawing` con un riempimento e/o un
contorno, e i colori sono segnaposto `TemplateColor1..5` che l'editor
sostituisce con i colori del Mii (pelle, capelli, iride...).

Il JSON risultante ha, per ogni tipo di tratto, la lista delle icone in ordine
di indice; ogni icona è `{"box": [x, y, w, h], "layers": [...]}`. Il riquadro
è quello del contenuto, contorni compresi, come lo calcola Avalonia: l'editor
lo usa come `viewBox`, così ogni icona riempie il suo bottone.

Uso:
    python scripts/convert-mii-icons.py <cartella MiiIcons> [uscita.json]
"""

from __future__ import annotations

import json
import math
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

AVALONIA = "{https://github.com/avaloniaui}"
XAML = "{http://schemas.microsoft.com/winfx/2006/xaml}"

# File .axaml -> nome del tratto nel JSON.
KINDS = {
    "MiiFace": "face",
    "MiiHair": "hair",
    "MiiEye": "eye",
    "MiiEyebrow": "eyebrow",
    "MiiNose": "nose",
    "MiiMouth": "mouth",
    "MiiMustache": "mustache",
    "MiiGoatee": "beard",
    "MiiGlasses": "glasses",
}

TOKEN = re.compile(r"[A-Za-z]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


def brush(value: str | None):
    """`TemplateColorN` diventa N, un colore diretto resta com'è."""
    if not value:
        return None
    match = re.search(r"TemplateColor(\d)", value)
    if match:
        return int(match.group(1))
    # Un solo tracciato di un occhio è rimasto con il ciano segnaposto dello
    # script originale: è il contorno, come negli occhi vicini.
    if value.lower() == "#00ffff":
        return 2
    return value.lower() if value.startswith("#") else value


def parse_path(data: str):
    """Punti campionati lungo il percorso, per calcolarne il riquadro."""
    tokens = TOKEN.findall(data)
    points: list[tuple[float, float]] = []
    index = 0
    command = ""
    x = y = 0.0
    start = (0.0, 0.0)
    last_control = None

    def number():
        nonlocal index
        value = float(tokens[index])
        index += 1
        return value

    def flag():
        # Negli archi i flag possono essere attaccati: "a256 256 0 1 0 0 512".
        nonlocal index
        token = tokens[index]
        if len(token) > 1 and token[0] in "01" and not token.startswith(("0.", "1.")):
            tokens[index] = token[1:]
            return float(token[0])
        index += 1
        return float(token)

    def cubic(p0, p1, p2, p3):
        for step in range(1, 33):
            t = step / 32
            u = 1 - t
            points.append(
                (
                    u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0],
                    u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1],
                )
            )

    def arc(p0, rx, ry, angle, large, sweep, p1):
        # Parametrizzazione al centro, come nelle specifiche SVG (F.6.5).
        if rx == 0 or ry == 0:
            points.append(p1)
            return
        phi = math.radians(angle)
        cos_phi, sin_phi = math.cos(phi), math.sin(phi)
        dx, dy = (p0[0] - p1[0]) / 2, (p0[1] - p1[1]) / 2
        x1 = cos_phi * dx + sin_phi * dy
        y1 = -sin_phi * dx + cos_phi * dy
        rx, ry = abs(rx), abs(ry)
        scale = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry)
        if scale > 1:
            rx *= math.sqrt(scale)
            ry *= math.sqrt(scale)
        numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
        denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1
        factor = math.sqrt(max(0.0, numerator / denominator)) if denominator else 0.0
        if large == sweep:
            factor = -factor
        cx1 = factor * rx * y1 / ry
        cy1 = -factor * ry * x1 / rx
        cx = cos_phi * cx1 - sin_phi * cy1 + (p0[0] + p1[0]) / 2
        cy = sin_phi * cx1 + cos_phi * cy1 + (p0[1] + p1[1]) / 2

        def angle_of(ux, uy):
            return math.atan2(uy, ux)

        theta = angle_of((x1 - cx1) / rx, (y1 - cy1) / ry)
        delta = angle_of((-x1 - cx1) / rx, (-y1 - cy1) / ry) - theta
        if sweep and delta < 0:
            delta += 2 * math.pi
        elif not sweep and delta > 0:
            delta -= 2 * math.pi
        for step in range(1, 65):
            t = theta + delta * step / 64
            points.append(
                (
                    cx + rx * math.cos(t) * cos_phi - ry * math.sin(t) * sin_phi,
                    cy + rx * math.cos(t) * sin_phi + ry * math.sin(t) * cos_phi,
                )
            )

    while index < len(tokens):
        if tokens[index].isalpha():
            command = tokens[index]
            index += 1
            if command in "Zz":
                x, y = start
                points.append(start)
                last_control = None
                continue
        relative = command.islower()
        op = command.upper()
        ox, oy = (x, y) if relative else (0.0, 0.0)

        if op == "M":
            x, y = ox + number(), oy + number()
            start = (x, y)
            points.append((x, y))
            command = "l" if relative else "L"
            last_control = None
        elif op == "L":
            x, y = ox + number(), oy + number()
            points.append((x, y))
            last_control = None
        elif op == "H":
            x = (x if relative else 0.0) + number()
            points.append((x, y))
            last_control = None
        elif op == "V":
            y = (y if relative else 0.0) + number()
            points.append((x, y))
            last_control = None
        elif op == "C":
            p1 = (ox + number(), oy + number())
            p2 = (ox + number(), oy + number())
            p3 = (ox + number(), oy + number())
            cubic((x, y), p1, p2, p3)
            last_control = p2
            x, y = p3
        elif op == "S":
            p1 = (2 * x - last_control[0], 2 * y - last_control[1]) if last_control else (x, y)
            p2 = (ox + number(), oy + number())
            p3 = (ox + number(), oy + number())
            cubic((x, y), p1, p2, p3)
            last_control = p2
            x, y = p3
        elif op == "A":
            rx, ry, angle = number(), number(), number()
            large, sweep = flag(), flag()
            end = (ox + number(), oy + number())
            arc((x, y), rx, ry, angle, large, sweep, end)
            x, y = end
            last_control = None
        else:
            raise ValueError(f"comando di percorso non gestito: {command}")

    return points


def convert(folder: Path):
    result: dict[str, list] = {}

    for stem, kind in KINDS.items():
        tree = ET.parse(folder / f"{stem}.axaml")
        icons: dict[int, dict] = {}

        for image in tree.getroot().iter(f"{AVALONIA}DrawingImage"):
            key = image.get(f"{XAML}Key", "")
            match = re.fullmatch(rf"{stem}(\d+)", key)
            if not match:
                continue

            layers = []
            xs: list[float] = []
            ys: list[float] = []
            for drawing in image.iter(f"{AVALONIA}GeometryDrawing"):
                data = drawing.get("Geometry", "").strip()
                if not data:
                    continue
                layer: dict = {"d": data}
                fill = brush(drawing.get("Brush"))
                if fill is not None:
                    layer["fill"] = fill

                pen = drawing.find(f"{AVALONIA}GeometryDrawing.Pen/{AVALONIA}Pen")
                half = 0.0
                if pen is not None:
                    layer["stroke"] = brush(pen.get("Brush"))
                    layer["width"] = float(pen.get("Thickness", "1"))
                    half = layer["width"] / 2
                layers.append(layer)

                for px, py in parse_path(data):
                    xs += [px - half, px + half]
                    ys += [py - half, py + half]

            box = [min(xs), min(ys), max(xs) - min(xs), max(ys) - min(ys)]
            icons[int(match.group(1))] = {
                "box": [round(value, 2) for value in box],
                "layers": layers,
            }

        result[kind] = [icons[index] for index in sorted(icons)]
        if sorted(icons) != list(range(len(icons))):
            raise ValueError(f"{stem}: indici non contigui")

    return result


def main():
    folder = Path(sys.argv[1])
    output = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("src/lib/mii/icons.json")
    icons = convert(folder)
    output.write_text(json.dumps(icons, separators=(",", ":")), encoding="utf-8")
    counts = ", ".join(f"{kind} {len(items)}" for kind, items in icons.items())
    print(f"{output}: {counts}")


if __name__ == "__main__":
    main()
