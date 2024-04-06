# Legacy BDAT format

This document describes the data format used for BDAT files in games prior to Xenoblade Chronicles 3.

### Endianness

XC1 (Wii) and XCX use big endian. XC3D/XC2/DE files are little-endian.

## File header

| Field | Type |
| ----- | ---- |
| Table count | u32 |
| File size | u32 |
| Table offsets | u16 * table count |

Tables must be ordered lexicographically by name, as the game runs a binary search to look up tables.

**Note**: in XC3D (3DS), the file size in the header can be **higher** than the actual size of the buffer (or file on disk). In all observed cases, the actual file size works well, so the header file size probably accounts for padding that is not included in file dumps. In XC1 (Wii) the opposite is often true, the actual file is usually bigger than the reported size, containing miscellaneous data at the end of the BDAT file.

# Table structure

In a table context, "absolute" pointers are relative to the start of the table.

## Overview

| Section                                       | Notes                                 |
|-----------------------------------------------|---------------------------------------|
| Header                                        |                                       |
| Column info tables (full section padded to 4) |                                       |
| Name table                                    | *scrambled*                           |
| Column nodes                                  | *scrambled*                           |
| Hash table (full section padded to 8)         |                                       |
| Row data (full section padded to 32)          |                                       |
| String table                                  | *scrambled*                           |
| Padding                                       | table size should be a multiple of 64 |

Game expects the string table to mark the end of the table (for example, in `Bdat::calcCheckSumSub`)

## Header

| Field | Type | Notes |
|-------|------|-------|
| Magic (`b"BDAT"`) | u8 * 4 | This is `b"TADB"` in XC3D (3DS). Magic is not affected by endianness in other games. |
| Flags (see below) | u8 | |
| ??? (0) | u8 | *Likely alignment for next field*. Not part of Flags, as it doesn't respect the byte order |
| Name table offset | u16 | |
| Size of each row, bytes | u16 | |
| Hash table offset | u16 | |
| Hash table slot count (hash factor) | u16 | Always 61, other values also supported |
| Row data offset | u16 | |
| Number of rows | u16 | |
| ID of the first row | u16 | |
| ??? (2) | u16 | |
| Scramble key/Checksum | u16 | |
| String table offset | u32 | |
| String table size, bytes + Final table padding | u32 | |
| **<u>The next fields are absent in XC1 (Wii) and XC3D (3DS) BDATs</u>** | <--                                 | <--                                                     |
| Column node table offset                                 | u16                                 |                                                         |
| Number of column nodes                                   | u16                                 |                                                         |
| Padding                                                  | total header size = 64 (32 for Wii/3DS) |                                                         |

### Known table flags

| Bit | Mask | Notes |
| --- | ---- | ----- |
| 0 | 0x1 | *Unknown*. Set in XC1, XC3D, XCX |
| 1 | 0x2 | 1 if scrambled |

## Scrambled sections

Certain sections can be "encrypted" using a checksum (see below) as the key. Below are some snippets
to scramble and unscramble sections.

The first scrambled section includes all bytes starting at the name table and ending before the first
byte of the hash table.  
The second section includes the entire string table.

### Unscramble (decrypt)

```cpp
void unscramble(char* start, char* end, u16 key) {
    u8 k1 = (key >> 8) ^ 0xff;
    u8 k2 = key ^ 0xff;
    // Note: with proper padding, the size of each scrambled
    // section should never be odd.
    while (start < end) {
        char a = *start;
        char b = *(start + 1);
        *(++start) ^= k1;
        *(++start) ^= k2;
        k1 += a;
        k2 += b;
    }
}
```

### Scramble (encrypt)

```cpp
void scramble(char* start, char* end, u16 key) {
    u8 k1 = (key >> 8) ^ 0xff;
    u8 k2 = key ^ 0xff;
    while (start < end) {
        char a = *start ^ k1;
        char b = *(start + 1) ^ k2;
        *(++start) = a;
        *(++start) = b;
        k1 += a;
        k2 += b;
    }
}
```

