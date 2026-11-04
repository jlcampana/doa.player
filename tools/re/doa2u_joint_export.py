#!/usr/bin/env python3
"""Writes a pickle of doa2u_capture_objects.py as text for joint_pieces_match_captured_vertices.

doa2u_joint_export.py CAPTURE.pkl STEM OUT.txt"""
import pickle
import struct
import sys


def main():
    capture = pickle.load(open(sys.argv[1], 'rb'))
    fighter = next(f for f, stem in capture['fighters'] if stem == sys.argv[2])
    with open(sys.argv[3], 'w') as out:
        for frame in capture['frames']:
            print('matrices', *struct.unpack_from('<512f', frame['m'], fighter * 0x800), file=out)
            for obj, count, stride, _ in capture['dyn'][fighter]:
                data = frame['v'][fighter][obj]
                rows = [struct.unpack_from('<6f', data, i * stride) for i in range(count)]
                print('object', obj, *(x for r in rows for x in r[:3]), file=out)
                print('normals', obj, *(x for r in rows for x in r[3:]), file=out)


if __name__ == '__main__':
    main()
