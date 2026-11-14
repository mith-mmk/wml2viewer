"""Original generated JPEGs: four distinct 8x8 patches, no third-party image data.
DC-only blocks, unit quantizers, complete custom Huffman tables. Python stdlib only.
"""
from pathlib import Path
import struct
ROOT = Path(__file__).parent

def segment(marker, payload):
    return bytes([255, marker]) + struct.pack('>H', len(payload)+2) + payload

def entropy(bits):
    bits += '1' * ((-len(bits)) % 8)
    return bytes(int(bits[i:i+8],2) for i in range(0,len(bits),8)).replace(b'\xff', b'\xff\x00')

def dc(value):
    size = abs(value).bit_length()
    amplitude = value if value >= 0 else (1 << size) - 1 + value
    return f'{size:04b}' + (f'{amplitude:0{size}b}' if size else '')

def make(progressive):
    ids = b'CMYK'
    out = b'\xff\xd8' + segment(0xee,b'Adobe\x00\x64\x00\x00\x00\x00\x00')
    out += segment(0xdb,b'\x00' + bytes([1])*64)
    out += segment(0xc2 if progressive else 0xc0, b'\x08\x00\x10\x00\x10\x04' + b''.join(bytes([c,17,0]) for c in ids))
    out += segment(0xc4, bytes([0])+bytes([0,0,0,12]+[0]*12)+bytes(range(12)) + bytes([16])+bytes([1]+[0]*15)+bytes([0]))
    out += segment(0xda,bytes([4])+b''.join(bytes([c,0]) for c in ids)+bytes([0,0 if progressive else 63,0]))
    previous = [0]*4
    bits = ''
    for block in range(4):
        for c in range(4):
            sample = [180+block*12,200-block*8,210-block*10,240][c]
            coefficient = 8*(sample-128)
            bits += dc(coefficient-previous[c]) + ('' if progressive else '0')
            previous[c] = coefficient
    out += entropy(bits)
    if progressive:
        for c in ids:
            out += segment(0xda,bytes([1,c,0,1,63,0])) + entropy('0000')
    return out+b'\xff\xd9'

for progressive in [False,True]:
    (ROOT / ('jpeg-cmyk-blocks' + ('-progressive' if progressive else '') + '.jpg')).write_bytes(make(progressive))