## Column info table

| Field                          | Type  | Offset |
|--------------------------------|-------|--------|
| Cell type (Flags, Value, List) | u8    | 0      |
| **<u>Value Fields</u>** (Cell type 1)       | ----- |        |
| Value type                     | u8 | 1      |
| Value offset (relative to row) | u16 | 2      |
| **<u>List Fields</u>** (Cell type 2)        | ----- |        |
| Value type                     | u8 | 1      |
| Value offset (relative to row) | u16 | 2      |
| Number of elements             | u16 | 4      |
| **<u>Flags Fields</u>** (Cell type 3)        | ----- |        |
| Flag right shift amount        | u8 | 1      |
| Flag AND mask                  | u32 | 2 |
| Pointer to parent column node  | u16 | 6


## Column node (XCX+)

| Field | Type |
| ----- | ---- |
| Info table offset (absolute) | u16 |
| Same-hash linked node offset (absolute) | u16 |
| Name offset (absolute) | u16 |

In XCX and newer BDATs, these live in their own section in the table.

## Column node (XC1 Wii/XC3D)

| Field | Type                                  |
| ---- |---------------------------------------|
| Info table offset (absolute) | u16                                   |
| Same-hash linked node offset (absolute) | u16                                   |
| Name | nul-terminated, padded-to-even string |

In XC1 (Wii) and XC3D BDATs, these nodes are used instead of column/flag names in the name table.

## String/name table

Strings are nul-terminated, stored contiguously, and padded to even length (after including the null byte).  
Name tables are similar, but they only store the table name (always in the first slot), and the column/flag names.

The name table is a bit different in XC1 (Wii)/XC3D, as it stores column nodes as well. The table name is still
a plain string, not a column node.

## Rows and value types

Rows are stored sequentially. "Flag" cells are skipped, as they use the parent cell's value.
For "List" cells (cell type 2), values are stored sequentially.

Legacy BDATs support these value types:

| ID | Type | Size (bytes) | Notes |
| -- | ---- | ------------ | ----- |
| 1 | Unsigned Byte | 1 | |
| 2 | Unsigned Short | 2 | |
| 3 | Unsigned Int | 4 | | 
| 4 | Signed Byte | 1 | |
| 5 | Signed Short | 2 | |
| 6 | Signed Int | 4 | |
| 7 | String | 4 | pointer to a nul-terminated C string (absolute) |
| 8 | Float | 4 | IEEE-754 floating point (1/3D/2/DE), `20.12` fixed-point (`f = raw / 4096.0`) (XCX) |

### Flag cells

Flag cells (cell type 3) do not consume space on their own. Instead, they read bits from their parent column's value. The parent column must then have a single-value integer type. 

Generally, flags are used to read boolean values, but wider flags are also supported.

## Hash table

The column hash table is a closed-addressing table of size `hash factor * 2` bytes.

Each slot is a `u16` containing either 0 or a pointer to a column node. Collisions
are handled using separate chaining. When multiple columns hash to the same value, they are
linked together using the second field in the column node. The field is a pointer to
another node.

The hash function is the following:
```cpp
int hash(char* str, int len) {
    if (len == 0) {
        return 0;
    }
    int hash = *str;
    for (int i = 1; i < 8 && i < len; i++) {
        hash = hash * 7 + *(++str);
    }
    return hash;
}
```

Collisions are very common, as strings with at least the first 8 characters in common will always
hash to the same value.

## Checksum

The game can calculate a checksum of the table, though currently no game verifies the integrity of tables.
(XC1 and XCX don't even link the function)

The checksum is currently used as the scramble key. It can be calculated using this code, for example:

```cpp
// Note: tableLen should be string table offset + string table size
unsigned short checksum(char* tableStart, int tableLen) {
    unsigned short checksum = 0;
    // Start index is 0x20 for all games, meaning checksums in
    // XCX+ also consider part of the header
    for (int i = 0x20; i < tableLen; i++) {
        checksum += tableStart[i] << (i & 3);
    }
    return checksum;
}
```