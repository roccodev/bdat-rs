# Modern BDAT format

This document describes the data format used for BDAT files in Xenoblade Chronicles 3 and Xenoblade Chronicles X: Definitive Edition.

## Hashed labels

All table and column names are hashed using the 32-bit version of Murmur3.

## File header

| Field                | Type              |
|----------------------|-------------------|
| Magic (`b"BDAT"`)    | u32               |
| Version? (must be 4) | u8                |
| ??? (16)             | u16               |
| ??? (1)              | u8                |
| Table count          | u32               |
| File size            | u32               |
| Table offsets        | u32 * table count |

Tables in each file are in no particular order.

# Table structure

## Overview

| Section              |
|----------------------|
| Header               |
| Row ID -> Index table |
| Column info          |
| Row data |
| String table |
| Padding (table size padded to 4) |

## Header

| Field                                          | Type |
|------------------------------------------------|------|
| Magic (`b"BDAT"`)                              | u32  |
| Version? (must be 4)                           | u8   |
| ??? (48)                                       | u24  |
| Number of columns                              | u32  |
| Number of rows                                 | u32  |
| ID of the first row                            | u32  |
| ??? (0)                                        | u32  |
| Column info table offset                       | u32  |
| Row ID table offset                   | u32  |
| Row data offset                                | u32  |
| Size of a single row, bytes                    | u32  |
| String table offset                            | u32  |
| String table size, bytes | u32  |

## Row ID table

Most if not all tables have a `Murmur3 Hash` column (usually called `ID` or `label`). If that is the case,
a table should be included that maps those hashed IDs to row indices.

Each entry is a `(u32, u32)` pair, in which the first element is the 32-bit hash from the ID field,
and the second element is the row index (that always starts at 0, i.e. `Row ID - Base ID`).

Pairs must be ordered by hash, as the game runs a binary search to look them up.

In 3, there must not be duplicate hashes, and there must be an entry for each row. This is relaxed
in XDE (likely to ease porting of X tables): duplicates are still not allowed, but the table may
be incomplete. In XDE, the row ID table has its size bounded by the start of the row section. In 3,
the game reads as many entries as there are rows.

## Column info

Column names are always hashed. When reading the column name, a 32-bit value should be expected.

| Field                                           | Type |
|-------------------------------------------------| ----- |
| Value type ID (see below)                       | u8 |
| Name pointer (hashed, relative to string table) | u32 |

This structure is repeated for each column.

## Row data and values

Unlike legacy BDATs, there are no flag or list cells (see below).  
Rows are stored sequentially. Each cell is represented as follows, depending on value type:

| ID | Type | Size (bytes) | Notes |
| -- | ---- | ------------ | ----- |
| 1 | Unsigned Byte | 1 | |
| 2 | Unsigned Short | 2 | |
| 3 | Unsigned Int | 4 | |
| 4 | Signed Byte | 1 | |
| 5 | Signed Short | 2 | |
| 6 | Signed Int | 4 | |
| 7 | String | 4 | pointer to a nul-terminated UTF-8 string (relative to string table) |
| 8 | Float | 4 | IEEE-754 floating point |
| 9 | Murmur3 Hash | 4 | murmur3 (32bit) hashed ID |
| 10 | Percent | 1 | `v = raw * 0.01` |
| 11 | Debug String | 4 | same as String, used for debug columns like `DebugName` |
| 12 | Unknown | 1 | |
| 13 | MessageStudio Index? | 2 | Used for most `Name` and `Caption` fields, that point to message tables |

In XDE, floats seem to have been adapted from their original precise source values (in X, they were `20.12` fixed-points), as most of them cannot be recreated from trivial `/ 4096.0` operations on the XCX BDATs.

### Alternative flag/list cell representation

In XDE, tables from the original game were adapted to use the new column representation. The game still accesses fields using the original structure though, here's how the conversion is implemented.

Flag cells are now converted to boolean (`Unsigned Byte` 0/1) columns named with the format `{parent}({field})`. For example, the `is_friendly` field from the `flags` parent column is now a separate column called `flags(is_friendly)`.

List cells are converted to multiple columns of the same type, named with the format `{name}[{index}]`. For example, the `attributes` list cell with 5 elements has turned into 5 columns named `attributes[0]`, `attributes[1]`, `attributes[2]`, `attributes[3]`, and `attributes[4]`.

## String table

The string table is a sequence of hashes and strings.

If the first byte of the string table is zero, then the table and column names are hashed. Otherwise,
no additional first byte is added, and they are represented as nul-terminated C strings.

Strings from row values are always in plain text.

The table name is always the first entry.  
The second entry is also reserved. Language BDATs leave it empty (0), but it is populated in game BDATs (it's possibly a debug name).

## Debug sections (1.3.0, XDE)

Some files in XC3 1.3.0 and XDE have debug sections left inside them.

Debug sections aren't referenced by any offset, but they are found at index 0x30 (right after the header, where
column info would usually be).

In XDE, some functions load the string but immediately discard it, suggesting it might have been
used for e.g. logging.

Debug sections follow this format:

| Field | Type | Notes                               |
| ----- |------|-------------------------------------|
| ID | u32  | 1 for Row debug, 2 for Column debug |
| Section size | u32 | Includes ID and this field          |

Then, Section 1 follows with this:

| Field         | Type                     | Notes                                                                                                         |
|---------------|--------------------------|---------------------------------------------------------------------------------------------------------------|
| Name pointers | u32                      | Pointers to nul-terminated strings at `section + 8 + ptr`, repeated for each column, but not for each row (?) |
| Strings       | nul-terminated C-strings | see above, strings are *unhashed* row IDs, also contains strings not referenced by the above pointer          |

while Section 2 follows with this:

| Field        | Type                     | Notes                                                    |
|--------------|--------------------------|----------------------------------------------------------|
| Strings | nul-terminated C-strings | see above, strings are *unhashed* table and column names |