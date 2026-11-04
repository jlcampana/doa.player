#!/usr/bin/env python3
"""Checks the DOA2U chest spring formula against the running game, read-only.

Reads the spring state at 0x850538 (stride 0x70) and the chest matrix of both fighters, and
predicts each frame's value from the frame before. Run with fighters whose costume has it."""
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import numpy as np
from doa2u_live import Guest

STATE, STRIDE, MATRICES = 0x850538, 0x70, 0x875C20
g = Guest()
REFERENCE = np.array([0.0, 0.2, 0.3])
for f in range(2):
    pairs, last, start = [], None, time.time()
    while len(pairs) < 150 and time.time() - start < 10:
        block = g.r(STATE + f * STRIDE, 0x70)
        matrix = g.r(MATRICES + f * 0x800, 0x40)
        now = time.time()
        if last is None or block != last[0]:
            if last is not None and now - last[2] < 0.021:
                pairs.append((last, (block, matrix)))
            last = (block, matrix, now)
    errors, targets = [], []
    for (a, _, _), (b, m) in pairs:
        A, B = struct.unpack('<28f', a), struct.unpack('<28f', b)
        k, damping, push, rest = A[0:4]
        M = np.array(struct.unpack('<16f', m)).reshape(4, 4).T
        previous_point = np.array(A[4:7])
        local = np.linalg.inv(M) @ np.append(previous_point, 1.0)
        pull_y = REFERENCE[1] - local[1]
        drive = pull_y - A[13]
        half = rest * 0.5
        up = -M[1, 1]
        target = up * half + half
        velocity = damping * A[25] - (A[26] - target) * k - push * drive
        value = max(-1.0, min(1.0, A[26] + velocity))
        errors.append(abs(value - B[26]))
        targets.append(abs(B[24] - up))
    errors.sort()
    print('fighter', f, 'id', g.r(0x87AE31 + 0x94 * f, 1)[0], 'parameters', [round(v, 3) for v in A[0:4]], 'flag', struct.unpack('<I', a[0x6C:0x70])[0],
          'pairs', len(pairs), 'error median %.5f 95%% %.5f worst %.5f' % (errors[len(errors) // 2], errors[len(errors) * 95 // 100], errors[-1]),
          'value range %.3f..%.3f' % (min(struct.unpack('<28f', p[1][0])[26] for p in pairs), max(struct.unpack('<28f', p[1][0])[26] for p in pairs)),
          'up error %.5f' % max(targets))
