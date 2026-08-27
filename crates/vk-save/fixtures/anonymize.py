#!/usr/bin/env python3
"""Produce le fixture anonime di `rksys.dat` e `RFL_DB.dat`.

I due file di partenza sono salvataggi **reali**: contengono nomi utente,
friend code e gli identificativi della console che ha creato ogni Mii. Le
fixture nel repository devono conservare la struttura byte per byte — è quello
che i test verificano — ma non un solo dato personale.

Lo script fa tre cose:

1. tronca il file alla dimensione utile (Dolphin alloca il file NAND molto piu
   grande, il resto e zeri);
2. sostituisce **ovunque compaiano** nomi, profile ID, Mii ID e system ID, non
   solo nei campi noti: il salvataggio ne tiene copie anche nei record dei
   tempi e in regioni di cui non si conosce il significato;
3. ricalcola i checksum con un'implementazione indipendente (`zlib` per il
   CRC-32 di `rksys.dat`, un CRC-16/CCITT scritto qui per `RFL_DB.dat`), cosi
   che i test Rust che li verificano non stiano confrontando il codice con se
   stesso.

Uso:

    python anonymize.py rksys <ingresso> <uscita>
    python anonymize.py rfldb <ingresso> <uscita>
"""

from __future__ import annotations

import hashlib
import struct
import sys
import zlib

# --- rksys.dat ------------------------------------------------------------
RKSYS_SIZE = 0x28000
RKSYS_MAGIC = b"RKSD0006"
RKPD_MAGIC = b"RKPD"
RKPD_SIZE = 0x8CC0
LICENSE_SLOTS = 4
LICENSE_NAME = 0x14
LICENSE_MII_ID = 0x28
LICENSE_SYSTEM_ID = 0x2C
LICENSE_PROFILE_ID = 0x5C
LICENSE_MII_BLOCK = 0x5680
FRIEND_MAIN = 0x56D0
FRIEND_STRIDE = 0x1C0
FRIEND_SECONDARY = 0x8B50
FRIEND_SECONDARY_STRIDE = 0x0C
FRIEND_SLOTS = 30
GLOBAL_CRC = 0x27FFC

# --- RFL_DB.dat -----------------------------------------------------------
RFLDB_SIZE = 0x1F1E0
RFLDB_FIRST_BLOCK = 0x04
RFLDB_SLOTS = 100
RFLDB_CRC = 0x1F1DE

# --- blocco Mii -----------------------------------------------------------
MII_BLOCK_SIZE = 74
MII_NAME = 0x02
MII_ID = 0x18
MII_SYSTEM_ID = 0x1C
MII_CREATOR = 0x36
MII_NAME_BYTES = 20

# Nomi che non identificano nessuno e restano com'erano.
KEEP = {"", "VanzaKart", "Mii"}

# Profile ID sintetici: il primo di un intervallo che non appartiene a nessuno.
SYNTHETIC_PID_BASE = 0x1000_0001
FC_SALT = b"JCMR"


def read_utf16(data: bytes, offset: int) -> str:
    raw = data[offset : offset + MII_NAME_BYTES]
    text = raw.decode("utf-16-be", "replace")
    return text.split("\x00")[0]


def friend_code_checksum(profile_id: int) -> int:
    buffer = struct.pack("<I", profile_id) + FC_SALT
    return (hashlib.md5(buffer).digest()[0] >> 1) & 0x7F


