#!/usr/bin/env python3
"""Compare real native captures. No resizing, masks or font-region exclusions.
Requires Pillow and numpy. Exit 1 is a failed parity gate, not a tool failure.
"""
import argparse
import json
import sys
from pathlib import Path
import numpy as np
from PIL import Image

KEYS = {'player': 'player-bar'}
def gaussian(values):
    weights = np.exp(-(np.arange(-5, 6, dtype=np.float64) ** 2) / (2 * 1.5 ** 2))
    weights /= weights.sum()
    # Valid, separable 11x11 Gaussian convolution. Same pixel grid for both inputs.
    horizontal = sum(weight * values[:, i:i + values.shape[1] - 10] for i, weight in enumerate(weights))
    return sum(weight * horizontal[i:i + horizontal.shape[0] - 10] for i, weight in enumerate(weights))

def ssim(reference, actual):
    a = np.asarray(reference, dtype=np.float64)
    b = np.asarray(actual, dtype=np.float64)
    ma, mb = gaussian(a), gaussian(b)
    va, vb = gaussian(a*a)-ma*ma, gaussian(b*b)-mb*mb
    cov = gaussian(a*b)-ma*mb
    return float(np.mean(((2*ma*mb+(0.01*255)**2)*(2*cov+(0.03*255)**2)) /
                         ((ma*ma+mb*mb+(0.01*255)**2)*(va+vb+(0.03*255)**2))))

def compare(reference_json, actual_json, reference_png, actual_png):
    ref = json.loads(Path(reference_json).read_text())
    actual = json.loads(Path(actual_json).read_text())
    errors, geometry = [], {}
    if ref['viewport'] != actual['viewport']:
        errors.append('viewport or DPR differs')
    if ref['theme'] != actual['theme']:
        errors.append('theme differs')
    for name, element in ref['elements'].items():
        if element is None:
            continue
        expected = element['bounds']
        if expected['width'] == 0 and expected['height'] == 0:
            continue
        observed = actual['elements'].get(KEYS.get(name, name))
        if observed is None:
            errors.append(f'missing visible element: {name}')
            continue
        delta = {axis: abs(expected[axis] - observed['bounds'][axis]) for axis in ('x','y','width','height')}
        geometry[name] = {'delta_px': delta, 'pass': max(delta.values()) <= 1}
        if not geometry[name]['pass']:
            errors.append(f'geometry exceeds 1px: {name}')
    a,b=Image.open(reference_png).convert('RGB'),Image.open(actual_png).convert('RGB')
    score = None
    if a.size != b.size:
        errors.append('image dimensions differ')
    elif a.size != (int(ref['viewport']['width']*ref['viewport']['dpr']),int(ref['viewport']['height']*ref['viewport']['dpr'])):
        errors.append('image dimensions do not match declared viewport')
    else:
        score = ssim(a,b)
        if score < 0.97:
            errors.append('whole-screen SSIM below 0.97')
    return {'reference':str(reference_json), 'actual':str(actual_json), 'reference_variant':ref.get('variant'),
            'geometry':geometry, 'max_delta_px':max((max(g['delta_px'].values()) for g in geometry.values()),default=None),
            'ssim':score, 'ssim_threshold':0.97, 'errors':errors, 'pass':not errors,
            'scope':'Captured element bounds plus unmasked whole-screen RGB SSIM; individual style/color values are not yet gated.'}

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference',required=True,help='Reference JSON; adjacent PNG required')
    parser.add_argument('--actual',required=True,help='GPUI JSON; adjacent PNG required')
    parser.add_argument('--report',required=True)
    args=parser.parse_args()
    report=compare(args.reference,args.actual,Path(args.reference).with_suffix('.png'),Path(args.actual).with_suffix('.png'))
    path=Path(args.report);path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ('pass','max_delta_px','ssim','errors')},ensure_ascii=False))
    sys.exit(0 if report['pass'] else 1)
