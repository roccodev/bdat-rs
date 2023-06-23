# Legacy BDAT format

This document describes the data format used for BDAT files in games prior to Xenoblade Chronicles 3.

### Endianness

### Scrambled sections

### Cell types

## File header

| Field | Type |
| ----- | ---- |
| Table count | u32 |
| File size | u32 |
| Table offsets | u16 * table count

Tables must be ordered lexicographically by name, as the game runs a binary search to look up tables.

# Table structure

## Overview

| Section | Notes       |
| ------- |-------------|
| Header |             |
| Column info tables |             |
| Name table | *scrambled* |
| Column definitions | *scrambled* |
| Hash table |             |
| Row data |             |
| String table | *scrambled* |

Game expects the string table to mark the end of the table (for example, in `Bdat::calcCheckSumSub`)

## Header

| Field                   | Type              |
|-------------------------|-------------------|
| Magic (`b"BDAT"`) | u32 |
| Scramble type | u16 |
| Name table offset       | u16               |
| Size of each row, bytes | u16               |
| Hash table offset       | u16 |
| Hash table slot count (hash factor) | u16 |
| Row data offset | u16 |
| Number of rows | u16 |
| ID of the first row | u16 |
| ??? | u16 |
| Scramble key/Checksum | u16 |
| String table offset | u32 |
| String table size, bytes | u32 |
| Column definition table offset | u16 |
| Number of columns | u16 |
| Padding | total header size = 64 |


## Column info table

| Field                          | Type  | Offset |
|--------------------------------|-------|--------|
| Cell type (Flags, Value, List) | u8    | 0      |
| **<u>Flags Fields</u>**        | ----- |        |
| Flag right shift amount | u8 | 1      |
| Flag AND mask | u32 | 2 |
| Pointer to parent column definition | u16 | 6
| **<u>Value Fields</u>**               | ----- |        |
| Value type                     | u8 | 1      |
| Value offset (relative to row) | u16 | 2      |
| **<u>List Fields</u>**                | ----- |        |
| Value type                     | u8 | 1      |
| Value offset (relative to row) | u16 | 2      |
| Number of elements | u16 | 4      |


## Column definition

| Field | Type |
| ----- | ---- |
| Info table offset (absolute) | u16 |
| Same-hash linked node offset (absolute) | u16 |
| Name offset (absolute) | u16 |

## Hash table

The column hash table is a closed-addressing table of size `hash factor * 2` bytes.

Each slot is a `u16` containing either 0 or a pointer to a column definition. Collisions
are handled using separate chaining. When multiple columns hash to the same value, they are
linked together using the second field in the column definition. The field is a pointer to
another definition.

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
