# BDAT conversion toolset
A set of tools using the `bdat` library, that allow users to convert to/from BDAT files.

## Supported formats
The toolset supports conversion from and to these formats:  
* **JSON** (read & write)
* CSV (read only)

## Examples
Print a table's structure
```sh
bdat-toolset info file.bdat -t TableName
```

Extract all tables from `file.bdat` into the `output` directory (in JSON format)
```sh
bdat-toolset extract file.bdat -o output -f json --pretty
```

Convert the extracted JSON tables back into BDAT
```sh
bdat-toolset pack json_files_dir -o bdat_output_dir

```
