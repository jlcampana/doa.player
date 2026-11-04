#!/usr/bin/env python3
"""Shows which objects the game rewrites with new normals, from pickles of
doa2u_capture_objects.py. Read-only on the files. doa2u_normal_check.py CAPTURE.pkl ...

Prints, per object: vertices whose position moved from the file, vertices whose normal
moved, and normals that stop being unit length."""
import pickle
import struct
import sys

import numpy as np


def main():
    for path in sys.argv[1:]:
        capture = pickle.load(open(path, 'rb'))
        for fighter, stem in capture['fighters']:
            for obj, count, stride, _ in capture['dyn'][fighter]:
                rest = capture['rest'][fighter][obj]
                columns = np.frombuffer(rest, dtype='<f4').reshape(count, stride // 4)
                moved = np.zeros(count, bool)
                turned = np.zeros(count, bool)
                worst = 0.0
                for frame in capture['frames']:
                    live = np.frombuffer(frame['v'][fighter][obj], dtype='<f4').reshape(count, stride // 4)
                    moved |= np.abs(live[:, :3] - columns[:, :3]).max(axis=1) > 1e-4
                    # Weighted objects hold extra floats before the normal; none are in this list.
                    turned |= np.abs(live[:, 3:6] - columns[:, 3:6]).max(axis=1) > 1e-3
                    worst = max(worst, float(np.abs(np.linalg.norm(live[:, 3:6], axis=1) - 1).max()))
                print(f'{stem} {obj:3d} verts {count:5d} moved {moved.sum():5d} normals changed {turned.sum():5d} '
                      f'unit error {worst:.3f}')


if __name__ == '__main__':
    main()
