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

use parse_display::{Display, FromStr};

use crate::recipes::{MachineClass, Quantity};

#[derive(Debug, Clone, clap::Args)]
pub struct FormatArgs {
    /// Format label for recipes.
    #[arg(short = 'R', long, default_value = "%ts")]
    pub recipe_label: FormatStr,
    /// Format label for edges.
    #[arg(short = 'E', long, default_value = "%n")]
    pub edge_label: FormatStr,
    /// Format label for items.
    #[arg(short = 'I', long, default_value = "%N")]
    pub item_label: FormatStr,
    /// Shape for recipe nodes.
    #[arg(long, default_value = "plain")]
    pub recipe_shape: String,
    /// Shape for item nodes.
    #[arg(long, default_value = "box")]
    pub item_shape: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
    type Err = std::convert::Infallible;

    fn from_str(mut s: &str) -> Result<Self, Self::Err> {
        let mut format = Vec::new();
        while let Some(idx) = s.find('%') {
            if idx > 0 {
                format.push(FormatElement::literal(&s[..idx]));
            }
            if let Some((esc, rest)) = s[idx..].split_at_checked(2)
                && let Ok(el) = esc.parse()
            {
                format.push(el);
                s = rest;
            } else {
                format.push(FormatElement::LiteralPercent);
                s = &s[idx + 1..];
            }
        }
        if !s.is_empty() {
            format.push(FormatElement::literal(s));
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
        self.format
            .0
            .iter()
            .try_for_each(|el| el.format(self.data, true, f))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Display, FromStr)]
pub enum FormatElement {
    #[display("{0}")]
    #[from_str(regex = "(?<0>[^%]*)", new = FormatElement::literal(String::as_str(&_0)))]
    Literal(Box<str>),
    #[display("%%")]
    LiteralPercent,
    #[display("%c")]
    Count,
    #[display("%t")]
    Time,
    #[display("%r")]
    Rate,
    #[display("%R")]
    TotalRate,
    #[display("%n")]
    RecipeIngredientCount,
    #[display("%P")]
    Production,
    #[display("%C")]
    Consumption,
    #[display("%N")]
    Name,
    #[display("%M")]
    MachineClass,
    #[display("%S")]
    StackSize,
}

impl FormatElement {
    fn literal(s: &str) -> Self {
        Self::Literal(s.into())
    }

    fn format(&self, data: FormatData, debug: bool, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Literal(s) => fmt::Display::fmt(&StrDebugDisplay(s, debug), f),
            Self::LiteralPercent => f.write_str("%"),
            Self::Count => self.display_or_escape(data.count, f),
            Self::Time => self.display_or_escape(data.time, f),
            Self::Rate => self.display_or_escape(data.rate(), f),
            Self::TotalRate => self.display_or_escape(data.total_rate(), f),
            Self::RecipeIngredientCount => self.display_or_escape(data.ingredient_count, f),
            Self::Production => self.display_or_escape(data.production, f),
            Self::Consumption => self.display_or_escape(data.consumption, f),
            Self::Name => {
                self.display_or_escape(data.name.map(|name| StrDebugDisplay(name, debug)), f)
            }
            Self::MachineClass => self.display_or_escape(data.machine_class, f),
            Self::StackSize => self.display_or_escape(data.stack_size, f),
        }
    }

    fn display_or_escape(
        &self,
        item: Option<impl fmt::Display>,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        if let Some(item) = item {
            fmt::Display::fmt(&item, f)
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
    pub machine_class: Option<MachineClass>,
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
