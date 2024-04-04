//! BDAT table, row, cell implementations
//!
//! Depending on how they were read, BDAT tables can either own their data source
//! or borrow from it.
//!
//! ## Accessing cells
//! The `row` function provides an easy interface to access cells.
//!
//! See also: [`RowRef`]
//!
//! ## Specialized views
//! If you know what type of BDAT tables you're dealing with (legacy or modern), you can use
//! [`as_modern`] and [`as_legacy`] to get specialized table views.
//!
//! These views return more ergonomic row accessors that let you quickly extract values, instead
//! of having to handle cases that are not supported by the known version.
//!
//! See also: [`ModernTable`], [`LegacyTable`]
//!
//!
//! [`RowRef`]: row::RowRef
//! [`as_legacy`]: CompatTable::as_legacy
//! [`as_modern`]: CompatTable::as_modern

pub(crate) mod builder;
pub(crate) mod cell;
pub(crate) mod column;
pub(crate) mod compat;
pub(crate) mod convert;
pub(crate) mod legacy;
pub(crate) mod modern;
pub(crate) mod private;
pub(crate) mod row;
pub(crate) mod util;
