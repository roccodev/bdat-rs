//! Optionally hashed labels used as table and column names

use crate::io::BdatVersion;
use crate::Utf;
use std::borrow::Cow;
use std::{cmp::Ordering, fmt::Display};

/// The label is hashed and an operation on a plain string (e.g. comparison) was requested.
#[derive(thiserror::Error, Debug)]
#[error("label is not a string")]
pub struct LabelNotStringError;

/// A name for a BDAT element (table, column, ID, etc.)
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Label<'buf> {
    /// 32-bit hash, notably used in [`BdatVersion::Modern`] BDATs.
    Hash(u32),
    /// Plain-text string, used in older BDAT formats.
    String(Utf<'buf>),
}

impl<'buf> Label<'buf> {
    /// Extracts a [`Label`] from a [`String`].
    ///
    /// The format is as follows:  
    /// * `<01ABCDEF>` (8 hex digits) => `Label::Hash(0x01abcdef)`
    /// * s => `Label::String(s)`
    ///
    /// If `force_hash` is `true`, the label will be re-hashed
    /// if it is [`Label::String`].
    pub fn parse<S: Into<Utf<'buf>>>(text: S, force_hash: bool) -> Self {
        let text = text.into();
        if text.len() == 10 && text.as_bytes()[0] == b'<' {
            if let Ok(n) = u32::from_str_radix(&text[1..=8], 16) {
                return Label::Hash(n);
            }
        }
        if force_hash {
            Label::Hash(crate::hash::murmur3_str(&text))
        } else {
            Label::String(text)
        }
    }

    /// If needed, turns the label into a hashed label.
    pub fn into_hash(self, version: BdatVersion) -> Self {
        if !version.are_labels_hashed() {
            return self;
        }
        match self {
            l @ Self::Hash(_) => l,
            Self::String(s) => Self::Hash(crate::hash::murmur3_str(&s)),
        }
    }

    /// Comparison function for the underlying values.
    ///
    /// Unlike a typical [`Ord`] implementation for enums, this only takes values into consideration
    /// (though hashed labels are still considered separately), meaning the following holds:
    ///
    /// ```rs
    /// use bdat::Label;
    /// use std::cmp::Ordering;
    ///
    /// assert_eq!(Label::Hash(0x0).cmp_value(&Label::Hash(0x0)), Ordering::Equal);
    /// assert_eq!(Label::String("Test".to_string()).cmp_value(&Label::String("Test".to_string())), Ordering::Equal);
    /// // and...
    /// assert_eq!(Label::String("Test".to_string()).cmp_value(&Label::Unhashed("Test".to_string())), Ordering::Equal);
    /// // ...but not
    /// assert_ne!(Label::String(String::new()).cmp_value(&Label::Hash(0x0)), Ordering::Equal);
    /// ```
    pub fn cmp_value(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Hash(slf), Self::Hash(oth)) => slf.cmp(oth),
            (_, Self::Hash(_)) => Ordering::Less, // hashed IDs always come last
            (Self::Hash(_), _) => Ordering::Greater,
            (a, b) => a.as_str().cmp(b.as_str()),
        }
    }

    /// An alternative to [`ToString::to_string`] that returns a reference to the label if it's
    /// already a string.
    pub fn to_string_convert(&self) -> Utf {
        match self {
            Self::String(s) => Cow::Borrowed(s.as_ref()),
            _ => Cow::Owned(self.to_string()),
        }
    }

    /// Clones the string value to give it a `'static` lifetime, if the label is a string.
    pub fn into_owned(self) -> Label<'static> {
        match self {
            Label::Hash(h) => Label::Hash(h),
            Label::String(s) => Label::String(s.into_owned().into()),
        }
    }

    /// Converts from `&'a Label<'buf>` to `Label<'a>`.
    ///
    /// If this is a hashed label, the hash is copied. Otherwise,
    /// if this is a string label, the string is borrowed.
    pub fn as_ref(&self) -> Label {
        match self {
            Self::Hash(h) => Label::Hash(*h),
            Self::String(s) => Label::String(s.as_ref().into()),
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        self.try_into().expect("label is not a string")
    }
}

impl<'a> From<&'a Label<'_>> for Label<'a> {
    fn from(value: &'a Label) -> Self {
        value.as_ref()
    }
}

impl<'buf> From<&'buf str> for Label<'buf> {
    fn from(s: &'buf str) -> Self {
        Self::String(s.into())
    }
}

impl<'buf> From<String> for Label<'buf> {
    fn from(s: String) -> Self {
        Self::String(s.into())
    }
}

impl<'buf> From<Utf<'buf>> for Label<'buf> {
    fn from(value: Utf<'buf>) -> Self {
        Self::String(value)
    }
}

impl<'buf> From<u32> for Label<'buf> {
    fn from(hash: u32) -> Self {
        Self::Hash(hash)
    }
}

impl<'s> TryFrom<Label<'s>> for Utf<'s> {
    type Error = LabelNotStringError;

    fn try_from(value: Label<'s>) -> Result<Self, Self::Error> {
        match value {
            Label::String(s) => Ok(s),
            _ => Err(LabelNotStringError),
        }
    }
}

impl<'s> TryFrom<&'s Label<'s>> for &'s str {
    type Error = LabelNotStringError;

    fn try_from(value: &'s Label<'s>) -> Result<Self, Self::Error> {
        match value {
            Label::String(s) => Ok(s.as_ref()),
            _ => Err(LabelNotStringError),
        }
    }
}

impl<'buf> Display for Label<'buf> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash(hash) => {
                if f.sign_plus() {
                    write!(f, "{:08X}", hash)
                } else {
                    write!(f, "<{:08X}>", hash)
                }
            }
            Self::String(s) => write!(f, "{}", s),
        }
    }
}
