#!/usr/bin/env python3
"""Where the whole-screen SSIM of a capture is lost.

Uses the same Gaussian SSIM as compare.py, per pixel, averaged over RGB, and sums it over named
rectangles of the reference window (CSS pixels). The share column says how much of the total
loss a region holds, the local column how bad that region itself is (0 = identical).
Requires Pillow and numpy. Reports only; the gate stays in compare.py.
"""
import argparse
import sys
from pathlib import Path

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare as C  # noqa: E402

# Regions of the 1200x800 idle discovery screen, CSS px: (x0, y0, x1, y1).
REGIONS = {
    'sidebar brand': (0, 0, 252, 70),
    'sidebar nav': (0, 70, 252, 720),
    'header back/forward': (252, 0, 360, 88),
    'header search': (357, 20, 580, 70),
    'header right': (900, 0, 1200, 88),
    'guide orb': (327, 233, 399, 305),
    'guide title+description': (327, 326, 1125, 392),
    'guide cards': (327, 407, 1125, 576),
    'guide rest': (286, 192, 1166, 617),
    'player left': (0, 720, 420, 800),
    'player center': (420, 720, 780, 800),
    'player right': (780, 720, 1200, 800),
}


def loss_map(reference, actual):
    a = np.asarray(Image.open(reference).convert('RGB'), dtype=np.float64)
    b = np.asarray(Image.open(actual).convert('RGB'), dtype=np.float64)
    if a.shape != b.shape:
        raise SystemExit('image dimensions differ')
    g = C.gaussian

    def one(x, y):
        mx, my = g(x), g(y)
        vx, vy = g(x * x) - mx * mx, g(y * y) - my * my
        c = g(x * y) - mx * my
        k1, k2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
        return ((2 * mx * my + k1) * (2 * c + k2)) / ((mx * mx + my * my + k1) * (vx + vy + k2))

    return 1 - np.mean([one(a[:, :, i], b[:, :, i]) for i in range(3)], axis=0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', required=True, help='reference PNG')
    parser.add_argument('--actual', required=True, help='GPUI PNG')
    parser.add_argument('--dpr', type=float, default=2.0)
    args = parser.parse_args()
    loss = loss_map(args.reference, args.actual)
    total = loss.sum()
    print(f'SSIM {1 - loss.mean():.4f}  (gate 0.97)')
    print(f'{"region":28s} {"share":>7s} {"local":>7s}')
    for name, box in REGIONS.items():
        # The valid Gaussian window trims 5 px on every side.
        x0, y0, x1, y1 = [max(int(v * args.dpr) - 5, 0) for v in box]
        part = loss[y0:y1, x0:x1]
        if part.size:
            print(f'{name:28s} {part.sum() / total * 100:6.1f}% {part.mean():7.3f}')


if __name__ == '__main__':
    main()
