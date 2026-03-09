//! # Formatting specifiers
//!
//! ## For recipes:
//! - `%c`: total count of the recipe. If no goals are specified, this will be 1 for all
//!   recipes
//! - `%t`: time cost of the recipe
//! - `%r`: rate of the recipe (= 1 / t)
//! - `%R`: total rate of the recipe (= c * r)
//! - `%M`: machine class required for the recipe
//!
//! ## For ingredient/product edges
//! - `%c`: total count of the recipe. If no goals are specified, this will be 1 for all
//!   recipes
//! - `%t`: time cost of the recipe
//! - `%n`: amount of the ingredient produced or consumed by one instance of the recipe
//! - `%r`: rate of production or consumption of the ingredient by one instance of the recipe (= n
//!   / t)
//! - `%R`: total rate of production or consumption of the ingredient/product (= c * r)
//!
//! ## For item labels
//! - `%P`: total rate of production of the item (= sum(R for incoming edges))
//! - `%C`: total rate of consumption of the item (= sum(R for outgoing edges))
//! - `%R`: net rate of production or consumption of the item (= P - C)
//! - `%N`: name of the item
//! - `%S`: stack size of the item

use std::fmt;

use num_traits::cast::ToPrimitive;

use crate::{Error, recipes::Quantity};

#[derive(Clone, Default, Debug, Eq, PartialEq)]
pub struct FormatStr(Vec<FormatElement>);

impl FormatStr {
    pub fn format<'a>(&'a self, data: FormatData<'a>) -> FormatStrData<'a> {
        FormatStrData { data, format: self }
    }
}

impl fmt::Display for FormatStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.iter().try_for_each(|el| fmt::Display::fmt(el, f))
    }
}

impl std::str::FromStr for FormatStr {
    type Err = Error;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        if let Some(trimmed) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            s = trimmed;
        }
        let mut format = Vec::new();
        while let Some(idx) = s.find('%') {
            if idx > 0 {
                format.push(FormatElement::Literal(s[..idx].into()));
            }
            let (element, rest) = FormatElement::parse(&s[idx..]);
            format.push(element);
            s = rest;
        }
        if !s.is_empty() {
            format.push(FormatElement::Literal(s.into()));
        }
        Ok(Self(format))
    }
}

pub struct FormatStrData<'a> {
    data: FormatData<'a>,
    format: &'a FormatStr,
}

impl fmt::Display for FormatStrData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format
            .0
            .iter()
            .try_for_each(|el| el.format(self.data, false, f))
    }
}

impl fmt::Debug for FormatStrData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("\"")?;
        self.format
            .0
            .iter()
            .try_for_each(|el| el.format(self.data, true, f))?;
        f.write_str("\"")?;
        Ok(())
    }
}

