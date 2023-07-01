use crate::io::BdatVersion;
use crate::Utf;
use std::borrow::Cow;
use std::{cmp::Ordering, fmt::Display};

/// A name for a BDAT element (table, column, ID, etc.)
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Label {
    /// 32-bit hash, notably used in [`BdatVersion::Modern`] BDATs.
    Hash(u32),
    /// Plain-text string, used in older BDAT formats.
    String(String),
    /// Equivalent to [`Label::String`], but it is made explicit that the label
    /// was originally hashed.
    Unhashed(String),
}

impl Label {
    /// Extracts a [`Label`] from a [`String`].
    ///
    /// The format is as follows:  
    /// * `<01ABCDEF>` (8 hex digits) => `Label::Hash(0x01abcdef)`
    /// * s => `Label::String(s)`
    ///
    /// If `force_hash` is `true`, the label will be re-hashed
    /// if it is either [`Label::String`] or [`Label::Unhashed`].
    pub fn parse<'a, S: Into<Utf<'a>>>(text: S, force_hash: bool) -> Self {
        let text = text.into();
        if text.len() == 10 && text.as_bytes()[0] == b'<' {
            if let Ok(n) = u32::from_str_radix(&text[1..=8], 16) {
                return Label::Hash(n);
            }
        }
        if force_hash {
            Label::Hash(crate::hash::murmur3_str(&text))
        } else {
            Label::String(text.into_owned())
        }
    }

    /// If needed, turns the label into a hashed label.
    pub fn into_hash(self, version: BdatVersion) -> Self {
        if !version.are_labels_hashed() {
            return self;
        }
        match self {
            l @ Self::Hash(_) => l,
            Self::String(s) | Self::Unhashed(s) => Self::Hash(crate::hash::murmur3_str(&s)),
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
            Self::String(s) | Self::Unhashed(s) => Cow::Borrowed(s.as_str()),
            _ => Cow::Owned(self.to_string()),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::String(s) | Self::Unhashed(s) => s.as_str(),
            _ => panic!("label is not a string"),
        }
    }
}

impl From<String> for Label {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<u32> for Label {
    fn from(hash: u32) -> Self {
        Self::Hash(hash)
    }
}

impl Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash(hash) => {
                if f.sign_plus() {
                    write!(f, "{:08X}", hash)
                } else {
                    write!(f, "<{:08X}>", hash)
                }
            }
            Self::String(s) | Self::Unhashed(s) => write!(f, "{}", s),
        }
    }
}
