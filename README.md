# bdat-rs
[![Crates.io Version](https://img.shields.io/crates/v/bdat)](https://crates.io/crates/bdat)
[![GitHub Release](https://img.shields.io/github/v/release/RoccoDev/bdat-rs?label=toolset%20release)](https://github.com/RoccoDev/bdat-rs/releases/latest)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/RoccoDev/bdat-rs/ci.yml)
[![docs.rs (with version)](https://img.shields.io/docsrs/bdat/latest)](https://docs.rs/bdat)


A library to read and write [MONOLITHSOFT](https://www.monolithsoft.co.jp/)'s proprietary BDAT format, used for data tables in all Xenoblade games.

## Usage

Refer to the [documentation](https://docs.rs/bdat) for detailed info on how to use the library.

You may also find other projects that use the library useful:

* This repository includes a [command-line tool](toolset/README.md) to convert BDAT tables to various formats.
* [Recordkeeper](https://github.com/RoccoDev/recordkeeper) is a save editor for Xenoblade 3 and Future Redeemed that reads game data from BDAT tables.

If you have a project that uses the library, feel free to add it to the list by submitting a PR.

## License

The bdat-rs library is dual-licensed under both [Apache-2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT).  
The bdat-toolset executable is licensed under the [GPLv3](toolset/LICENSE).

## Credits
* [vaxherd](https://github.com/vaxherd) for [xb3tool](https://github.com/vaxherd/xb3tool) and research on the Xenoblade 3 BDAT format