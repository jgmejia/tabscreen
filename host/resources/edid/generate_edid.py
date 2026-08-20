#!/usr/bin/env python3
"""Generate an EDID 1.4 binary for the HONOR Pad X8a virtual output.

Encodes the tablet's real panel spec (1200x1920, measured in Phase 0) as two
detailed timing descriptors (60Hz preferred, 90Hz secondary), using CVT
timings from `cvt 1200 1920 60` / `cvt 1200 1920 90`. See
docs/adr/0008-edid-override-virtual-output.md.

Usage: python3 generate_edid.py > tabscreen-honor-pad-x8a.bin
Validate with: edid-decode tabscreen-honor-pad-x8a.bin
"""

# (pixel_clock_mhz, hactive, hsyncstart, hsyncend, htotal,
#  vactive, vsyncstart, vsyncend, vtotal, hsync_positive, vsync_positive)
# from `cvt 1200 1920 60` / `cvt 1200 1920 90` — CVT standard blanking.
MODE_60HZ = (196.50, 1200, 1296, 1424, 1648, 1920, 1923, 1933, 1989, False, True)
MODE_90HZ = (302.75, 1200, 1304, 1432, 1664, 1920, 1923, 1933, 2024, False, True)

# HONOR Pad X8a: ~11" diagonal, 1200x1920 -> ~148mm x 237mm physical.
H_SIZE_MM = 148
V_SIZE_MM = 237


def pack_manufacturer_id(three_letters: str) -> bytes:
    assert len(three_letters) == 3 and three_letters.isupper()
    l1, l2, l3 = (ord(c) - ord("A") + 1 for c in three_letters)
    value = (l1 << 10) | (l2 << 5) | l3
    return value.to_bytes(2, "big")


def pack_detailed_timing(mode, h_mm: int, v_mm: int) -> bytes:
    pclk_mhz, hact, hss, hse, htot, vact, vss, vse, vtot, hpos, vpos = mode

    pclk_units = round(pclk_mhz * 1000 / 10)  # 10kHz units
    h_blank = htot - hact
    v_blank = vtot - vact
    h_sync_off = hss - hact
    h_sync_wid = hse - hss
    v_sync_off = vss - vact
    v_sync_wid = vse - vss

    for name, val, bits in [
        ("h_active", hact, 12), ("h_blank", h_blank, 12),
        ("v_active", vact, 12), ("v_blank", v_blank, 12),
        ("h_sync_off", h_sync_off, 10), ("h_sync_wid", h_sync_wid, 10),
        ("v_sync_off", v_sync_off, 6), ("v_sync_wid", v_sync_wid, 6),
    ]:
        assert 0 <= val < (1 << bits), f"{name}={val} exceeds {bits} bits"

    b = bytearray(18)
    b[0], b[1] = pclk_units.to_bytes(2, "little")
    b[2] = hact & 0xFF
    b[3] = h_blank & 0xFF
    b[4] = ((hact >> 8) << 4) | (h_blank >> 8)
    b[5] = vact & 0xFF
    b[6] = v_blank & 0xFF
    b[7] = ((vact >> 8) << 4) | (v_blank >> 8)
    b[8] = h_sync_off & 0xFF
    b[9] = h_sync_wid & 0xFF
    b[10] = ((v_sync_off & 0xF) << 4) | (v_sync_wid & 0xF)
    b[11] = (
        ((h_sync_off >> 8) << 6)
        | ((h_sync_wid >> 8) << 4)
        | ((v_sync_off >> 4) << 2)
        | (v_sync_wid >> 4)
    )
    b[12] = h_mm & 0xFF
    b[13] = v_mm & 0xFF
    b[14] = ((h_mm >> 8) << 4) | (v_mm >> 8)
    b[15] = 0  # horizontal border
    b[16] = 0  # vertical border
    # Digital separate sync; polarity per cvt output (-hsync +vsync -> 0,1).
    b[17] = (0b11 << 3) | ((1 if vpos else 0) << 2) | ((1 if hpos else 0) << 1)
    return bytes(b)


def pack_text_descriptor(tag: int, text: str) -> bytes:
    payload = (text + "\n").ljust(13, " ").encode("ascii")[:13]
    return bytes([0, 0, 0, tag, 0]) + payload


def pack_dummy_descriptor() -> bytes:
    return bytes([0, 0, 0, 0x10, 0]) + bytes(13)


def pack_chromaticity(primaries) -> bytes:
    def to10(coord: float) -> int:
        v = round(coord * 1024)
        assert 0 <= v < 1024
        return v

    rx, ry, gx, gy, bx, by, wx, wy = (to10(c) for c in primaries)
    b = bytearray(10)
    b[0] = ((rx & 0x3) << 6) | ((ry & 0x3) << 4) | ((gx & 0x3) << 2) | (gy & 0x3)
    b[1] = ((bx & 0x3) << 6) | ((by & 0x3) << 4) | ((wx & 0x3) << 2) | (wy & 0x3)
    b[2] = rx >> 2
    b[3] = ry >> 2
    b[4] = gx >> 2
    b[5] = gy >> 2
    b[6] = bx >> 2
    b[7] = by >> 2
    b[8] = wx >> 2
    b[9] = wy >> 2
    return bytes(b)


def build_edid() -> bytes:
    b = bytearray(128)
    b[0:8] = bytes([0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00])
    b[8:10] = pack_manufacturer_id("TBS")  # tabscreen, unregistered/local-use ID
    b[10:12] = (1).to_bytes(2, "little")  # product code
    b[12:16] = (1).to_bytes(4, "little")  # serial number
    b[16] = 1  # week of manufacture
    b[17] = 2026 - 1990  # year of manufacture
    b[18] = 1  # EDID version
    b[19] = 4  # EDID revision (1.4)

    # Video input: digital, 8 bpc, DisplayPort interface.
    b[20] = 0b1_010_0101
    b[21] = round(H_SIZE_MM / 10)  # cm
    b[22] = round(V_SIZE_MM / 10)  # cm
    b[23] = round(2.2 * 100) - 100  # gamma 2.2
    # Feature support: RGB 4:4:4 only, preferred timing is descriptor 1.
    b[24] = 0b000_00_0_1_0

    b[25:35] = pack_chromaticity(
        (0.640, 0.330, 0.300, 0.600, 0.150, 0.060, 0.3127, 0.3290)  # sRGB
    )

    b[35] = 0  # established timings I
    b[36] = 0  # established timings II
    b[37] = 0  # manufacturer reserved timing
    b[38:54] = bytes([0x01] * 16)  # standard timings 1-8: all unused

    b[54:72] = pack_detailed_timing(MODE_60HZ, H_SIZE_MM, V_SIZE_MM)  # preferred
    b[72:90] = pack_detailed_timing(MODE_90HZ, H_SIZE_MM, V_SIZE_MM)
    b[90:108] = pack_text_descriptor(0xFC, "TabScreen")  # monitor name
    b[108:126] = pack_dummy_descriptor()

    b[126] = 0  # no extensions
    b[127] = (256 - sum(b[0:127]) % 256) % 256  # checksum

    assert sum(b) % 256 == 0
    return bytes(b)


if __name__ == "__main__":
    import sys

    sys.stdout.buffer.write(build_edid())