macro_rules! format_element {
    (
        $(#[$meta:meta])*
        $vis:vis enum $ty:ident {
            $(
                $(#[$var_meta:meta])*
                $var:ident = $name:literal
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $ty {
            $(
                $(#[$var_meta])*
                $var,
            )*
        }

        impl $ty {
            pub fn as_escape(&self, precise: bool) -> &'static str {
                match self {
                    $(
                        Self::$var if precise => concat!("%", $name),
                        Self::$var => concat!("%~", $name),
                    )*
                }
            }

            pub fn from_escape(escape: &str) -> Option<(Self, bool, &str)> {
                $(
                    if let Some(rest) = escape.strip_prefix(concat!("%", $name)) {
                        return Some((Self::$var, true, rest))
                    }
                    if let Some(rest) = escape.strip_prefix(concat!("%~", $name)) {
                        return Some((Self::$var, false, rest))
                    }
                )*
                None
            }
        }
    };
}

format_element! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FormatElementKind {
        LiteralPercent = "%",
        Count = "c",
        Time = "t",
        Rate = "r",
        TotalRate = "R",
        RecipeIngredientCount = "n",
        Production = "P",
        Consumption = "C",
        Name = "N",
        MachineClass = "M",
        StackSize = "S",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatElement {
    Literal(Box<str>),
    Escape {
        kind: FormatElementKind,
        precise: bool,
    },
}

impl FormatElement {
    fn parse(s: &str) -> (Self, &str) {
        if let Some((kind, precise, rest)) = FormatElementKind::from_escape(s) {
            (Self::Escape { kind, precise }, rest)
        } else {
            match s.find('%') {
                Some(0) => (
                    Self::Escape {
                        kind: FormatElementKind::LiteralPercent,
                        precise: true,
                    },
                    &s[1..],
                ),
                Some(idx) => (Self::Literal(s[..idx].into()), &s[idx..]),
                None => (Self::Literal(s.into()), ""),
            }
        }
    }
}

impl fmt::Display for FormatElement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Literal(s) => f.write_str(s),
            Self::Escape { kind, precise } => f.write_str(kind.as_escape(*precise)),
        }
    }
}

impl FormatElement {
    fn format(&self, data: FormatData, debug: bool, f: &mut fmt::Formatter) -> fmt::Result {
        use self::FormatElementKind as Kind;
        match self {
            Self::Literal(s) => fmt::Display::fmt(&StrDebugDisplay(s, debug), f),
            Self::Escape {
                kind: Kind::LiteralPercent,
                precise: _,
            } => f.write_str("%"),
            Self::Escape {
                kind: Kind::Count,
                precise,
            } => self.display_or_escape(data.count, *precise, f),
            Self::Escape {
                kind: Kind::Time,
                precise,
            } => self.display_or_escape(data.time, *precise, f),
            Self::Escape {
                kind: Kind::Rate,
                precise,
            } => self.display_or_escape(data.rate(), *precise, f),
            Self::Escape {
                kind: Kind::TotalRate,
                precise,
            } => self.display_or_escape(data.total_rate(), *precise, f),
            Self::Escape {
                kind: Kind::RecipeIngredientCount,
                precise,
            } => self.display_or_escape(data.ingredient_count, *precise, f),
            Self::Escape {
                kind: Kind::Production,
                precise,
            } => self.display_or_escape(data.production, *precise, f),
            Self::Escape {
                kind: Kind::Consumption,
                precise,
            } => self.display_or_escape(data.consumption, *precise, f),
            Self::Escape {
                kind: Kind::Name,
                precise,
            } => self.display_or_escape(
                data.name.map(|name| StrDebugDisplay(name, debug)),
                *precise,
                f,
            ),
            Self::Escape {
                kind: Kind::MachineClass,
                precise,
            } => self.display_or_escape(
                data.machine_class.map(|name| StrDebugDisplay(name, debug)),
                *precise,
                f,
            ),
            Self::Escape {
                kind: Kind::StackSize,
                precise,
            } => self.display_or_escape(data.stack_size, *precise, f),
        }
    }

    fn display_or_escape(
        &self,
        item: Option<impl Formattable>,
        precise: bool,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        if let Some(item) = item {
            item.format(precise, f)
        } else {
            fmt::Display::fmt(self, f)
        }
    }
}

trait Formattable {
    fn format(&self, precise: bool, f: &mut fmt::Formatter) -> fmt::Result;
}

impl Formattable for StrDebugDisplay<'_> {
    fn format(&self, _precise: bool, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl Formattable for Quantity {
    fn format(&self, precise: bool, f: &mut fmt::Formatter) -> fmt::Result {
        if !precise && let Some(approx) = self.0.to_f64() {
            fmt::Display::fmt(&approx, f)
        } else {
            fmt::Display::fmt(self, f)
        }
    }
}

struct StrDebugDisplay<'a>(&'a str, bool);

impl fmt::Display for StrDebugDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.1 {
            fmt::Display::fmt(&self.0.escape_debug(), f)
        } else {
            fmt::Display::fmt(self.0, f)
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FormatData<'a> {
    pub count: Option<Quantity>,
    pub time: Option<Quantity>,
    pub ingredient_count: Option<Quantity>,
    pub production: Option<Quantity>,
    pub consumption: Option<Quantity>,
    pub name: Option<&'a str>,
    pub machine_class: Option<&'a str>,
    pub stack_size: Option<Quantity>,
}

impl FormatData<'_> {
    pub fn rate(&self) -> Option<Quantity> {
        Some(self.ingredient_count.unwrap_or(Quantity::ONE) / self.time?)
    }

    pub fn total_rate(&self) -> Option<Quantity> {
        self.count
            .and_then(|count| Some(count * self.rate()?))
            .or_else(|| Some(self.production? - self.consumption?))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn format_str() {
        let input = "hello %N! %%%c %k %%P";
        let f: FormatStr = input.parse().unwrap();
        assert_eq!(f.to_string(), "hello %N! %%%c %%k %%P");
    }
}