def crc16_ccitt(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 8
        crc &= 0xFFFF
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


class Anonymizer:
    """Raccoglie i valori personali e li sostituisce ovunque compaiano."""

    def __init__(self) -> None:
        self.names: dict[str, str] = {}
        self.words: dict[int, int] = {}

    def name(self, original: str) -> None:
        if original in KEEP or original in self.names:
            return

        index = len(self.names)
        units = len(original.encode("utf-16-be")) // 2
        tag = f"{index:02d}"
        if units >= 5:
            base = "Mii" + tag
        elif units == 4:
            base = "Mi" + tag
        elif units == 3:
            base = "M" + tag
        elif units == 2:
            base = tag
        else:
            base = chr(ord("A") + index % 26)
        self.names[original] = base[:units]

    def word(self, original: int, replacement: int) -> None:
        if original in (0, 0xFFFFFFFF) or original in self.words:
            return
        self.words[original] = replacement

    def apply(self, data: bytes) -> bytes:
        # Prima i nomi piu lunghi: uno corto puo essere prefisso di uno lungo.
        for original in sorted(self.names, key=len, reverse=True):
            source = original.encode("utf-16-be")
            target = self.names[original].encode("utf-16-be")
            target = target + b"\x00" * (len(source) - len(target))
            data = data.replace(source, target)

        for original, replacement in self.words.items():
            data = data.replace(struct.pack(">I", original), struct.pack(">I", replacement))

        return data

    def assert_clean(self, data: bytes) -> None:
        for original in self.names:
            assert original.encode("utf-16-be") not in data, f"nome superstite: {original!r}"
        for original in self.words:
            assert struct.pack(">I", original) not in data, f"valore superstite: {original:#x}"


def collect_mii_block(anon: Anonymizer, block: bytes, index: int) -> None:
    if not any(block):
        return
    anon.name(read_utf16(block, MII_NAME))
    anon.name(read_utf16(block, MII_CREATOR))
    # Il Mii ID della Wii ha il prefisso 0b100 sui bit alti: lo si conserva,
    # perche il gioco lo usa per distinguere un Mii creato su console.
    anon.word(struct.unpack(">I", block[MII_ID : MII_ID + 4])[0], 0x8000_0000 | (index + 1))
    anon.word(struct.unpack(">I", block[MII_SYSTEM_ID : MII_SYSTEM_ID + 4])[0], 0x0A00_0000 | (index + 1))


def anonymize_rksys(data: bytes) -> bytes:
    assert data[:8] == RKSYS_MAGIC, "non e un rksys.dat"
    assert len(data) >= RKSYS_SIZE, "file troppo corto"
    assert not any(data[RKSYS_SIZE:]), "oltre la dimensione utile ci sono dati"
    data = data[:RKSYS_SIZE]

    anon = Anonymizer()
    blocks = 0

    for slot in range(LICENSE_SLOTS):
        base = len(RKSYS_MAGIC) + slot * RKPD_SIZE
        if data[base : base + 4] != RKPD_MAGIC:
            continue

        anon.name(read_utf16(data, base + LICENSE_NAME))
        anon.word(
            struct.unpack(">I", data[base + LICENSE_PROFILE_ID : base + LICENSE_PROFILE_ID + 4])[0],
            SYNTHETIC_PID_BASE + slot,
        )
        anon.word(
            struct.unpack(">I", data[base + LICENSE_MII_ID : base + LICENSE_MII_ID + 4])[0],
            0x8000_0000 | (blocks + 1),
        )
        anon.word(
            struct.unpack(">I", data[base + LICENSE_SYSTEM_ID : base + LICENSE_SYSTEM_ID + 4])[0],
            0x0A00_0000 | (blocks + 1),
        )

        mii = data[base + LICENSE_MII_BLOCK : base + LICENSE_MII_BLOCK + MII_BLOCK_SIZE]
        collect_mii_block(anon, mii, blocks)
        blocks += 1

        for friend in range(FRIEND_SLOTS):
            pointer = base + FRIEND_MAIN + friend * FRIEND_STRIDE
            profile_id = struct.unpack(">I", data[pointer + 4 : pointer + 8])[0]
            if profile_id == 0:
                continue

            anon.word(profile_id, SYNTHETIC_PID_BASE + 0x100 + blocks)
            collect_mii_block(
                anon,
                data[pointer + 0x1A : pointer + 0x1A + MII_BLOCK_SIZE],
                blocks,
            )
            blocks += 1

    data = bytearray(anon.apply(data))
    anon.assert_clean(bytes(data))

    # Il checksum del friend code dipende dal profile ID: dopo la sostituzione
    # va ricalcolato, altrimenti la fixture porta amici con un codice non
    # valido e il test non distinguerebbe un bug da un dato incoerente.
    for slot in range(LICENSE_SLOTS):
        base = len(RKSYS_MAGIC) + slot * RKPD_SIZE
        if data[base : base + 4] != RKPD_MAGIC:
            continue
        for friend in range(FRIEND_SLOTS):
            pointer = base + FRIEND_MAIN + friend * FRIEND_STRIDE
            profile_id = struct.unpack(">I", data[pointer + 4 : pointer + 8])[0]
            if profile_id == 0:
                continue
            struct.pack_into(">I", data, pointer, friend_code_checksum(profile_id))

    crc = zlib.crc32(bytes(data[:GLOBAL_CRC])) & 0xFFFFFFFF
    struct.pack_into(">I", data, GLOBAL_CRC, crc)
    print(f"rksys: {blocks} blocchi Mii, {len(anon.names)} nomi, CRC {crc:#010x}")
    return bytes(data)


def anonymize_rfldb(data: bytes) -> bytes:
    assert len(data) >= RFLDB_SIZE, "file troppo corto"
    assert not any(data[RFLDB_SIZE:]), "oltre la dimensione utile ci sono dati"
    data = data[:RFLDB_SIZE]

    anon = Anonymizer()
    blocks = 0
    for index in range(RFLDB_SLOTS):
        offset = RFLDB_FIRST_BLOCK + index * MII_BLOCK_SIZE
        block = data[offset : offset + MII_BLOCK_SIZE]
        if not any(block):
            continue
        collect_mii_block(anon, block, index)
        blocks += 1

    data = bytearray(anon.apply(data))
    anon.assert_clean(bytes(data))

    crc = crc16_ccitt(bytes(data[:RFLDB_CRC]))
    struct.pack_into(">H", data, RFLDB_CRC, crc)
    print(f"RFL_DB: {blocks} Mii, {len(anon.names)} nomi, CRC {crc:#06x}")
    return bytes(data)


def main() -> int:
    if len(sys.argv) != 4 or sys.argv[1] not in {"rksys", "rfldb"}:
        print(__doc__)
        return 2

    kind, source, destination = sys.argv[1:]
    with open(source, "rb") as handle:
        data = handle.read()

    result = anonymize_rksys(data) if kind == "rksys" else anonymize_rfldb(data)

    with open(destination, "wb") as handle:
        handle.write(result)
    print(f"scritto {destination} ({len(result)} byte)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
