#!/usr/bin/env python3
import sys, struct

DATA_OFFSET = 4096
STORAGE_HEADER_MAGIC = bytes([0x1C, 0x53, 0x4F, 0x00])
SEGMENT_HEADER_MAGIC = bytes([0x1E, 0x53, 0x4F, 0x00])

f = open(sys.argv[1], 'rb')
b = f.read()
f.close()

def hexstr(b):
    return ''.join([f'{c:02X}' for c in b])

class Prior:
    def __init__(self, c, subtype_decoder):
        self.values = []
        tag = c.get_leb()
        print(f'bytes: ({tag}): {hexstr(c.peek_bytes(8))}')
        if tag > 2:
            raise Exception('too many items in prior')
        for i in range(0, tag):
            print(f'iter: ({i}): {hexstr(c.peek_bytes(8))}')
            v = subtype_decoder(c)
            self.values.append(v)

    def __str__(self):
        return f'{self.values}'

    def __iter__(self):
        return self.values.__iter__()

class Cursor:
    def __init__(self, b: bytes, /, rkyv=False):
        self.b = b
        self.c = 0
        self.rkyv = rkyv

    def align(self, to: int):
        if self.c % to != 0:
            self.c += to - (self.c % to)
    
    def get_u8(self):
        v = self.b[self.c]
        self.c += 1
        return v

    def get_bool(self):
        return self.get_u8() != 0

    def get_u16(self):
        if self.rkyv:
            self.align(4) # Don't know if rkyv word-aligns u16 too but I assume so
        b = self.get_bytes(2)
        return struct.unpack('<H', b)[0]

    def get_u32(self):
        if self.rkyv:
            self.align(4)
        b = self.get_bytes(4)
        return struct.unpack('<I', b)[0]

    def get_option(self, subtype_decoder):
        exists = self.get_bool()
        if exists:
            return subtype_decoder(self)
        return None

    def get_leb(self):
        i = 0
        vl = []
        b = self.get_u8()
        while b & 0x80 != 0:
            vl.append(b & 0x7F)
            b = self.get_u8()
        vl.append(b)
        v = 0
        for vv in reversed(vl):
            v = (v << 7) | vv
        return v

    def get_prior(self, subtype_decoder):
        return Prior(self, subtype_decoder)

    def peek_bytes(self, n: int):
        return self.b[self.c:self.c + n]

    def get_bytes(self, n: int):
        b = self.peek_bytes(n)
        self.c += n
        return b

    def get_byte_array(self):
        l = self.get_leb()
        return self.get_bytes(l)

    def get_array(self, subtype_decoder):
        l = self.get_leb()
        arr = []
        for i in range(0, l):
            arr.append(subtype_decoder(self))
        return arr

class Header:
    def __init__(self, b):
        c = Cursor(b, rkyv=True)
        if c.get_bytes(4) != STORAGE_HEADER_MAGIC:
            raise Exception('bad header magic')

        self.epoch = c.get_u32()
        self.graph_id = c.get_option(lambda c: c.get_bytes(32))

        c.align(4)
        self.head = c.get_option(lambda c: (c.get_u32(), c.get_u32()))

        self.stored_bytes = c.get_u32()

    def __str__(self):
        return f'Header:\n  epoch {self.epoch}\n  graph_id {hexstr(self.graph_id) if self.graph_id is not None else None}\n  head: {self.head}\n  stored bytes: {self.stored_bytes}'

def decode_location(c):
    l1 = c.get_leb()
    l2 = c.get_leb()
    return (l1, l2)

def decode_address(c):
    l1 = c.get_bytes(32)
    l2 = c.get_leb()
    return (l1, l2)

class Segment:
    def __init__(self, offset):
        self.offset = offset
        self.read()

    def read(self):
        if b[self.offset + DATA_OFFSET:self.offset + DATA_OFFSET + 4] != SEGMENT_HEADER_MAGIC:
            raise Exception(f'bad segment header magic @{self.offset}')

        (self.size,) = struct.unpack_from('<I', b, self.offset + DATA_OFFSET + 4)

        self.data = b[self.offset + DATA_OFFSET + 8:self.offset + DATA_OFFSET + 8 + self.size]

        self.decode_data()

    def decode_data(self):
        c = Cursor(self.data)
        offset = c.get_leb()
        self.prior = c.get_prior(decode_location)
        self.parents = c.get_prior(decode_address)
        self.policy = c.get_bytes(32)
        self.facts = c.get_leb()
        commands = c.get_array(CommandData)
        max_cut = c.get_leb()
        skip_list = c.get_array(lambda c: ((c.get_leb(), c.get_leb()), c.get_leb()))

        if len(self.parents.values) == 0:
            self.type = 'init'
        elif len(self.parents.values) == 1:
            self.type = 'basic'
        elif len(self.parents.values) == 2:
            self.type = 'merge'

    def __str__(self):
        return f'Segment {self.offset} ({self.type}):\n  size: {self.size}\n  prior: {self.prior}\n  parents: {self.parents}'

class CommandData:
    def __init__(self, c):
        self.id = c.get_bytes(32)
        self.priority = c.get_leb()
        if self.priority == 1: # Basic
            self.priority_value = c.get_leb()
        else:
            self.priority_value = None
        self.policy = c.get_option(lambda c: c.get_byte_array())
        self.data = c.get_byte_array()
        self.updates = c.get_array(lambda c: (c.get_string(), c.get_array(lambda c: c.get_byte_array()), c.get_option(lambda c: c.get_byte_array())))

segments = {}
h = Header(b)
print(h)
head_seg = Segment(h.head[0])
print(head_seg)

def walk_segment_tree(offset):
    if offset in segments:
        return
    seg = Segment(offset)
    segments[offset] = seg
    if seg.prior is None:
        return
    for p in seg.prior:
        walk_segment_tree(p[0])

walk_segment_tree(h.head[0])
dot = open('graph.dot', 'w')
dot.write('digraph {\n')
for s in segments.values():
    dot.write(f'    segment_{s.offset} [label="Segment {s.offset }", shape=rectangle]\n')
    if s.prior is not None:
        for p in s.prior:
            dot.write(f'    segment_{s.offset} -> segment_{p[0]}\n')
dot.write('}\n')
dot.close()
